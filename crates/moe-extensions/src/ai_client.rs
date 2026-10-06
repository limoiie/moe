//! Request construction and SSE parsing for OpenAI-compatible endpoints (pure functions, testable).

/// Outcome of processing one SSE line.
#[derive(Debug, PartialEq, Eq)]
pub enum SseLine {
    /// Incremental text carried by this line.
    Delta(String),
    /// Stream end (`[DONE]`).
    Done,
    /// Ignore (heartbeats, empty lines, non-JSON, first packet without content, etc.).
    Ignore,
}

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
    match json["choices"][0]["delta"]["content"].as_str() {
        Some(content) if !content.is_empty() => SseLine::Delta(content.to_string()),
        _ => SseLine::Ignore,
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
