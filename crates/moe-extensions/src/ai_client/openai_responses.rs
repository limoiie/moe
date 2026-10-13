//! The OpenAI Responses adapter (ADR-0040): endpoint URL, request body, SSE parsing, and the
//! blocking-response text extractor.
//!
//! Wire notes (platform.openai.com/docs/api-reference/responses, `POST {api_url}/responses`):
//! - Bearer auth (see `mod::authenticate`).
//! - The input is an `input` array of messages: `system` messages concatenate into the top-level
//!   `instructions` string; user text parts are `{"type":"input_text","text":…}`, assistant text
//!   parts `{"type":"output_text","text":…}`, images `{"type":"input_image","image_url":…}`
//!   (the shared wire form is an `image_url` part with a `data:` URL).
//! - `store: false` (a local desktop app keeps no server-side copies); `stream` per request.
//! - The reasoning opt-in is `reasoning: {"summary":"auto"}`; summaries stream as
//!   `response.reasoning_summary_text.delta` events; text as `response.output_text.delta`;
//!   `response.completed` (and `.failed`/`.incomplete`) end the stream.

use super::{ChatRequest, SseLine};

pub fn endpoint_url(api_url: &str) -> String {
    format!("{}/responses", api_url.trim_end_matches('/'))
}

/// The request → Responses body (`POST {api_url}/responses`): `system` turns concatenate into the
/// top-level `instructions` string (joined with blank lines), not into `input`; `user`/`assistant`
/// turns become `input` items whose parts are `input_text` / `output_text` / `input_image` (the
/// shared `image_url.url` data URL moves over as is). Always `store: false` (no server-side
/// copies); a reasoning request adds `reasoning: {"summary":"auto"}`, and an explicit effort
/// (MOE-0062) joins it as `{"effort":"low|medium|high","summary":"auto"}`.
pub(super) fn body(request: &ChatRequest) -> serde_json::Value {
    let mut instructions: Vec<&str> = Vec::new();
    let mut input: Vec<serde_json::Value> = Vec::new();
    for message in &request.messages {
        match message["role"].as_str() {
            Some("system") => {
                if let Some(text) = message["content"].as_str().filter(|text| !text.is_empty()) {
                    instructions.push(text);
                }
            }
            Some(role @ ("user" | "assistant")) => {
                let parts = content_parts(role, &message["content"]);
                if !parts.is_empty() {
                    input.push(serde_json::json!({ "role": role, "content": parts }));
                }
            }
            // `tool` turns and tool-call payloads cannot occur on this protocol (the tool loop is
            // gated to openai-chat); anything unmappable is dropped, never an error.
            _ => {}
        }
    }
    let mut body = serde_json::json!({
        "model": request.model,
        "stream": request.stream,
        "input": input,
        "store": false,
    });
    if !instructions.is_empty() {
        body["instructions"] = serde_json::Value::String(instructions.join("\n\n"));
    }
    if request.reasoning.is_on() {
        let mut reasoning = serde_json::json!({ "summary": "auto" });
        if let Some(effort) = request.reasoning.effort() {
            reasoning["effort"] = serde_json::json!(effort.as_str());
        }
        body["reasoning"] = reasoning;
    }
    if let Some(temperature) = request.temperature {
        body["temperature"] = serde_json::json!(temperature);
    }
    body
}

/// One wire message's `content` → `input` parts: a bare string becomes a single text part; an
/// array maps part by part (`text` per role, `image_url` → `input_image`). Empty parts are
/// skipped.
fn content_parts(role: &str, content: &serde_json::Value) -> Vec<serde_json::Value> {
    if let Some(text) = content.as_str().filter(|text| !text.is_empty()) {
        return vec![text_part(role, text)];
    }
    content
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|part| match part["type"].as_str() {
            Some("text") => part["text"]
                .as_str()
                .filter(|text| !text.is_empty())
                .map(|text| text_part(role, text)),
            Some("image_url") => part["image_url"]["url"]
                .as_str()
                .filter(|url| !url.is_empty())
                .map(|url| serde_json::json!({ "type": "input_image", "image_url": url })),
            _ => None,
        })
        .collect()
}

/// The text part type differs by turn: user content is `input_text`, assistant `output_text`.
fn text_part(role: &str, text: &str) -> serde_json::Value {
    let part_type = if role == "assistant" {
        "output_text"
    } else {
        "input_text"
    };
    serde_json::json!({ "type": part_type, "text": text })
}

