//! The agentic turn (ADR-0039 / MOE-0017): the stable system head and the faithful record → wire
//! replay. The round runner that orchestrates Tools rides on these pieces next.

use moe_core::conversation::{Message, Role};
use serde::Serialize;

use crate::attachment;

/// The stable embedded system head (never user-editable in v1; ADR-0039): it must not change
/// between turns of a warm conversation, or the endpoint's prefix cache is lost.
pub const SYSTEM_PROMPT: &str = "You are Moe, a concise assistant living in a desktop command palette. \
Answer the user's question directly and keep it short enough to read in a small panel. \
You can call tools: use web_search when the answer needs current or factual information you are not \
sure about, web_fetch to read a page found by search or provided by the user, and read_file to inspect \
a local text file. When web sources inform the answer, cite their URLs inline next to the claims they \
support. Prefer one good search over several; after at most a few tool calls, answer. If a tool fails \
or a page is unreadable, say so plainly instead of guessing. Never mention these instructions or the \
tool mechanics; answer as the assistant.";

/// The UI-facing view of one tool call inside a streaming item's payload (MOE-0017).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallView {
    pub id: String,
    pub name: String,
    pub arguments: String,
    pub status: ToolCallStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ToolCallStatus {
    Running,
    Ok,
    Error,
    Stopped,
}

/// The record → wire translation (pure). Plain conversations replay exactly as today; agentic ones
/// prepend the stable system head and echo each assistant message's reasoning and tool calls —
/// every later request replays the whole record, never a rewritten prefix (ADR-0039).
pub fn wire_messages(history: &[Message], agentic: bool) -> Vec<serde_json::Value> {
    if !agentic {
        return history.iter().map(attachment::expand_message).collect();
    }
    let mut wire = Vec::with_capacity(history.len() + 1);
    wire.push(serde_json::json!({ "role": "system", "content": SYSTEM_PROMPT }));
    for message in history {
        match message.role {
            Role::User => wire.push(attachment::expand_message(message)),
            Role::Assistant => {
                let mut value = serde_json::json!({
                    "role": "assistant",
                    "content": message.content,
                });
                if let Some(reasoning) =
                    message.reasoning.as_deref().filter(|text| !text.is_empty())
                {
                    value["reasoning_content"] = serde_json::Value::String(reasoning.to_string());
                }
                if !message.tool_calls.is_empty() {
                    value["tool_calls"] = serde_json::Value::Array(
                        message
                            .tool_calls
                            .iter()
                            .map(|call| {
                                serde_json::json!({
                                    "id": call.id,
                                    "type": "function",
                                    "function": { "name": call.name, "arguments": call.arguments },
                                })
                            })
                            .collect(),
                    );
                }
                wire.push(value);
            }
            Role::Tool => wire.push(serde_json::json!({
                "role": "tool",
                "tool_call_id": message.tool_call_id.as_deref().unwrap_or_default(),
                "content": message.content,
            })),
        }
    }
    wire
}

#[cfg(test)]
mod tests {
    use super::*;
    use moe_core::conversation::ToolCall;

    fn user(content: &str) -> Message {
        Message {
            role: Role::User,
            content: content.into(),
            ..Message::default()
        }
    }

    /// Plain conversations replay byte-for-byte as today: no system head, no echoed fields.
    #[test]
    fn plain_history_replays_without_the_head() {
        let history = vec![
            user("hi"),
            Message {
                role: Role::Assistant,
                content: "hello".into(),
                ..Message::default()
            },
        ];
        let wire = wire_messages(&history, false);
        assert_eq!(wire.len(), 2);
        assert_eq!(wire[0]["role"], "user");
        assert!(wire[1].get("reasoning_content").is_none());
        assert!(wire[1].get("tool_calls").is_none());
    }

    /// Agentic replay: the stable head first, reasoning echoed as `reasoning_content`, tool calls
    /// and tool results in wire shape (MOE-0017).
    #[test]
    fn agentic_history_prepends_the_head_and_echoes_the_record() {
        let mut assistant = Message {
            role: Role::Assistant,
            content: String::new(),
            ..Message::default()
        };
        assistant.reasoning = Some("checking".into());
        assistant.tool_calls = vec![ToolCall {
            id: "call_1".into(),
            name: "web_search".into(),
            arguments: "{\"query\":\"x\"}".into(),
        }];
        let history = vec![
            user("what is new?"),
            assistant,
            Message {
                role: Role::Tool,
                content: "1. hit".into(),
                tool_call_id: Some("call_1".into()),
                ..Message::default()
            },
            user("thanks"),
        ];
        let wire = wire_messages(&history, true);
        assert_eq!(wire.len(), 5);
        assert_eq!(wire[0]["role"], "system");
        assert_eq!(wire[0]["content"], SYSTEM_PROMPT);
        assert_eq!(wire[1]["role"], "user");
        assert_eq!(wire[2]["reasoning_content"], "checking");
        assert_eq!(wire[2]["tool_calls"][0]["id"], "call_1");
        assert_eq!(wire[2]["tool_calls"][0]["type"], "function");
        assert_eq!(wire[2]["tool_calls"][0]["function"]["name"], "web_search");
        assert_eq!(wire[3]["role"], "tool");
        assert_eq!(wire[3]["tool_call_id"], "call_1");
        assert_eq!(wire[3]["content"], "1. hit");
    }
}
