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
//! - The reasoning opt-in is `thinking: {"type":"enabled","budget_tokens":N}` with
//!   `1024 <= budget < max_tokens`; thinking arrives as `content_block_delta` with
//!   `delta.type == "thinking_delta"`.
//! - SSE lines are `data: {json}` where `json.type` is the event; `message_stop` ends the stream.

use super::{ChatRequest, SseLine};

/// The `anthropic-version` header every request carries.
pub const API_VERSION: &str = "2023-06-01";

pub fn endpoint_url(api_url: &str) -> String {
    format!("{}/v1/messages", api_url.trim_end_matches('/'))
}

/// Request body (module notes): `model`/`stream`/`max_tokens`/`messages`, the top-level `system`
/// string (every `system` message's text, blank-line joined; the key is absent when there is
/// none), and the `thinking` opt-in when `request.reasoning` and a legal `budget_tokens` exists —
/// half of `max_tokens` clamped to `1024..=4096`, strictly below `max_tokens`, otherwise omitted.
/// `tool` roles and empty content can never occur in this protocol and are dropped.
pub(super) fn body(request: &ChatRequest) -> serde_json::Value {
    let mut system: Vec<String> = Vec::new();
    let mut messages: Vec<serde_json::Value> = Vec::new();
    for message in &request.messages {
        let blocks = content_blocks(&message["content"]);
        match message["role"].as_str() {
            // System text rides the top-level `system` string, never `messages` (images in a
            // system message cannot be represented there and are dropped).
            Some("system") => system.extend(
                blocks
                    .iter()
                    .filter_map(|block| block["text"].as_str())
                    .map(str::to_string),
            ),
            Some(role @ ("user" | "assistant")) if !blocks.is_empty() => {
                messages.push(serde_json::json!({ "role": role, "content": blocks }));
            }
            _ => {}
        }
    }
    let mut body = serde_json::json!({
        "model": request.model,
        "stream": request.stream,
        "max_tokens": request.max_tokens,
        "messages": messages,
    });
    if !system.is_empty() {
        body["system"] = system.join("\n\n").into();
    }
    let budget = (request.max_tokens / 2).clamp(1024, 4096);
    if request.reasoning && budget < request.max_tokens {
        body["thinking"] = serde_json::json!({ "type": "enabled", "budget_tokens": budget });
    }
    body
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
/// (`delta.type == "text_delta"`, `delta.text`) and reasoning (`delta.type == "thinking_delta"`,
/// `delta.thinking`); `message_stop` and `error` end the stream; every other event
/// (`message_start`, `content_block_start`/`stop`, `ping`, `message_delta`, signature and JSON
/// deltas), malformed JSON, and empty payloads are ignored.
pub fn sse_delta(line: &str) -> SseLine {
    let Some(payload) = line.strip_prefix("data:") else {
        return SseLine::Ignore;
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(payload.trim()) else {
        return SseLine::Ignore;
    };
    match json["type"].as_str() {
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
        // `signature_delta` and `input_json_delta` carry no answer text (tools are gated to
        // openai-chat in v1)
        assert_eq!(
            sse_delta(
                "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"signature_delta\",\"signature\":\"EqQBCgIYAhIM\"}}"
            ),
            SseLine::Ignore
        );
        assert_eq!(
            sse_delta(
                "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{}\"}}"
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
    /// non-empty `user`/`assistant` turns (`tool` roles cannot occur here and are dropped).
    #[test]
    fn builds_body_with_system_extraction() {
        let request = ChatRequest {
            model: "claude-sonnet-4-5".into(),
            messages: vec![
                serde_json::json!({ "role": "system", "content": "be brief" }),
                serde_json::json!({ "role": "system", "content": "answer in English" }),
                serde_json::json!({ "role": "user", "content": "hello" }),
                serde_json::json!({ "role": "assistant", "content": "Hi!" }),
                serde_json::json!({ "role": "tool", "content": "tool output" }),
                serde_json::json!({ "role": "assistant", "content": null, "tool_calls": [] }),
                serde_json::json!({ "role": "user", "content": "" }),
            ],
            stream: true,
            reasoning: false,
            max_tokens: 8192,
        };
        // A request without system messages carries no `system` key at all
        let plain = body(&ChatRequest {
            model: "m".into(),
            messages: vec![serde_json::json!({ "role": "user", "content": "hi" })],
            stream: false,
            reasoning: false,
            max_tokens: 4096,
        });
        assert!(plain.get("system").is_none());
        assert_eq!(plain["stream"], false);

        let body = body(&request);
        assert_eq!(body["model"], "claude-sonnet-4-5");
        assert_eq!(body["stream"], true);
        assert_eq!(body["max_tokens"], 8192);
        assert_eq!(body["system"], "be brief\n\nanswer in English");
        assert_eq!(body["messages"].as_array().map(Vec::len), Some(2));
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
        assert!(
            body.get("thinking").is_none(),
            "reasoning off → no thinking opt-in"
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

    /// The reasoning opt-in: `thinking.budget_tokens` is half of `max_tokens` clamped to
    /// `1024..=4096` and must stay strictly below `max_tokens` — when both cannot hold, no
    /// `thinking` key is sent.
    #[test]
    fn opts_into_thinking_when_the_budget_fits() {
        let request = |reasoning: bool, max_tokens: u64| ChatRequest {
            model: "claude-opus-4-1".into(),
            messages: vec![serde_json::json!({ "role": "user", "content": "hi" })],
            stream: true,
            reasoning,
            max_tokens,
        };
        let enabled = body(&request(true, 8192));
        assert_eq!(
            enabled["thinking"],
            serde_json::json!({ "type": "enabled", "budget_tokens": 4096 })
        );
        let halved = body(&request(true, 3000));
        assert_eq!(halved["thinking"]["budget_tokens"], 1500);
        // 1024 is the floor *and* must stay below `max_tokens`: too small a request → omitted
        let too_small = body(&request(true, 1024));
        assert!(too_small.get("thinking").is_none());
        let off = body(&request(false, 8192));
        assert!(off.get("thinking").is_none());
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
