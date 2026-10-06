//! Echo：Command 契约的活文档——一个 Extension 如何产出 WriteBack 与 Item 流。

use moe_core::contract::{
    Action, ActionKind, ActionResult, CommandMeta, Extension, InputKind, Item, MoeError,
};

pub struct Echo;

fn action(id: &str, title: &str, kind: ActionKind, keybinding: Option<&str>) -> Action {
    Action {
        id: id.into(),
        title: title.into(),
        kind,
        keybinding: keybinding.map(Into::into),
    }
}

fn meta(id: &str, title: &str, subtitle: &str, input: InputKind) -> CommandMeta {
    CommandMeta {
        id: id.into(),
        extension_id: "echo".into(),
        title: title.into(),
        subtitle: Some(subtitle.into()),
        input,
        live: false,
    }
}

impl Extension for Echo {
    fn id(&self) -> &str {
        "echo"
    }

    fn title(&self) -> &str {
        "Echo"
    }

    fn commands(&self) -> Vec<CommandMeta> {
        vec![
            meta(
                "echo.write-back",
                "Echo: Write Back",
                "把输入文本回写到光标处",
                InputKind::Query,
            ),
            meta(
                "echo.items",
                "Echo: List Demo",
                "演示 Item 流：结果仍可 Apply / 副操作",
                InputKind::Query,
            ),
            meta(
                "echo.shout",
                "Echo: Shout",
                "有选区则大写回写，无选区则插入标记",
                InputKind::Selection,
            ),
        ]
    }

    fn invoke(
        &self,
        command_id: &str,
        query: Option<&str>,
        selection: Option<&str>,
    ) -> Result<ActionResult, MoeError> {
        let text = query.unwrap_or_default().to_string();
        match command_id {
            "echo.write-back" => Ok(ActionResult::WriteBack { text }),
            "echo.shout" => Ok(ActionResult::WriteBack {
                text: match selection {
                    Some(sel) => sel.to_uppercase(),
                    None => "MOE WAS HERE".to_string(),
                },
            }),
            "echo.items" => Ok(ActionResult::List {
                items: ["alpha", "beta", "gamma"]
                    .iter()
                    .map(|word| Item {
                        id: format!("echo.word.{word}"),
                        title: (*word).to_string(),
                        subtitle: Some("示例词".into()),
                        actions: vec![
                            action("write-back", "回写该词", ActionKind::Primary, None),
                            action("copy", "复制纯文本", ActionKind::Secondary, Some("⌥⏎")),
                        ],
                        payload: serde_json::json!({ "word": word }),
                        detail: None,
                    })
                    .collect(),
            }),
            _ => Err(MoeError::NotFound),
        }
    }

    fn run_item_action(
        &self,
        _command_id: &str,
        item: &Item,
        action: &Action,
    ) -> Result<ActionResult, MoeError> {
        match action.id.as_str() {
            "write-back" => Ok(ActionResult::WriteBack {
                text: item.title.clone(),
            }),
            // 与 AI 回答同一语义：只写剪贴板，不动宿主应用（ADR-0002 增补）
            "copy" => {
                moe_platform::clipboard::copy(&item.title)
                    .map_err(|err| MoeError::Internal(err.to_string()))?;
                Ok(ActionResult::Silent)
            }
            _ => Err(MoeError::NotFound),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 契约一致性（ADR-0006）：Item 流里每个 item 都有 Primary 与副操作（⌥⏎ 复制）。
    #[test]
    fn list_items_follow_unified_action_semantics() {
        let ext = Echo;
        let ActionResult::List { items } = ext.invoke("echo.items", None, None).unwrap() else {
            panic!("expected list");
        };
        assert!(!items.is_empty());
        for item in &items {
            assert!(
                item.actions.iter().any(|a| a.kind == ActionKind::Primary),
                "{} 缺 Primary",
                item.id
            );
            let secondary = item
                .actions
                .iter()
                .find(|a| a.kind == ActionKind::Secondary)
                .expect("缺副操作");
            assert_eq!(secondary.id, "copy");
            assert_eq!(secondary.keybinding.as_deref(), Some("⌥⏎"));
        }
    }
}
