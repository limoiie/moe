//! Moe's own meta-extension: settings entry points (ADR-0009 — no settings UI in v1).
//!
//! - "Open Config File" (regular command; declares its ⌘, invocation shortcut from the platform keymap)
//! - `key <your-key>` (capturing fallback): stores the API key in a local key file (0600), never echoed

use moe_core::contract::{
    ActionResult, CommandMeta, Extension, InputKind, Item, MoeError, Selection,
};
use moe_core::keymap::{SystemKey, display_of};
use moe_platform::keychain;

pub struct Moe;

const KEY_TRIGGER: &str = "key";

fn info_item(text: &str) -> Item {
    Item {
        id: "moe.info".into(),
        title: text.into(),
        subtitle: None,
        group: None,
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
                title: "Open Config File".into(),
                subtitle: Some("Summon hotkey, [ai] endpoint and other settings".into()),
                icon: Some("settings-2".into()),
                input: InputKind::None,
                live: false,
                // Its invocation shortcut comes from the platform table, so the Kbd can never drift (ADR-0030).
                keybinding: display_of(SystemKey::OpenConfig).map(str::to_string),
                extension_title: None,
                kind: None,
            },
            CommandMeta {
                id: "moe.set-ai-key".into(),
                extension_id: "moe".into(),
                title: "Save AI Key".into(),
                subtitle: Some(
                    "Type `key <your-key>` (never echoed, stored in a local key file)".into(),
                ),
                icon: Some("key-round".into()),
                input: InputKind::Query,
                live: false,
                keybinding: None,
                extension_title: None,
                kind: None,
            },
        ]
    }

    fn invoke(
        &self,
        command_id: &str,
        query: Option<&str>,
        _selection: Option<&Selection>,
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
                        "Type the API key right after `key`, e.g. `key sk-xxxx`.\n\n\
                         The key is only stored in a local key file (0600 permissions), never echoed, never written to the config file; you can also set the `MOE_AI_API_KEY` environment variable.",
                    )]));
                }
                keychain::set_ai_api_key(&key).map_err(MoeError::Internal)?;
                Ok(ActionResult::detail(vec![info_item(
                    "✅ AI API Key saved (0600 key file, never echoed). You can ask questions now.",
                )]))
            }
            _ => Err(MoeError::NotFound),
        }
    }

    fn fallback_command(&self, query: &str, _selection: Option<&Selection>) -> Option<CommandMeta> {
        if !is_key_capture(query) {
            return None;
        }
        Some(CommandMeta {
            id: "moe.set-ai-key".into(),
            extension_id: "moe".into(),
            title: "Save AI Key (never echoed)".into(),
            subtitle: Some("Enter to store in a local key file (0600)".into()),
            icon: Some("key-round".into()),
            input: InputKind::Query,
            live: false,
            keybinding: None,
            extension_title: None,
            kind: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use moe_core::contract::Extension;

    /// Guard: the id synthesized by fallback must be routable within commands(),
    /// otherwise Apply would hit NotFound (we've been bitten by this before).
    #[test]
    fn fallback_command_is_invocable() {
        let ext = Moe;
        let fallback = ext.fallback_command("key sk-test", None).expect("fallback");
        assert!(
            ext.commands().iter().any(|c| c.id == fallback.id),
            "fallback id must be routable within commands(): {}",
            fallback.id
        );
    }

    /// Titles carry no extension prefix (ADR-0030): the row shows the extension name separately.
    /// Open Config File declares its ⌘, shortcut straight from the platform keymap, so the Kbd
    /// can never drift from the table.
    #[test]
    fn titles_are_prefix_free_and_open_config_declares_its_shortcut() {
        let ext = Moe;
        let commands = ext.commands();
        assert!(
            commands.iter().all(|c| !c.title.starts_with("Moe: ")),
            "titles must not repeat the extension name"
        );
        let open = commands
            .iter()
            .find(|c| c.id == "moe.open-config")
            .expect("open-config registered");
        assert_eq!(open.title, "Open Config File");
        assert_eq!(
            open.keybinding.as_deref(),
            display_of(SystemKey::OpenConfig),
            "the Kbd comes from the keymap table"
        );
    }
}
