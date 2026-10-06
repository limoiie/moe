//! 会话与消息：AI 问答的领域类型（归属 `ai` Namespace，ADR-0003/0005）。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    pub id: String,
    pub namespace: String,
    pub title: String,
    pub updated_unix: u64,
}

/// 消息上的附件引用（只存引用，内容在请求时现读，ADR-0010）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentRef {
    /// 展示名（文件名）。
    pub name: String,
    /// 绝对路径（写入时已做 `~` 展开）。
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub role: Role,
    /// 用户可见文本（mention 已剥离，ADR-0010）。
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