/// Outcome of processing one SSE line: text streams as `response.output_text.delta`, reasoning
/// summaries as `response.reasoning_summary_text.delta` (the alias `response.reasoning_text.delta`
/// is accepted too); `response.completed` — also `.failed` and `.incomplete` — ends the stream;
/// every other event is ignored.
pub fn sse_delta(line: &str) -> SseLine {
    let Some(payload) = line.strip_prefix("data:") else {
        return SseLine::Ignore;
    };
    let payload = payload.trim();
    let Ok(json) = serde_json::from_str::<serde_json::Value>(payload) else {
        return SseLine::Ignore;
    };
    let delta = json["delta"].as_str().filter(|delta| !delta.is_empty());
    match (json["type"].as_str(), delta) {
        (Some("response.output_text.delta"), Some(delta)) => SseLine::Delta(delta.to_string()),
        (
            Some("response.reasoning_summary_text.delta" | "response.reasoning_text.delta"),
            Some(delta),
        ) => SseLine::Reasoning(delta.to_string()),
        (Some("response.completed" | "response.failed" | "response.incomplete"), _) => {
            SseLine::Done
        }
        _ => SseLine::Ignore,
    }
}

/// Non-streaming response parsing (the blocking invoke path): concat the `text` of `content[]`
/// parts with `type == "output_text"` across `output[]` items with `type == "message"`.
pub fn completion_text(payload: &str) -> Option<String> {
    let json: serde_json::Value = serde_json::from_str(payload).ok()?;
    let text: String = json["output"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item["type"].as_str() == Some("message"))
        .flat_map(|item| item["content"].as_array().into_iter().flatten())
        .filter(|part| part["type"].as_str() == Some("output_text"))
        .filter_map(|part| part["text"].as_str().filter(|text| !text.is_empty()))
        .collect();
    Some(text).filter(|text| !text.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_client::Reasoning;
    use moe_platform::config::ReasoningSetting;

    fn request(messages: Vec<serde_json::Value>, reasoning: Reasoning) -> ChatRequest {
        ChatRequest {
            model: "gpt-5.2".into(),
            messages,
            stream: true,
            reasoning,
            temperature: None,
            max_tokens: 2048,
            tools: Vec::new(),
            anthropic_thinking: crate::ai_client::AnthropicThinking::Omit,
        }
    }

    /// Recorded SSE fixtures: text deltas and reasoning-summary deltas (ADR-0040).
    #[test]
    fn parses_text_and_reasoning_deltas() {
        assert_eq!(
            sse_delta(
                r#"data: {"type":"response.output_text.delta","item_id":"msg_1","output_index":0,"content_index":0,"delta":"Hel"}"#
            ),
            SseLine::Delta("Hel".into())
        );
        assert_eq!(
            sse_delta(
                r#"data: {"type":"response.reasoning_summary_text.delta","item_id":"rs_1","output_index":0,"summary_index":0,"delta":"the user"}"#
            ),
            SseLine::Reasoning("the user".into())
        );
        // The alias some gateways emit for the same summary channel
        assert_eq!(
            sse_delta(r#"data: {"type":"response.reasoning_text.delta","delta":"asks"}"#),
            SseLine::Reasoning("asks".into())
        );
    }

    /// Stream-end events land on `Done`; protocol chrome, malformed lines, and empty deltas are
    /// ignored (mirrors openai_chat's ignore-path coverage).
    #[test]
    fn ends_on_terminal_events_and_ignores_the_rest() {
        assert_eq!(
            sse_delta(r#"data: {"type":"response.completed","response":{"id":"resp_1"}}"#),
            SseLine::Done
        );
        assert_eq!(
            sse_delta(r#"data: {"type":"response.failed"}"#),
            SseLine::Done
        );
        assert_eq!(
            sse_delta(r#"data: {"type":"response.incomplete"}"#),
            SseLine::Done
        );
        assert_eq!(
            sse_delta(r#"data: {"type":"response.created","response":{"id":"resp_1"}}"#),
            SseLine::Ignore
        );
        assert_eq!(
            sse_delta(r#"data: {"type":"response.output_item.added","output_index":0}"#),
            SseLine::Ignore
        );
        assert_eq!(
            sse_delta(
                r#"data: {"type":"response.content_part.done","part":{"type":"output_text","text":"Hel"}}"#
            ),
            SseLine::Ignore
        );
        // A closing text delta with no text
        assert_eq!(
            sse_delta(r#"data: {"type":"response.output_text.delta","delta":""}"#),
            SseLine::Ignore
        );
        assert_eq!(sse_delta("data: not-json"), SseLine::Ignore);
        assert_eq!(sse_delta("data: [DONE]"), SseLine::Ignore);
        assert_eq!(
            sse_delta("event: response.output_text.delta"),
            SseLine::Ignore
        );
        assert_eq!(sse_delta(""), SseLine::Ignore);
        assert_eq!(sse_delta(": keep-alive"), SseLine::Ignore);
    }

    /// `system` → top-level `instructions` (blank-line joined); user/assistant turns → `input`
    /// items with `input_text` / `output_text` / `input_image` parts (ADR-0040).
    #[test]
    fn builds_body_with_instructions_and_input_parts() {
        let body = body(&request(
            vec![
                serde_json::json!({ "role": "system", "content": "You are Moe." }),
                serde_json::json!({ "role": "system", "content": "Be terse." }),
                serde_json::json!({ "role": "user", "content": "hello" }),
                serde_json::json!({ "role": "assistant", "content": "hi there" }),
                serde_json::json!({ "role": "user", "content": [
                    { "type": "text", "text": "what is this?" },
                    { "type": "image_url", "image_url": { "url": "data:image/png;base64,AA==" } }
                ] }),
            ],
            Reasoning::Off,
        ));
        assert_eq!(body["model"], "gpt-5.2");
        assert_eq!(body["stream"], true);
        assert_eq!(body["store"], false);
        assert_eq!(body["instructions"], "You are Moe.\n\nBe terse.");
        assert_eq!(body["input"].as_array().unwrap().len(), 3);
        assert_eq!(body["input"][0]["role"], "user");
        assert_eq!(body["input"][0]["content"][0]["type"], "input_text");
        assert_eq!(body["input"][0]["content"][0]["text"], "hello");
        assert_eq!(body["input"][1]["role"], "assistant");
        assert_eq!(body["input"][1]["content"][0]["type"], "output_text");
        assert_eq!(body["input"][1]["content"][0]["text"], "hi there");
        assert_eq!(body["input"][2]["content"][0]["type"], "input_text");
        assert_eq!(body["input"][2]["content"][0]["text"], "what is this?");
        assert_eq!(body["input"][2]["content"][1]["type"], "input_image");
        assert_eq!(
            body["input"][2]["content"][1]["image_url"],
            "data:image/png;base64,AA=="
        );
    }

    /// Unmappable turns are dropped (`tool` roles / tool-call parts never occur here); `store`
    /// stays `false`; the `reasoning` opt-in appears exactly when requested.
    #[test]
    fn drops_unmappable_turns_and_toggles_reasoning() {
        let reasoning_body = body(&request(
            vec![
                serde_json::json!({ "role": "tool", "content": "{\"ok\":true}", "tool_call_id": "call_1" }),
                serde_json::json!({ "role": "user", "content": "" }),
                serde_json::json!({ "role": "assistant", "content": [
                    { "type": "tool_call", "id": "call_1" }
                ] }),
                serde_json::json!({ "role": "system", "content": "" }),
                serde_json::json!({ "role": "user", "content": "hi" }),
            ],
            Reasoning::Default,
        ));
        assert_eq!(reasoning_body["input"].as_array().unwrap().len(), 1);
        assert_eq!(reasoning_body["input"][0]["role"], "user");
        assert_eq!(reasoning_body["input"][0]["content"][0]["text"], "hi");
        assert!(
            reasoning_body.get("instructions").is_none(),
            "no non-empty system turns → no instructions key"
        );
        assert_eq!(reasoning_body["store"], false);
        assert_eq!(
            reasoning_body["reasoning"],
            serde_json::json!({ "summary": "auto" })
        );

        // An explicit effort joins the same object (MOE-0062); the temperature is the key's
        // presence, and the chat-completions dialect never sends one.
        let effort = body(&request(
            vec![serde_json::json!({ "role": "user", "content": "hi" })],
            Reasoning::Effort(ReasoningSetting::High),
        ));
        assert_eq!(
            effort["reasoning"],
            serde_json::json!({ "effort": "high", "summary": "auto" })
        );
        assert!(effort.get("temperature").is_none());

        let plain = body(&request(
            vec![serde_json::json!({ "role": "user", "content": "hi" })],
            Reasoning::Off,
        ));
        assert!(
            plain.get("reasoning").is_none(),
            "no opt-in → no reasoning key"
        );
        assert_eq!(plain["store"], false);
    }

    /// The blocking path walks `output[]` messages and concats their `output_text` parts.
    #[test]
    fn completion_text_walks_output_messages() {
        let payload = r#"{"id":"resp_1","output":[
            {"type":"reasoning","id":"rs_1","summary":[]},
            {"type":"message","id":"msg_1","role":"assistant","content":[
                {"type":"output_text","text":"the rewritten "},
                {"type":"output_text","text":"text"}
            ]}
        ]}"#;
        assert_eq!(
            completion_text(payload).as_deref(),
            Some("the rewritten text")
        );
        assert_eq!(completion_text(r#"{"output":[]}"#), None);
        assert_eq!(
            completion_text(r#"{"output":[{"type":"message","content":[]}]}"#),
            None
        );
        assert_eq!(
            completion_text(
                r#"{"output":[{"type":"message","content":[{"type":"output_text","text":""}]}]}"#
            ),
            None
        );
        assert_eq!(completion_text("not-json"), None);
    }
}
