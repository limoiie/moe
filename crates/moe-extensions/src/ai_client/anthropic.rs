//! The Anthropic Messages adapter (ADR-0040): endpoint URL, request body, SSE parsing, and the
//! blocking-response text extractor.
//!
//! Wire notes (docs.anthropic.com, `POST {api_url}/v1/messages`):
//! - `x-api-key: <key>` + `anthropic-version: 2023-06-01` (see `mod::authenticate`); no bearer.
//! - `system` messages are **not** in `messages`: they concatenate into the top-level `system`
//!   string. `messages` carry only `user`/`assistant` roles with content parts
//!   (`{"type":"text","text":…}`, images as `{"type":"image","source":{"type":"base64",
//!   "media_type":…,"data":…}}` — the shared wire form is a `data:` URL, `image_url.url`).
//! - `max_tokens` is required (the resolver passes it).
//! - The reasoning opt-in is the `thinking` parameter in the dialect [`AnthropicThinking`] names
//!   (MOE-0035): legacy Anthropic `thinking: {"type":"enabled","budget_tokens":N}` with
//!   `1024 <= budget < max_tokens`, or MiniMax's `{"type":"adaptive"}` — their surface whitelists
//!   only `disabled|adaptive` (M3.1 rejects even `disabled`, so reasoning-off omits the key).
//!   Thinking arrives as `content_block_delta` with `delta.type == "thinking_delta"`, its
//!   signature as `signature_delta` (replayed verbatim in later turns — MiniMax requires it
//!   "unchanged", real Anthropic 400s on altered blocks mid-tool-use).
//! - Tools (MOE-0035): specs convert to `{name, description, input_schema}`; a call streams as
//!   `content_block_start` (`tool_use`, id+name) + `input_json_delta` argument fragments (MiniMax
//!   may deliver the complete `input` in the start event instead — both paths are handled); the
//!   turn ends with `stop_reason: "tool_use"`. Results ride **user** messages as `tool_result`
//!   blocks, immediately after the assistant turn, results first in the content array; `is_error`
//!   is never sent (absent from MiniMax's schema — error text is prefixed by the runner instead).
//! - SSE lines are `data: {json}` where `json.type` is the event; `message_stop` ends the stream.

use super::{AnthropicThinking, ChatRequest, SseLine, ToolCallFragment};

/// The `anthropic-version` header every request carries.
pub const API_VERSION: &str = "2023-06-01";

pub fn endpoint_url(api_url: &str) -> String {
    let base = api_url.trim_end_matches('/');
    // A catalogued base may already carry the version root: MiniMax's Anthropic-compatible
    // surface is `https://api.minimax.io/anthropic/v1` (models.dev; MOE-0031), where the
    // endpoint is `…/v1/messages` — an existing `/v1` suffix is never duplicated.
    if base.ends_with("/v1") {
        format!("{base}/messages")
    } else {
        format!("{base}/v1/messages")
    }
}

