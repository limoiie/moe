//! AI 问答壳：登记命令盘入口，真实流式回答、Side View、历史是 M3。

use moe_core::contract::{
    Action, ActionKind, ActionResult, CommandMeta, Extension, InputKind, Item, MoeError,
};

pub struct AiShell;

impl Extension for AiShell {
    fn id(&self) -> &str {
        // Namespace 键（ADR-0003/0005：Conversation 历史存于此 Namespace）。
        "ai"
    }

    fn title(&self) -> &str {
        "AI"
    }

    fn commands(&self) -> Vec<CommandMeta> {
        vec![
            CommandMeta {
                id: "ai.quick-ask".into(),
                extension_id: "ai".into(),
                title: "AI: Quick Ask".into(),
                subtitle: Some("流式回答走 Item 语义".into()),
                input: InputKind::Query,
            },
            CommandMeta {
                id: "ai.search-history".into(),
                extension_id: "ai".into(),
                title: "AI: 搜索历史会话".into(),
                subtitle: Some("M3 实现（Namespace: ai）".into()),
                input: InputKind::Query,
            },
        ]
    }

    fn invoke(
        &self,
        command_id: &str,
        _query: Option<&str>,
        _selection: Option<&str>,
    ) -> Result<ActionResult, MoeError> {
        match command_id {
            "ai.quick-ask" => Ok(ActionResult::List {
                items: vec![Item {
                    id: "ai.pending".into(),
                    title: "尚未配置模型：在 config.toml 设置 base_url，API key 存 keychain".into(),
                    subtitle: Some("主操作：实体化为 Side View".into()),
                    actions: vec![Action {
                        id: "materialize".into(),
                        title: "Materialize".into(),
                        kind: ActionKind::Primary,
                        keybinding: Some("⌘M".into()),
                    }],
                    payload: serde_json::Value::Null,
                }],
            }),
            "ai.search-history" => Ok(ActionResult::Silent),
            _ => Err(MoeError::NotFound),
        }
    }

    fn run_item_action(
        &self,
        _command_id: &str,
        _item: &Item,
        action: &Action,
    ) -> Result<ActionResult, MoeError> {
        match action.id.as_str() {
            // Side View 窗口本身是 M3；先返回语义让 UI 可接线。
            "materialize" => Ok(ActionResult::OpenSideView),
            _ => Err(MoeError::NotFound),
        }
    }
}
