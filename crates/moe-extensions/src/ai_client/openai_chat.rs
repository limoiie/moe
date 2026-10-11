//! The OpenAI Chat Completions adapter (ADR-0005; kept byte-compatible for MOE-0026's tool loop):
//! endpoint URL, request bodies, SSE parsing, and the blocking-response text extractor.

use super::{SseLine, ToolCallFragment};
use crate::ai_client::ChatRequest;

pub fn chat_completions_url(base_url: &str) -> String {
    format!("{}/chat/completions", base_url.trim_end_matches('/'))
}

pub fn chat_body(model: &str, question: &str) -> serde_json::Value {
    chat_body_from_messages(
        model,
        vec![serde_json::json!({ "role": "user", "content": question })],
    )
}

/// Request body: messages are already built in OpenAI format (multimodal parts with attachments,
/// ADR-0010); history is passed to the endpoint in chronological order.
pub fn chat_body_from_messages(model: &str, messages: Vec<serde_json::Value>) -> serde_json::Value {
    chat_body_with_stream(model, messages, true)
}

/// Same as above, but streaming can be disabled (`stream: false` for blocking one-shot requests, the AI commands invoke path, ADR-0024).
pub fn chat_body_with_stream(
    model: &str,
    messages: Vec<serde_json::Value>,
    stream: bool,
) -> serde_json::Value {
    serde_json::json!({
        "model": model,
        "stream": stream,
        "messages": messages,
    })
}

/// Same as `chat_body_with_stream`, plus the advertised tool definitions when any are present
/// (MOE-0026): only Agentic Conversations carry `tools`; plain requests look exactly as before.
pub fn chat_body_with_tools(
    model: &str,
    messages: Vec<serde_json::Value>,
    tools: &[serde_json::Value],
    stream: bool,
) -> serde_json::Value {
    let mut body = chat_body_with_stream(model, messages, stream);
    if !tools.is_empty() {
        body["tools"] = serde_json::Value::Array(tools.to_vec());
    }
    body
}

/// The shared request → this protocol's body: `chat_body_with_stream` plus the tools passthrough
/// when the request advertises any (an empty tool list keeps MOE-0026's wire byte-identical).
pub(super) fn body(request: &ChatRequest) -> serde_json::Value {
    chat_body_with_tools(
        &request.model,
        request.messages.clone(),
        &request.tools,
        request.stream,
    )
}