/// Request body (module notes): `model`/`stream`/`max_tokens`/`messages`, the top-level `system`
/// string (every `system` message's text, blank-line joined; the key is absent when there is
/// none), the `thinking` opt-in per the [`AnthropicThinking`] dialect, and — when the request
/// advertises any — `tools` in the Anthropic shape (MOE-0035). OpenAI-shaped wire messages are
/// normalized: an assistant's `reasoning_content` (+ `reasoning_signature`) becomes a leading
/// `thinking` block, its `tool_calls` become `tool_use` blocks, and each run of `tool` messages
/// folds into `tool_result` blocks on the next user message (results first — Anthropic's pairing
/// rule; a run at the record's end becomes a standalone user message).
pub(super) fn body(request: &ChatRequest) -> serde_json::Value {
    let mut system: Vec<String> = Vec::new();
    let mut messages: Vec<serde_json::Value> = Vec::new();
    let mut pending_results: Vec<serde_json::Value> = Vec::new();
    // An open `tool` run closes as one user message of `tool_result` blocks (parallel calls).
    let flush_results = |messages: &mut Vec<serde_json::Value>,
                         pending: &mut Vec<serde_json::Value>| {
        if !pending.is_empty() {
            messages.push(serde_json::json!({
                "role": "user",
                "content": std::mem::take(pending),
            }));
        }
    };
    for message in &request.messages {
        match message["role"].as_str() {
            Some("tool") => pending_results.push(serde_json::json!({
                "type": "tool_result",
                "tool_use_id": message["tool_call_id"].as_str().unwrap_or_default(),
                "content": message["content"].as_str().unwrap_or_default(),
            })),
            // System text rides the top-level `system` string, never `messages` (images in a
            // system message cannot be represented there and are dropped).
            Some("system") => system.extend(
                content_blocks(&message["content"])
                    .iter()
                    .filter_map(|block| block["text"].as_str())
                    .map(str::to_string),
            ),
            Some("assistant") => {
                flush_results(&mut messages, &mut pending_results);
                let blocks = assistant_blocks(message);
                if !blocks.is_empty() {
                    messages.push(serde_json::json!({ "role": "assistant", "content": blocks }));
                }
            }
            Some("user") => {
                // Pending tool results merge into the next user message, results first (the
                // protocol's ordering rule); with nothing pending this is the plain path.
                let mut blocks = std::mem::take(&mut pending_results);
                blocks.extend(content_blocks(&message["content"]));
                if !blocks.is_empty() {
                    messages.push(serde_json::json!({ "role": "user", "content": blocks }));
                }
            }
            _ => {}
        }
    }
    flush_results(&mut messages, &mut pending_results);
    let mut body = serde_json::json!({
        "model": request.model,
        "stream": request.stream,
        "max_tokens": request.max_tokens,
        "messages": messages,
    });
    if !system.is_empty() {
        body["system"] = system.join("\n\n").into();
    }
    if !request.tools.is_empty() {
        let tools: Vec<serde_json::Value> = request.tools.iter().filter_map(tool_spec).collect();
        if !tools.is_empty() {
            body["tools"] = serde_json::Value::Array(tools);
        }
    }
    match request.anthropic_thinking {
        AnthropicThinking::EnabledBudget => {
            let budget = (request.max_tokens / 2).clamp(1024, 4096);
            if budget < request.max_tokens {
                body["thinking"] =
                    serde_json::json!({ "type": "enabled", "budget_tokens": budget });
            }
        }
        AnthropicThinking::Adaptive => {
            body["thinking"] = serde_json::json!({ "type": "adaptive" });
        }
        AnthropicThinking::Omit => {}
    }
    body
}

/// One OpenAI-shaped tool spec → the Anthropic shape: `input_schema` is the function's
/// `parameters` (a missing or non-object schema becomes the empty-object schema the protocol
/// requires); `type: "function"` wrappers drop away.
fn tool_spec(spec: &serde_json::Value) -> Option<serde_json::Value> {
    let function = &spec["function"];
    let name = function["name"].as_str()?;
    let input_schema = match &function["parameters"] {
        schema if schema.is_object() => (*schema).clone(),
        _ => serde_json::json!({ "type": "object", "properties": {} }),
    };
    let mut out = serde_json::json!({ "name": name, "input_schema": input_schema });
    if let Some(description) = function["description"].as_str() {
        out["description"] = serde_json::Value::String(description.to_string());
    }
    Some(out)
}

