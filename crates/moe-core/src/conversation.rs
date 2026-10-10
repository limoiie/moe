//! Conversations and messages: domain types for AI Q&A (owned by the `ai` Namespace, ADR-0003/0005).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    pub id: String,
    pub namespace: String,
    pub title: String,
    pub updated_unix: u64,
    /// Agentic Conversations advertise Tools and replay faithfully (ADR-0039): fixed at creation;
    /// pre-feature conversations stay plain forever.
    pub agentic: bool,
}

/// Attachment reference on a message (stores the reference only; content is read on demand, ADR-0010).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentRef {
    /// Display name (file name).
    pub name: String,
    /// Absolute path (`~` expanded at write time).
    pub path: String,
}

/// One tool call in the shape the wire uses (`id` / `function.name` / `function.arguments`) and the
/// record persists (MOE-0017).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub role: Role,
    /// User-visible text (mentions already stripped, ADR-0010).
    pub content: String,
    #[serde(default)]
    pub attachments: Vec<AttachmentRef>,
    /// Replay material for Agentic Conversations (ADR-0039): echoed as `reasoning_content`, never
    /// copied or written back (the Thinking block renders it, ADR-0024 amendments).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    /// Assistant-round tool calls, replayed verbatim when the conversation is agentic (MOE-0017).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    /// On `Role::Tool` result messages: the call they answer (wire `tool_call_id`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Role {
    #[default]
    User,
    Assistant,
    /// A tool result message (wire role `tool`), part of an Agentic Conversation's record.
    Tool,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::Tool => "tool",
        }
    }

    pub fn parse(raw: &str) -> Self {
        match raw {
            "assistant" => Self::Assistant,
            "tool" => Self::Tool,
            _ => Self::User,
        }
    }
}