/// Outcome of processing one SSE line.
pub fn sse_delta(line: &str) -> SseLine {
    let Some(payload) = line.strip_prefix("data:") else {
        return SseLine::Ignore;
    };
    let payload = payload.trim();
    if payload == "[DONE]" {
        return SseLine::Done;
    }
    let Ok(json) = serde_json::from_str::<serde_json::Value>(payload) else {
        return SseLine::Ignore;
    };
    let delta = &json["choices"][0]["delta"];
    if let Some(content) = delta["content"]
        .as_str()
        .filter(|content| !content.is_empty())
    {
        return SseLine::Delta(content.to_string());
    }
    if let Some(reasoning) = delta["reasoning_content"]
        .as_str()
        .filter(|reasoning| !reasoning.is_empty())
    {
        return SseLine::Reasoning(reasoning.to_string());
    }
    let fragments: Vec<ToolCallFragment> = delta["tool_calls"]
        .as_array()
        .map(|calls| {
            calls
                .iter()
                .map(|call| ToolCallFragment {
                    index: call["index"].as_u64().unwrap_or(0),
                    id: call["id"]
                        .as_str()
                        .filter(|id| !id.is_empty())
                        .map(str::to_string),
                    name: call["function"]["name"]
                        .as_str()
                        .filter(|name| !name.is_empty())
                        .map(str::to_string),
                    arguments: call["function"]["arguments"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                })
                .collect()
        })
        .unwrap_or_default();
    if !fragments.is_empty() {
        return SseLine::ToolCalls(fragments);
    }
    SseLine::Ignore
}

/// Non-streaming response parsing (the blocking invoke path): `choices[0].message.content`.
pub fn completion_text(payload: &str) -> Option<String> {
    let json: serde_json::Value = serde_json::from_str(payload).ok()?;
    json["choices"][0]["message"]["content"]
        .as_str()
        .map(str::to_string)
        .filter(|text| !text.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_client::ToolCallFragment;

    #[test]
    fn parses_sse_delta_lines() {
        assert_eq!(
            sse_delta("data: {\"choices\":[{\"delta\":{\"content\":\"hello\"}}]}"),
            SseLine::Delta("hello".into())
        );
        assert_eq!(sse_delta("data: [DONE]"), SseLine::Done);
    }

    #[test]
    fn ignores_non_delta_lines() {
        assert_eq!(sse_delta(""), SseLine::Ignore);
        assert_eq!(sse_delta(": keep-alive"), SseLine::Ignore);
        assert_eq!(sse_delta("data: not-json"), SseLine::Ignore);
        // The first chunk usually only has role, no content
        assert_eq!(
            sse_delta("data: {\"choices\":[{\"delta\":{\"role\":\"assistant\"}}]}"),
            SseLine::Ignore
        );
        // content is an empty string (some endpoints send a closing packet)
        assert_eq!(
            sse_delta("data: {\"choices\":[{\"delta\":{\"content\":\"\"}}]}"),
            SseLine::Ignore
        );
    }

    #[test]
    fn builds_endpoint_url_without_double_slash() {
        assert_eq!(
            chat_completions_url("https://api.deepseek.com/v1"),
            "https://api.deepseek.com/v1/chat/completions"
        );
        assert_eq!(
            chat_completions_url("https://x/v1/"),
            "https://x/v1/chat/completions"
        );
    }

    #[test]
    fn builds_chat_request_body() {
        let body = chat_body("deepseek-chat", "hello");
        assert_eq!(body["model"], "deepseek-chat");
        assert_eq!(body["stream"], true);
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "hello");
    }

    /// Streaming off (blocking invoke of AI commands, ADR-0024).
    #[test]
    fn builds_non_stream_body() {
        let body = chat_body_with_stream(
            "m",
            vec![serde_json::json!({ "role": "user", "content": "hi" })],
            false,
        );
        assert_eq!(body["stream"], false);
        assert_eq!(body["messages"][0]["content"], "hi");
    }

    /// Multi-turn history (including attachment parts) enters the request body unchanged and in order (IIE4AD-360/358).
    #[test]
    fn multi_turn_body_keeps_history_in_order() {
        let messages = vec![
            serde_json::json!({ "role": "user", "content": "hello" }),
            serde_json::json!({ "role": "assistant", "content": "Hello! How can I help?" }),
            serde_json::json!({ "role": "user", "content": [
                { "type": "text", "text": "explain closures again" },
                { "type": "image_url", "image_url": { "url": "data:image/png;base64,AA==" } }
            ] }),
        ];
        let body = chat_body_from_messages("deepseek-chat", messages);
        assert_eq!(body["messages"].as_array().unwrap().len(), 3);
        assert_eq!(body["messages"][1]["role"], "assistant");
        assert_eq!(
            body["messages"][2]["content"][0]["text"],
            "explain closures again"
        );
        assert_eq!(body["messages"][2]["content"][1]["type"], "image_url");
    }

    /// DeepSeek-style separate reasoning field (MOE-0026).
    #[test]
    fn parses_reasoning_content_deltas() {
        assert_eq!(
            sse_delta("data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"step one\"}}]}"),
            SseLine::Reasoning("step one".into())
        );
        assert_eq!(
            sse_delta("data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"\"}}]}"),
            SseLine::Ignore
        );
    }

    /// OpenAI-style tool-call fragments: the first chunk carries id/name, later chunks carry
    /// argument slices keyed by index (MOE-0026).
    #[test]
    fn parses_tool_call_fragments() {
        assert_eq!(
            sse_delta(
                "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"web_search\",\"arguments\":\"\"}}]}}]}"
            ),
            SseLine::ToolCalls(vec![ToolCallFragment {
                index: 0,
                id: Some("call_1".into()),
                name: Some("web_search".into()),
                arguments: String::new(),
            }])
        );
        assert_eq!(
            sse_delta(
                "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"query\\\":\"}}]}}]}"
            ),
            SseLine::ToolCalls(vec![ToolCallFragment {
                index: 0,
                id: None,
                name: None,
                arguments: "{\"query\":".into(),
            }])
        );
        // A delta without tool calls still lands on the ignore path
        assert_eq!(
            sse_delta("data: {\"choices\":[{\"delta\":{\"role\":\"assistant\"}}]}"),
            SseLine::Ignore
        );
    }

    /// `tools` ride the body only when advertised (MOE-0026): none → no `tools` key at all.
    #[test]
    fn builds_tools_carrying_body() {
        let tools =
            vec![serde_json::json!({ "type": "function", "function": { "name": "web_search" } })];
        let body = chat_body_with_tools(
            "deepseek-flash",
            vec![serde_json::json!({ "role": "user", "content": "hi" })],
            &tools,
            true,
        );
        assert_eq!(body["tools"][0]["function"]["name"], "web_search");
        assert_eq!(body["stream"], true);

        let plain = chat_body_with_tools(
            "m",
            vec![serde_json::json!({ "role": "user", "content": "hi" })],
            &[],
            true,
        );
        assert!(
            plain.get("tools").is_none(),
            "no advertised tools → no tools field"
        );
    }

    /// The blocking path reads `choices[0].message.content` (moved here from ai_commands, MOE-0027).
    #[test]
    fn completion_text_reads_message_content() {
        let payload =
            r#"{"choices":[{"message":{"role":"assistant","content":"the rewritten text"}}]}"#;
        assert_eq!(
            completion_text(payload).as_deref(),
            Some("the rewritten text")
        );
        assert_eq!(completion_text(r#"{"choices":[]}"#), None);
        assert_eq!(completion_text("not-json"), None);
        assert_eq!(
            completion_text(r#"{"choices":[{"message":{"content":""}}]}"#),
            None
        );
    }
}