/// One assistant wire message → Anthropic content blocks in the order the protocol wants: the
/// `thinking` block first (with its signature when the record carries one — replayed unmodified,
/// MOE-0035), then the text, then one `tool_use` per recorded call (the string `arguments` are
/// parsed back into the object `input`; unparseable arguments become `{}`).
fn assistant_blocks(message: &serde_json::Value) -> Vec<serde_json::Value> {
    let mut blocks = Vec::new();
    if let Some(thinking) = message["reasoning_content"]
        .as_str()
        .filter(|text| !text.is_empty())
    {
        let mut block = serde_json::json!({ "type": "thinking", "thinking": thinking });
        if let Some(signature) = message["reasoning_signature"]
            .as_str()
            .filter(|signature| !signature.is_empty())
        {
            block["signature"] = serde_json::Value::String(signature.to_string());
        }
        blocks.push(block);
    }
    blocks.extend(content_blocks(&message["content"]));
    if let Some(calls) = message["tool_calls"].as_array() {
        for call in calls {
            let arguments = call["function"]["arguments"].as_str().unwrap_or_default();
            let input = serde_json::from_str::<serde_json::Value>(arguments)
                .ok()
                .filter(serde_json::Value::is_object)
                .unwrap_or_else(|| serde_json::json!({}));
            blocks.push(serde_json::json!({
                "type": "tool_use",
                "id": call["id"].as_str().unwrap_or_default(),
                "name": call["function"]["name"].as_str().unwrap_or_default(),
                "input": input,
            }));
        }
    }
    blocks
}

/// One wire content (plain string or multimodal parts, ADR-0010) → Anthropic content blocks:
/// `text` parts become `{"type":"text","text":…}`, `image_url` parts become `image` blocks; empty
/// texts, unknown part types, and unusable image URLs are dropped.
fn content_blocks(content: &serde_json::Value) -> Vec<serde_json::Value> {
    if let Some(text) = content.as_str().filter(|text| !text.is_empty()) {
        return vec![serde_json::json!({ "type": "text", "text": text })];
    }
    let mut blocks = Vec::new();
    if let Some(parts) = content.as_array() {
        for part in parts {
            match part["type"].as_str() {
                Some("text") => {
                    if let Some(text) = part["text"].as_str().filter(|text| !text.is_empty()) {
                        blocks.push(serde_json::json!({ "type": "text", "text": text }));
                    }
                }
                Some("image_url") => {
                    if let Some(block) = part["image_url"]["url"].as_str().and_then(image_block) {
                        blocks.push(block);
                    }
                }
                _ => {}
            }
        }
    }
    blocks
}

/// One `image_url.url` → an `image` block (`{"type":"image","source":…}`): a
/// `data:<media_type>;base64,<data>` URL splits at `;base64,` into a `base64` source; any other
/// non-empty URL is a `url` source; unusable URLs are dropped rather than sent malformed.
fn image_block(url: &str) -> Option<serde_json::Value> {
    if url.is_empty() {
        return None;
    }
    let Some(rest) = url.strip_prefix("data:") else {
        return Some(serde_json::json!({
            "type": "image",
            "source": { "type": "url", "url": url },
        }));
    };
    let (media_type, data) = rest.split_once(";base64,")?;
    if media_type.is_empty() || data.is_empty() {
        return None;
    }
    Some(serde_json::json!({
        "type": "image",
        "source": { "type": "base64", "media_type": media_type, "data": data },
    }))
}

