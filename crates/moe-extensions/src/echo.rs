//! Echo: living documentation of the Command contract — how an Extension produces WriteBack and Item streams.

use moe_core::contract::{
    Action, ActionKind, ActionResult, CommandMeta, Extension, InputKind, Item, MoeError, Selection,
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

fn meta(id: &str, title: &str, subtitle: &str, icon: &str, input: InputKind) -> CommandMeta {
    CommandMeta {
        id: id.into(),
        extension_id: "echo".into(),
        title: title.into(),
        subtitle: Some(subtitle.into()),
        icon: Some(icon.into()),
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
                "Write the input text back at the cursor",
                "terminal",
                InputKind::Query,
            ),
            meta(
                "echo.items",
                "Echo: List Demo",
                "Demo of an Item stream: results still support Apply / secondary actions",
                "list",
                InputKind::Query,
            ),
            meta(
                "echo.shout",
                "Echo: Shout",
                "With a selection: write back uppercased; without: insert a marker",
                "megaphone",
                InputKind::Selection,
            ),
        ]
    }

    fn invoke(
        &self,
        command_id: &str,
        query: Option<&str>,
        selection: Option<&Selection>,
    ) -> Result<ActionResult, MoeError> {
        let text = query.unwrap_or_default().to_string();
        match command_id {
            "echo.write-back" => Ok(ActionResult::WriteBack { text }),
            "echo.shout" => Ok(ActionResult::WriteBack {
                text: match selection.and_then(|s| s.text()) {
                    Some(sel) => sel.to_uppercase(),
                    None => "MOE WAS HERE".to_string(),
                },
            }),
            "echo.items" => Ok(ActionResult::list(
                ["alpha", "beta", "gamma"]
                    .iter()
                    .map(|word| Item {
                        id: format!("echo.word.{word}"),
                        title: (*word).to_string(),
                        subtitle: Some("Sample word".into()),
                        icon: Some("quote".into()),
                        actions: vec![
                            action(
                                "write-back",
                                "Write back this word",
                                ActionKind::Primary,
                                None,
                            ),
                            action(
                                "copy",
                                "Copy as plain text",
                                ActionKind::Secondary,
                                Some("⌥⏎"),
                            ),
                        ],
                        payload: serde_json::json!({ "word": word }),
                        detail: None,
                        pending: false,
                    })
                    .collect(),
            )),
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
            // Same semantics as the AI answer: only write to the clipboard, leave the host app alone (ADR-0002 amendment)
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

    /// Contract consistency (ADR-0006): every item in an Item stream has a Primary and a secondary action (⌥⏎ copies).
    #[test]
    fn list_items_follow_unified_action_semantics() {
        let ext = Echo;
        let ActionResult::List { items, detail_full } =
            ext.invoke("echo.items", None, None).unwrap()
        else {
            panic!("expected list");
        };
        assert!(
            !detail_full,
            "the word list is a list view (left list + right preview) — ADR-0013"
        );
        assert!(!items.is_empty());
        for item in &items {
            assert!(
                item.actions.iter().any(|a| a.kind == ActionKind::Primary),
                "{} missing Primary",
                item.id
            );
            let secondary = item
                .actions
                .iter()
                .find(|a| a.kind == ActionKind::Secondary)
                .expect("missing secondary action");
            assert_eq!(secondary.id, "copy");
            assert_eq!(secondary.keybinding.as_deref(), Some("⌥⏎"));
        }
    }
}
