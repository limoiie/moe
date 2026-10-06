//! Moe 自身的元扩展：设置面入口（ADR-0009——v1 不做设置 UI）。
//!
//! - 「Moe: 打开配置文件」（常规命令）
//! - `key <你的key>`（捕获式 fallback）：把 API key 存入 keychain，不回显

use moe_core::contract::{ActionResult, CommandMeta, Extension, InputKind, Item, MoeError};
use moe_platform::keychain;

pub struct Moe;

const KEY_TRIGGER: &str = "key";

fn info_item(text: &str) -> Item {
    Item {
        id: "moe.info".into(),
        title: text.into(),
        subtitle: None,
        actions: vec![],
        payload: serde_json::Value::Null,
        detail: None,
        pending: false,
        icon: None,
    }
}

fn is_key_capture(query: &str) -> bool {
    let trimmed = query.trim();
    trimmed == KEY_TRIGGER || trimmed.starts_with(&format!("{KEY_TRIGGER} "))
}

fn key_from_query(query: &str) -> String {
    query
        .trim()
        .strip_prefix(KEY_TRIGGER)
        .map(str::trim)
        .unwrap_or_default()
        .to_string()
}

impl Extension for Moe {
    fn id(&self) -> &str {
        "moe"
    }

    fn title(&self) -> &str {
        "Moe"
    }

    fn commands(&self) -> Vec<CommandMeta> {
        vec![
            CommandMeta {
                id: "moe.open-config".into(),
                extension_id: "moe".into(),
                title: "Moe: 打开配置文件".into(),
                subtitle: Some("呼出键、[ai] 端点等设置".into()),
                icon: Some("settings-2".into()),
                input: InputKind::None,
                live: false,
            },
            CommandMeta {
                id: "moe.set-ai-key".into(),
                extension_id: "moe".into(),
                title: "Moe: 保存 AI Key".into(),
                subtitle: Some("输入 `key <你的key>`（不回显，存 keychain）".into()),
                icon: Some("key-round".into()),
                input: InputKind::Query,
                live: false,
            },
        ]
    }

    fn invoke(
        &self,
        command_id: &str,
        query: Option<&str>,
        _selection: Option<&str>,
    ) -> Result<ActionResult, MoeError> {
        match command_id {
            "moe.open-config" => {
                moe_platform::config::open_in_editor()
                    .map_err(|err| MoeError::Internal(err.to_string()))?;
                Ok(ActionResult::Silent)
            }
            "moe.set-ai-key" => {
                let key = query.map(key_from_query).unwrap_or_default();
                if key.is_empty() {
                    return Ok(ActionResult::detail(vec![info_item(
                        "把 API key 直接跟在 `key` 后面，例如：`key sk-xxxx`。\n\n\
                         Key 只进系统 keychain，不回显、不写配置文件；也可设 `MOE_AI_API_KEY` 环境变量。",
                    )]));
                }
                keychain::set_ai_api_key(&key).map_err(MoeError::Internal)?;
                Ok(ActionResult::detail(vec![info_item(
                    "✅ AI API Key 已保存到 keychain（不回显）。现在可以直接提问了。",
                )]))
            }
            _ => Err(MoeError::NotFound),
        }
    }

    fn fallback_command(&self, query: &str) -> Option<CommandMeta> {
        if !is_key_capture(query) {
            return None;
        }
        Some(CommandMeta {
            id: "moe.set-ai-key".into(),
            extension_id: "moe".into(),
            title: "Moe: 保存 AI Key（不回显）".into(),
            subtitle: Some("Enter 存入 keychain".into()),
            icon: Some("key-round".into()),
            input: InputKind::Query,
            live: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use moe_core::contract::Extension;

    /// 守卫：fallback 合成的 id 必须能在 commands() 里被路由，
    /// 否则 Apply 时会 NotFound（曾经踩过）。
    #[test]
    fn fallback_command_is_invocable() {
        let ext = Moe;
        let fallback = ext.fallback_command("key sk-test").expect("fallback");
        assert!(
            ext.commands().iter().any(|c| c.id == fallback.id),
            "fallback id 必须在 commands() 中可路由：{}",
            fallback.id
        );
    }
}