/// One SSE line → the shared delta vocabulary: `content_block_delta` carries answer text
/// (`delta.type == "text_delta"`, `delta.text`), reasoning (`thinking_delta`, `delta.thinking`),
/// streamed tool arguments (`input_json_delta`, `delta.partial_json`) and the thinking signature
/// (`signature_delta`); `content_block_start` opens a tool call (`tool_use`, id + name, and —
/// MiniMax's shortcut — possibly the complete `input`); `message_stop` and `error` end the
/// stream; every other event (`message_start`, text `content_block_start`, `content_block_stop`,
/// `ping`, `message_delta`), malformed JSON, and empty payloads are ignored.
pub fn sse_delta(line: &str) -> SseLine {
    let Some(payload) = line.strip_prefix("data:") else {
        return SseLine::Ignore;
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(payload.trim()) else {
        return SseLine::Ignore;
    };
    match json["type"].as_str() {
        Some("content_block_start")
            if json["content_block"]["type"].as_str() == Some("tool_use") =>
        {
            // The start event carries the call's id and name; MiniMax may also deliver the
            // complete `input` here instead of streaming `input_json_delta` fragments (their
            // documented delta enum omits it) — both paths land in the same merge.
            let arguments = match &json["content_block"]["input"] {
                input @ serde_json::Value::Object(map) if !map.is_empty() => {
                    serde_json::to_string(input).unwrap_or_default()
                }
                _ => String::new(),
            };
            return SseLine::ToolCalls(vec![ToolCallFragment {
                index: json["index"].as_u64().unwrap_or_default(),
                id: Some(
                    json["content_block"]["id"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                ),
                name: Some(
                    json["content_block"]["name"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                ),
                arguments,
            }]);
        }
        Some("content_block_delta") => match json["delta"]["type"].as_str() {
            Some("text_delta") => {
                if let Some(text) = json["delta"]["text"]
                    .as_str()
                    .filter(|text| !text.is_empty())
                {
                    return SseLine::Delta(text.to_string());
                }
            }
            Some("thinking_delta") => {
                if let Some(thinking) = json["delta"]["thinking"]
                    .as_str()
                    .filter(|thinking| !thinking.is_empty())
                {
                    return SseLine::Reasoning(thinking.to_string());
                }
            }
            Some("input_json_delta") => {
                if let Some(partial) = json["delta"]["partial_json"]
                    .as_str()
                    .filter(|partial| !partial.is_empty())
                {
                    return SseLine::ToolCalls(vec![ToolCallFragment {
                        index: json["index"].as_u64().unwrap_or_default(),
                        id: None,
                        name: None,
                        arguments: partial.to_string(),
                    }]);
                }
            }
            Some("signature_delta") => {
                if let Some(signature) = json["delta"]["signature"]
                    .as_str()
                    .filter(|signature| !signature.is_empty())
                {
                    return SseLine::ReasoningSignature(signature.to_string());
                }
            }
            _ => {}
        },
        // `message_stop` ends the stream; `error` stops reading (the HTTP layer already builds
        // the error card for non-2xx responses).
        Some("message_stop" | "error") => return SseLine::Done,
        _ => {}
    }
    SseLine::Ignore
}

/// Blocking (non-streaming) response parsing (the AI commands invoke path, ADR-0024):
/// concatenates the `text` of every `content[]` block of type `text` (`thinking` blocks are
/// skipped); `None` when there is no text or the payload is unparseable.
pub fn completion_text(payload: &str) -> Option<String> {
    let json: serde_json::Value = serde_json::from_str(payload).ok()?;
    let text: String = json["content"]
        .as_array()?
        .iter()
        .filter(|block| block["type"].as_str() == Some("text"))
        .filter_map(|block| block["text"].as_str())
        .collect();
    Some(text).filter(|text| !text.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The endpoint join (MOE-0031): Anthropic's bare host gets `/v1/messages`; a base that
    /// already carries the version root (MiniMax's Anthropic-compatible surface) gains only
    /// `/messages` — and a trailing slash never doubles.
    #[test]
    fn endpoint_url_respects_an_existing_version_root() {
        assert_eq!(
            endpoint_url("https://api.anthropic.com"),
            "https://api.anthropic.com/v1/messages"
        );
        assert_eq!(
            endpoint_url("https://api.minimax.io/anthropic/v1/"),
            "https://api.minimax.io/anthropic/v1/messages"
        );
        assert_eq!(
            endpoint_url("https://api.minimax.cn/anthropic/v1"),
            "https://api.minimax.cn/anthropic/v1/messages"
        );
    }

    /// Recorded `content_block_delta` lines: `text_delta` is answer text, `thinking_delta` is
    /// reasoning; empty payloads and the non-text delta types land on the ignore path.
    #[test]
    fn parses_text_and_thinking_deltas() {
        assert_eq!(
            sse_delta(
                "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}"
            ),
            SseLine::Delta("Hello".into())
        );
        assert_eq!(
            sse_delta(
                "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"step one\"}}"
            ),
            SseLine::Reasoning("step one".into())
        );
        // The closing packet some endpoints send carries an empty text
        assert_eq!(
            sse_delta(
                "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"\"}}"
            ),
            SseLine::Ignore
        );
        assert_eq!(
            sse_delta(
                "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"\"}}"
            ),
            SseLine::Ignore
        );
        // The thinking signature rides its own delta; streamed tool arguments are fragments
        // (MOE-0035 — both are replay/call material, never answer text)
        assert_eq!(
            sse_delta(
                "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"signature_delta\",\"signature\":\"EqQBCgIYAhIM\"}}"
            ),
            SseLine::ReasoningSignature("EqQBCgIYAhIM".into())
        );
        assert_eq!(
            sse_delta(
                "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{}\"}}"
            ),
            SseLine::ToolCalls(vec![ToolCallFragment {
                index: 1,
                id: None,
                name: None,
                arguments: "{}".into(),
            }])
        );
    }

    /// A tool call opens as `content_block_start` with id + name (empty `input` → an
    /// arguments-empty fragment); MiniMax may deliver the complete `input` right there — both
    /// paths feed the same index-keyed merge (MOE-0035).
    #[test]
    fn parses_tool_use_starts_with_and_without_complete_input() {
        assert_eq!(
            sse_delta(
                "data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"call_function_9_1\",\"name\":\"web_search\",\"input\":{}}}"
            ),
            SseLine::ToolCalls(vec![ToolCallFragment {
                index: 1,
                id: Some("call_function_9_1".into()),
                name: Some("web_search".into()),
                arguments: String::new(),
            }])
        );
        assert_eq!(
            sse_delta(
                "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"c1\",\"name\":\"read_file\",\"input\":{\"path\":\"/tmp/x\"}}}"
            ),
            SseLine::ToolCalls(vec![ToolCallFragment {
                index: 0,
                id: Some("c1".into()),
                name: Some("read_file".into()),
                arguments: "{\"path\":\"/tmp/x\"}".into(),
            }])
        );
        // A streamed argument fragment carries neither id nor name
        assert_eq!(
            sse_delta(
                "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"query\\\":\\\"ru\"}}"
            ),
            SseLine::ToolCalls(vec![ToolCallFragment {
                index: 1,
                id: None,
                name: None,
                arguments: "{\"query\":\"ru".into(),
            }])
        );
        // A text block's start is still just bookkeeping
        assert_eq!(
            sse_delta(
                "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}"
            ),
            SseLine::Ignore
        );
    }

    /// `message_stop` and `error` end the stream; bookkeeping events, `event:`/comment/empty
    /// lines, and malformed JSON are ignored.
    #[test]
    fn stops_on_terminal_events_and_ignores_the_rest() {
        assert_eq!(
            sse_delta("data: {\"type\":\"message_stop\"}"),
            SseLine::Done
        );
        assert_eq!(
            sse_delta(
                "data: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}"
            ),
            SseLine::Done
        );
        assert_eq!(sse_delta(""), SseLine::Ignore);
        assert_eq!(sse_delta(": keep-alive"), SseLine::Ignore);
        // `event:` lines only name the event; the `data:` line that follows carries the JSON
        assert_eq!(sse_delta("event: content_block_delta"), SseLine::Ignore);
        assert_eq!(sse_delta("data: not-json"), SseLine::Ignore);
        assert_eq!(sse_delta("data: {}"), SseLine::Ignore);
        assert_eq!(
            sse_delta("data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\"}}"),
            SseLine::Ignore
        );
        assert_eq!(
            sse_delta(
                "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}"
            ),
            SseLine::Ignore
        );
        assert_eq!(
            sse_delta("data: {\"type\":\"content_block_stop\",\"index\":0}"),
            SseLine::Ignore
        );
        assert_eq!(sse_delta("data: {\"type\":\"ping\"}"), SseLine::Ignore);
        assert_eq!(
            sse_delta(
                "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":15}}"
            ),
            SseLine::Ignore
        );
    }

    /// System text concatenates into the top-level `system` string; `messages` carries only
    /// non-empty `user`/`assistant` turns, and a `tool` message folds into a `tool_result` block
    /// on a user message (MOE-0035 — it can no longer be dropped: the record replays it).
    #[test]
    fn builds_body_with_system_extraction() {
        let request = ChatRequest {
            model: "claude-sonnet-4-5".into(),
            messages: vec![
                serde_json::json!({ "role": "system", "content": "be brief" }),
                serde_json::json!({ "role": "system", "content": "answer in English" }),
                serde_json::json!({ "role": "user", "content": "hello" }),
                serde_json::json!({ "role": "assistant", "content": "Hi!" }),
                serde_json::json!({ "role": "tool", "tool_call_id": "call_1", "content": "tool output" }),
                serde_json::json!({ "role": "assistant", "content": null, "tool_calls": [] }),
                serde_json::json!({ "role": "user", "content": "" }),
            ],
            stream: true,
            reasoning: false,
            max_tokens: 8192,
            tools: Vec::new(),
            anthropic_thinking: AnthropicThinking::Omit,
        };
        // A request without system messages carries no `system` key at all
        let plain = body(&ChatRequest {
            model: "m".into(),
            messages: vec![serde_json::json!({ "role": "user", "content": "hi" })],
            stream: false,
            reasoning: false,
            max_tokens: 4096,
            tools: Vec::new(),
            anthropic_thinking: AnthropicThinking::Omit,
        });
        assert!(plain.get("system").is_none());
        assert_eq!(plain["stream"], false);

        let body = body(&request);
        assert_eq!(body["model"], "claude-sonnet-4-5");
        assert_eq!(body["stream"], true);
        assert_eq!(body["max_tokens"], 8192);
        assert_eq!(body["system"], "be brief\n\nanswer in English");
        assert_eq!(body["messages"].as_array().map(Vec::len), Some(3));
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(
            body["messages"][0]["content"][0],
            serde_json::json!({ "type": "text", "text": "hello" })
        );
        assert_eq!(body["messages"][1]["role"], "assistant");
        assert_eq!(
            body["messages"][1]["content"][0],
            serde_json::json!({ "type": "text", "text": "Hi!" })
        );
        // The tool result rides a user message (the block-less assistant that follows never
        // separates a result from its pairing); the empty-content blocks are dropped
        assert_eq!(body["messages"][2]["role"], "user");
        assert_eq!(
            body["messages"][2]["content"][0],
            serde_json::json!({ "type": "tool_result", "tool_use_id": "call_1", "content": "tool output" })
        );
        assert!(
            body.get("thinking").is_none(),
            "reasoning off → no thinking opt-in"
        );
        assert!(body.get("tools").is_none(), "no tools → no tools key");
    }

    /// The tool round translation (MOE-0035): specs → `{name, description, input_schema}`; the
    /// assistant round → `thinking`(with signature) first, then one `tool_use` per call (broken
    /// arguments become `{}`); a results run merges into the next user message results-first, or
    /// stands alone at the record's end.
    #[test]
    fn translates_tools_and_tool_rounds_into_anthropic_shapes() {
        let request = ChatRequest {
            model: "MiniMax-M3".into(),
            messages: vec![
                serde_json::json!({ "role": "system", "content": "head" }),
                serde_json::json!({ "role": "user", "content": "what is new?" }),
                serde_json::json!({ "role": "assistant", "content": "", "reasoning_content": "checking", "reasoning_signature": "sig123", "tool_calls": [
                    { "id": "call_function_1_1", "type": "function", "function": { "name": "web_search", "arguments": "{\"query\":\"x\"}" } },
                    { "id": "call_function_1_2", "type": "function", "function": { "name": "web_fetch", "arguments": "not-json" } },
                ] }),
                serde_json::json!({ "role": "tool", "tool_call_id": "call_function_1_1", "content": "1. hit" }),
                serde_json::json!({ "role": "tool", "tool_call_id": "call_function_1_2", "content": "Error: unreachable" }),
                serde_json::json!({ "role": "assistant", "content": "The news, cited." }),
                serde_json::json!({ "role": "user", "content": "thanks" }),
            ],
            stream: true,
            reasoning: true,
            max_tokens: 8192,
            tools: vec![serde_json::json!({ "type": "function", "function": {
                "name": "web_search",
                "description": "Search the web.",
                "parameters": { "type": "object", "properties": { "query": { "type": "string" } }, "required": ["query"] },
            } })],
            anthropic_thinking: AnthropicThinking::Adaptive,
        };
        let translated = body(&request);
        // The spec converts; the OpenAI wrapper is gone
        assert_eq!(translated["tools"][0]["name"], "web_search");
        assert_eq!(translated["tools"][0]["description"], "Search the web.");
        assert_eq!(translated["tools"][0]["input_schema"]["type"], "object");
        assert!(translated["tools"][0].get("function").is_none());
        // MiniMax's dialect: adaptive, never enabled+budget, never disabled
        assert_eq!(translated["thinking"], serde_json::json!({ "type": "adaptive" }));
        // The assistant round: thinking(+signature) first, then both tool_use blocks
        let assistant = &translated["messages"][1];
        assert_eq!(assistant["role"], "assistant");
        assert_eq!(
            assistant["content"][0],
            serde_json::json!({ "type": "thinking", "thinking": "checking", "signature": "sig123" })
        );
        assert_eq!(assistant["content"][1]["type"], "tool_use");
        assert_eq!(assistant["content"][1]["id"], "call_function_1_1");
        assert_eq!(
            assistant["content"][1]["input"],
            serde_json::json!({ "query": "x" })
        );
        assert_eq!(
            assistant["content"][2]["input"],
            serde_json::json!({}),
            "unparseable arguments become the empty object"
        );
        // The results run stands alone before the assistant that answered it
        let results = &translated["messages"][2];
        assert_eq!(results["role"], "user");
        assert_eq!(
            results["content"][0],
            serde_json::json!({ "type": "tool_result", "tool_use_id": "call_function_1_1", "content": "1. hit" })
        );
        assert_eq!(results["content"][1]["tool_use_id"], "call_function_1_2");
        assert_eq!(translated["messages"][3]["role"], "assistant");
        assert_eq!(translated["messages"][4]["content"][0]["text"], "thanks");

        // A run at the record's end (a stopped turn) followed by the next question: results
        // merge into that user message, results first
        let tail = body(&ChatRequest {
            model: "m".into(),
            messages: vec![
                serde_json::json!({ "role": "user", "content": "q" }),
                serde_json::json!({ "role": "assistant", "content": "", "tool_calls": [
                    { "id": "c1", "type": "function", "function": { "name": "web_search", "arguments": "{}" } },
                ] }),
                serde_json::json!({ "role": "tool", "tool_call_id": "c1", "content": "(stopped by the user)" }),
                serde_json::json!({ "role": "user", "content": "continue?" }),
            ],
            stream: true,
            reasoning: false,
            max_tokens: 4096,
            tools: Vec::new(),
            anthropic_thinking: AnthropicThinking::Omit,
        });
        let merged = &tail["messages"][2];
        assert_eq!(merged["role"], "user");
        assert_eq!(merged["content"][0]["type"], "tool_result");
        assert_eq!(
            merged["content"][1],
            serde_json::json!({ "type": "text", "text": "continue?" })
        );
    }

    /// Multimodal wire parts become Anthropic blocks: `data:` URLs a `base64` source with
    /// `media_type`/`data` split at `;base64,`, other URLs a `url` source; empty or malformed
    /// URLs are dropped.
    #[test]
    fn converts_image_parts_to_anthropic_sources() {
        let request = ChatRequest {
            model: "m".into(),
            messages: vec![serde_json::json!({ "role": "user", "content": [
                { "type": "text", "text": "what is this" },
                { "type": "image_url", "image_url": { "url": "data:image/png;base64,AA==" } },
                { "type": "image_url", "image_url": { "url": "https://example.com/shot.png" } },
                { "type": "image_url", "image_url": { "url": "" } },
                { "type": "image_url", "image_url": { "url": "data:image/png,not-base64" } },
            ] })],
            stream: false,
            reasoning: false,
            max_tokens: 4096,
            tools: Vec::new(),
            anthropic_thinking: AnthropicThinking::Omit,
        };
        let body = body(&request);
        let content = &body["messages"][0]["content"];
        assert_eq!(content.as_array().map(Vec::len), Some(3));
        assert_eq!(
            content[0],
            serde_json::json!({ "type": "text", "text": "what is this" })
        );
        assert_eq!(
            content[1],
            serde_json::json!({
                "type": "image",
                "source": { "type": "base64", "media_type": "image/png", "data": "AA==" },
            })
        );
        assert_eq!(
            content[2],
            serde_json::json!({
                "type": "image",
                "source": { "type": "url", "url": "https://example.com/shot.png" },
            })
        );
    }

    /// The reasoning opt-in per dialect (MOE-0035): `EnabledBudget` keeps the legacy rule —
    /// `thinking.budget_tokens` is half of `max_tokens` clamped to `1024..=4096`, strictly below
    /// `max_tokens`, omitted when both cannot hold; `Adaptive` (MiniMax) always sends its bare
    /// key; `Omit` never sends one.
    #[test]
    fn opts_into_thinking_when_the_budget_fits() {
        let request = |thinking: AnthropicThinking, max_tokens: u64| ChatRequest {
            model: "claude-opus-4-1".into(),
            messages: vec![serde_json::json!({ "role": "user", "content": "hi" })],
            stream: true,
            reasoning: false,
            max_tokens,
            tools: Vec::new(),
            anthropic_thinking: thinking,
        };
        let enabled = body(&request(AnthropicThinking::EnabledBudget, 8192));
        assert_eq!(
            enabled["thinking"],
            serde_json::json!({ "type": "enabled", "budget_tokens": 4096 })
        );
        let halved = body(&request(AnthropicThinking::EnabledBudget, 3000));
        assert_eq!(halved["thinking"]["budget_tokens"], 1500);
        // 1024 is the floor *and* must stay below `max_tokens`: too small a request → omitted
        let too_small = body(&request(AnthropicThinking::EnabledBudget, 1024));
        assert!(too_small.get("thinking").is_none());
        let off = body(&request(AnthropicThinking::Omit, 8192));
        assert!(off.get("thinking").is_none());
        // MiniMax's whitelist: adaptive, with no budget arithmetic at all
        let adaptive = body(&request(AnthropicThinking::Adaptive, 1024));
        assert_eq!(
            adaptive["thinking"],
            serde_json::json!({ "type": "adaptive" })
        );
    }

    /// The blocking path reads the Anthropic `content[]` blocks: every `text` block is
    /// concatenated, `thinking` blocks are skipped, and a payload without text yields `None`.
    #[test]
    fn completion_text_concats_text_blocks() {
        let payload = r#"{"id":"msg_1","type":"message","role":"assistant","content":[{"type":"thinking","thinking":"step one","signature":"Sig"},{"type":"text","text":"the answer"},{"type":"text","text":" indeed"}],"stop_reason":"end_turn"}"#;
        assert_eq!(
            completion_text(payload).as_deref(),
            Some("the answer indeed")
        );
        assert_eq!(
            completion_text(r#"{"content":[{"type":"thinking","thinking":"only thinking"}]}"#),
            None
        );
        assert_eq!(
            completion_text(r#"{"content":[{"type":"text","text":""}]}"#),
            None
        );
        assert_eq!(completion_text(r#"{"content":[]}"#), None);
        assert_eq!(completion_text("not-json"), None);
    }
}
