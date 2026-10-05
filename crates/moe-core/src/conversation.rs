//! 会话与消息：AI 问答的领域类型（归属 `ai` Namespace，ADR-0003/0005）。

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    pub id: String,
    pub namespace: String,
    pub title: String,
    pub updated_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub role: Role,
    pub content: String,
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
