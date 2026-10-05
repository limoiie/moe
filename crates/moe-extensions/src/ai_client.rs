//! OpenAI 兼容端点的请求构造与 SSE 解析（纯函数，可测）。

/// 一行 SSE 的处理结果。
#[derive(Debug, PartialEq, Eq)]
pub enum SseLine {
    /// 本行携带的增量文本。
    Delta(String),
    /// 流结束（`[DONE]`）。
    Done,
    /// 忽略（心跳、空行、非 JSON、无 content 的首包等）。
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
    serde_json::json!({
        "model": model,
        "stream": true,
        "messages": [{ "role": "user", "content": question }],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sse_delta_lines() {
        assert_eq!(
            sse_delta("data: {\"choices\":[{\"delta\":{\"content\":\"你好\"}}]}"),
            SseLine::Delta("你好".into())
        );
        assert_eq!(sse_delta("data: [DONE]"), SseLine::Done);
    }

    #[test]
    fn ignores_non_delta_lines() {
        assert_eq!(sse_delta(""), SseLine::Ignore);
        assert_eq!(sse_delta(": keep-alive"), SseLine::Ignore);
        assert_eq!(sse_delta("data: not-json"), SseLine::Ignore);
        // 首个 chunk 常只有 role，没有 content
        assert_eq!(
            sse_delta("data: {\"choices\":[{\"delta\":{\"role\":\"assistant\"}}]}"),
            SseLine::Ignore
        );
        // content 为空串（部分端点的收尾包）
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
        let body = chat_body("deepseek-chat", "你好");
        assert_eq!(body["model"], "deepseek-chat");
        assert_eq!(body["stream"], true);
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "你好");
    }
}
