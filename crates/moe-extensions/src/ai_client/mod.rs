//! The three-protocol client layer (ADR-0040): one shared delta vocabulary, one adapter per
//! protocol, dispatch by [`Protocol`].
//!
//! Adapters are pure functions in the existing style: an endpoint URL, a request body built from
//! OpenAI-shaped wire messages (ADR-0010), an SSE line parser, and a blocking-response text
//! extractor. `openai-chat` is today's code, kept byte-compatible — MOE-0026's tool loop and its
//! dsh replay economics depend on the wire staying exactly as it was.

pub mod anthropic;
pub mod openai_chat;
pub mod openai_responses;
pub mod providers;

pub use moe_core::conversation::ToolCall;
pub use openai_chat::{
    chat_body, chat_body_from_messages, chat_body_with_stream, chat_body_with_tools,
    chat_completions_url, sse_delta,
};

use moe_platform::config::{Protocol, ReasoningSetting};

/// Outcome of processing one SSE line — the vocabulary every adapter normalizes into
/// (text / reasoning / tool-call fragment / done), so `ai.rs` stays protocol-agnostic.
#[derive(Debug, PartialEq, Eq)]
pub enum SseLine {
    /// Incremental text.
    Delta(String),
    /// Incremental reasoning (`reasoning_content`, Anthropic `thinking_delta`, Responses
    /// reasoning-summary deltas).
    Reasoning(String),
    /// The reasoning block's provider signature (Anthropic `signature_delta`, MOE-0035): replayed
    /// verbatim beside the reasoning text — MiniMax requires it "unchanged in later turns".
    ReasoningSignature(String),
    /// Tool-call fragments; accumulate by index via [`merge_tool_call_fragments`] (the request must
    /// have advertised tools — MOE-0026; produced by `openai-chat` and `anthropic`, MOE-0035).
    ToolCalls(Vec<ToolCallFragment>),
    /// Stream end (`[DONE]`, `message_stop`, `response.completed`).
    Done,
    /// Ignore (heartbeats, empty lines, non-JSON, protocol events without text, etc.).
    Ignore,
}

/// One streamed tool-call fragment: the first chunk of a call carries `id` and the function
/// `name`; later chunks carry only `arguments` slices, keyed by `index`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCallFragment {
    pub index: u64,
    pub id: Option<String>,
    pub name: Option<String>,
    pub arguments: String,
}

/// Merge streamed fragments into complete calls, one per index, in index order: `id`/`name` come
/// from the fragment that carries them; `arguments` concatenates in arrival order (MOE-0026).
pub fn merge_tool_call_fragments(fragments: &[ToolCallFragment]) -> Vec<ToolCall> {
    let mut merged: std::collections::BTreeMap<u64, ToolCall> = std::collections::BTreeMap::new();
    for fragment in fragments {
        let call = merged.entry(fragment.index).or_insert_with(|| ToolCall {
            id: String::new(),
            name: String::new(),
            arguments: String::new(),
        });
        if let Some(id) = &fragment.id {
            call.id.clone_from(id);
        }
        if let Some(name) = &fragment.name {
            call.name.clone_from(name);
        }
        call.arguments.push_str(&fragment.arguments);
    }
    merged.into_values().collect()
}

/// One request in the shared vocabulary: messages are OpenAI-shaped wire messages (multimodal
/// parts with attachments, ADR-0010); each adapter normalizes them into its protocol's body.
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<serde_json::Value>,
    pub stream: bool,
    /// What the request asks of the model's reasoning (MOE-0062): the protocol's own default,
    /// off, or an explicit effort. Resolved from the per-command setting when there is one, else
    /// from the model's catalog flag / the provider block's override (ADR-0040).
    pub reasoning: Reasoning,
    /// Sampling temperature (MOE-0062): `None` = the key is not sent at all, which is the only
    /// safe default — several reasoning models reject an explicit one.
    pub temperature: Option<f64>,
    /// Anthropic requires `max_tokens`; the resolver clamps it to the catalogued output limit.
    pub max_tokens: u64,
    /// The advertised tool definitions in OpenAI function shape; each adapter normalizes them
    /// (Anthropic → `{name, description, input_schema}`, MOE-0035). Empty = no tools.
    pub tools: Vec<serde_json::Value>,
    /// The Anthropic adapter's `thinking` dialect (MOE-0035), resolved per provider in `ai.rs`;
    /// the other protocols ignore it.
    pub anthropic_thinking: AnthropicThinking,
}

/// What a request asks of the model's reasoning (MOE-0062), in one protocol-neutral vocabulary:
/// the adapters map it to their own knob, and the ones without a knob (OpenAI Chat Completions)
/// treat `Off` and `Default` alike — reasoning there is a property of the model, not of the request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reasoning {
    /// Ask for no reasoning at all (every dialect's parameter is omitted).
    Off,
    /// The provider's own default: today's behavior, i.e. the catalog/block flag's dialect.
    Default,
    /// An explicit effort (the settings window's per-command choice).
    Effort(ReasoningSetting),
}

