//! Conversations and messages: domain types for AI Q&A (owned by the `ai` Namespace, ADR-0003/0005).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    pub id: String,
    pub namespace: String,
    pub title: String,
    pub updated_unix: u64,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub role: Role,
    /// User-visible text (mentions already stripped, ADR-0010).
    pub content: String,
    #[serde(default)]
    pub attachments: Vec<AttachmentRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Role {
    User,
    Assistant,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
        }
    }

    pub fn parse(raw: &str) -> Self {
        match raw {
            "assistant" => Self::Assistant,
            _ => Self::User,
        }
    }
}