impl Reasoning {
    /// The effective setting for a run: the command's own when it has one, else the provider's
    /// resolved default (`on` = the dialect's default reasoning, `off` = nothing).
    pub fn resolve(provider_default: bool, setting: Option<ReasoningSetting>) -> Self {
        match setting {
            Some(ReasoningSetting::Off) => Self::Off,
            Some(effort) => Self::Effort(effort),
            None if provider_default => Self::Default,
            None => Self::Off,
        }
    }

    /// Whether reasoning is asked for at all (the `Off` reading is what turns every dialect off).
    pub fn is_on(self) -> bool {
        !matches!(self, Self::Off)
    }

    /// The explicit effort, when the request names one (`Off` is not an effort: it is
    /// [`Reasoning::Off`], and a level is never sent as `off`).
    pub fn effort(self) -> Option<ReasoningSetting> {
        match self {
            Self::Effort(setting) if setting != ReasoningSetting::Off => Some(setting),
            _ => None,
        }
    }
}

/// Which `thinking` parameter the Anthropic adapter sends (MOE-0035): MiniMax's surface
/// whitelists only `disabled|adaptive` — and M3.1 even 400s on `disabled`, so "off" must *omit*
/// the key — while legacy Anthropic models want `enabled` + `budget_tokens`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnthropicThinking {
    /// No `thinking` key at all.
    #[default]
    Omit,
    /// `{"type":"enabled","budget_tokens":N}` (legacy Anthropic surface).
    EnabledBudget(u64),
    /// `{"type":"adaptive"}` (MiniMax's Anthropic-compatible surface).
    Adaptive,
}

/// The protocol's endpoint: each adapter appends its canonical path to the base URL.
pub fn endpoint_url(protocol: Protocol, api_url: &str) -> String {
    match protocol {
        Protocol::OpenAiChat => openai_chat::chat_completions_url(api_url),
        Protocol::OpenAiResponses => openai_responses::endpoint_url(api_url),
        Protocol::Anthropic => anthropic::endpoint_url(api_url),
    }
}

/// One request → the protocol's request body.
pub fn request_body(protocol: Protocol, request: &ChatRequest) -> serde_json::Value {
    match protocol {
        Protocol::OpenAiChat => openai_chat::body(request),
        Protocol::OpenAiResponses => openai_responses::body(request),
        Protocol::Anthropic => anthropic::body(request),
    }
}

/// One SSE line → the shared delta vocabulary.
pub fn sse_line(protocol: Protocol, line: &str) -> SseLine {
    match protocol {
        Protocol::OpenAiChat => openai_chat::sse_delta(line),
        Protocol::OpenAiResponses => openai_responses::sse_delta(line),
        Protocol::Anthropic => anthropic::sse_delta(line),
    }
}

/// A blocking (non-streaming) response body → its text (the AI commands invoke path, ADR-0024).
pub fn completion_text(protocol: Protocol, payload: &str) -> Option<String> {
    match protocol {
        Protocol::OpenAiChat => openai_chat::completion_text(payload),
        Protocol::OpenAiResponses => openai_responses::completion_text(payload),
        Protocol::Anthropic => anthropic::completion_text(payload),
    }
}

/// Attach the protocol's authentication: bearer for the OpenAI dialects, `x-api-key` plus the
/// `anthropic-version` header for Anthropic Messages.
pub fn authenticate(
    protocol: Protocol,
    request: reqwest::blocking::RequestBuilder,
    key: &str,
) -> reqwest::blocking::RequestBuilder {
    match protocol {
        Protocol::OpenAiChat | Protocol::OpenAiResponses => request.bearer_auth(key),
        Protocol::Anthropic => request
            .header("x-api-key", key)
            .header("anthropic-version", anthropic::API_VERSION),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Index accumulation assembles complete calls from fragments, in index order (MOE-0026).
    #[test]
    fn merges_tool_call_fragments_in_index_order() {
        let fragments = vec![
            ToolCallFragment {
                index: 1,
                id: Some("call_b".into()),
                name: Some("web_fetch".into()),
                arguments: String::new(),
            },
            ToolCallFragment {
                index: 0,
                id: Some("call_a".into()),
                name: Some("web_search".into()),
                arguments: "{\"query\":\"ru".into(),
            },
            ToolCallFragment {
                index: 1,
                id: None,
                name: None,
                arguments: "{\"url\":\"https://x\"}".into(),
            },
            ToolCallFragment {
                index: 0,
                id: None,
                name: None,
                arguments: "st\"}".into(),
            },
        ];
        assert_eq!(
            merge_tool_call_fragments(&fragments),
            vec![
                ToolCall {
                    id: "call_a".into(),
                    name: "web_search".into(),
                    arguments: "{\"query\":\"rust\"}".into(),
                },
                ToolCall {
                    id: "call_b".into(),
                    name: "web_fetch".into(),
                    arguments: "{\"url\":\"https://x\"}".into(),
                },
            ]
        );
        assert!(merge_tool_call_fragments(&[]).is_empty());
    }
}
