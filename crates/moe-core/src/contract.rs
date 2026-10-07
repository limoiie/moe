//! Command contract: input kinds × streaming Item output; write-back is performed centrally by the platform (ADR-0006).

use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InputKind {
    /// Executes directly, with no input dependency.
    None,
    /// Depends on the Input Bar query text.
    Query,
    /// Grabs the Selection automatically when summoned; falls back to cursor mode when missing.
    Selection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ActionKind {
    /// Primary action triggered by Enter (Apply). Every Item has exactly one Primary action.
    Primary,
    /// Named alternative actions, each with its own fixed shortcut.
    Secondary,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Action {
    /// Stable identifier within the Extension, routed back to `run_item_action`.
    pub id: String,
    pub title: String,
    pub kind: ActionKind,
    /// Display shortcut (e.g. "⌥⏎"). System-level keybindings come from the platform Keymap; this field only holds extension-defined ones.
    pub keybinding: Option<String>,
}

/// serde helper for `Item.pending`: omitted when false (older readers unaffected).
fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    /// Semantic icon name (platform icon set, e.g. Lucide's "sparkles"; ADR-0012). The UI picks the actual glyph; unknown names fall back to a default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// The first action carries the Apply semantic; Show All Actions (⌘K) expands all of them.
    pub actions: Vec<Action>,
    /// Opaque to the UI; sent back to the Extension verbatim when an action runs.
    pub payload: serde_json::Value,
    /// Detail content (Markdown, rendered in the detail view card); streaming answers accumulate here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Still being produced (streaming placeholder): the platform's keybinding layer prioritizes requesting stop generation on Esc (IIE4AD-365).
    #[serde(default, skip_serializing_if = "is_false")]
    pub pending: bool,
}

/// Result after Apply or any action runs. The output type is not a closed enum — Items in a
/// List can in turn produce WriteBack / List; composition replaces the enum (ADR-0006).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ActionResult {
    /// Handed to the platform for write-back via TextTarget: replace the Selection, or insert at the cursor.
    WriteBack {
        text: String,
    },
    List {
        items: Vec<Item>,
        /// Whether the detail fills the screen: true when there is a single result and it is itself the content (AI answers, notifications).
        /// Multiple results (e.g. history search) keep "list on the left + detail on the right", declared by the Extension instead of guessed by the UI (ADR-0013).
        #[serde(default, skip_serializing_if = "is_false")]
        detail_full: bool,
    },
    /// Triggers Materialize: opens the Extension's Side View, carrying the payload needed to open it.
    OpenSideView {
        payload: serde_json::Value,
    },
    Silent,
}

impl ActionResult {
    /// List view: list on the left + detail on the right (multiple results, e.g. history search, action list).
    pub fn list(items: Vec<Item>) -> Self {
        Self::List {
            items,
            detail_full: false,
        }
    }

    /// Detail view: a single result that is itself the content (AI answers, system notifications); the detail fills the panel.
    pub fn detail(items: Vec<Item>) -> Self {
        Self::List {
            items,
            detail_full: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandMeta {
    /// Globally unique, by convention `"{extension_id}.{command}"`.
    pub id: String,
    pub extension_id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    /// Semantic icon name (ADR-0012). When absent, the UI uses the source Command's icon, then falls back to a default glyph.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub input: InputKind,
    /// Live list: each Input Bar change re-runs this command with the new query (e.g. history search);
    /// when false, input is only used for command palette search (default).
    pub live: bool,
}

/// One group of command palette search results (Raycast-style section, ADR-0020):
/// **source = group** — one section per Extension, headed by the extension name;
/// future sources (file search, etc.) just add a new section, no separate grouping rules needed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandSection {
    pub title: String,
    pub items: Vec<CommandMeta>,
}

/// Extension identity for the UI's avatar chip (ADR-0026): id + display name, plus a
/// representative icon (the extension's first command icon) used as its avatar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionMeta {
    pub id: String,
    pub title: String,
    pub icon: Option<String>,
}

/// The "selection" context captured before summoning the panel (ADR-0021): a text selection and
/// files selected (in Finder) can coexist; files are absolute paths (POSIX). The semantics passed
/// to fallback / invoke match the selection of ADR-0002/0019.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Selection {
    pub text: Option<String>,
    pub files: Vec<String>,
}

impl Selection {
    /// Raw text selection (may carry surrounding whitespace; consumers trim to check emptiness).
    pub fn text(&self) -> Option<&str> {
        self.text.as_deref()
    }

    /// Whether a file selection was attached.
    pub fn has_files(&self) -> bool {
        !self.files.is_empty()
    }
}

/// Platform general entries (ADR-0014): the landing points for the general actions
/// Browse (⌘P) / New (⌘N) — the platform fixes keybindings and routing; entries are declared by Extensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EntryKind {
    /// Browse (⌘P): the Extension's record list (AI = conversation history).
    Browse,
    /// New (⌘N): create a new record (AI = new conversation).
    New,
}

#[derive(Debug)]
pub enum MoeError {
    /// Platform capability requires permission (macOS Accessibility); callers should show inline guidance.
    PermissionRequired,
    Unsupported,
    NotFound,
    Internal(String),
}

impl std::fmt::Display for MoeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PermissionRequired => write!(f, "permission required"),
            Self::Unsupported => write!(f, "unsupported on this platform"),
            Self::NotFound => write!(f, "not found"),
            Self::Internal(msg) => write!(f, "internal error: {msg}"),
        }
    }
}

impl std::error::Error for MoeError {}

/// Incremental events during command execution (streaming answers follow Item semantics: updated in place by id).
/// Note: `rename_all` on the enum only renames variant names; struct variant fields need `rename_all_fields`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum CommandEvent {
    ItemUpdated {
        command_id: String,
        item: Item,
    },
    /// Automatic write-back request after background generation **completes naturally** (AI command streaming done → replace selection, ADR-0024).
    /// The platform layer intercepts and executes it (write-back + dismiss the panel) without forwarding to the UI; generation stopped via Esc does not emit it.
    WriteBack {
        text: String,
    },
}

/// Incremental event outlet. Worker threads may emit events too, so it must be thread-safe.
pub trait Emitter: Send + Sync {
    fn emit(&self, event: CommandEvent);
}

/// No-op event outlet (non-streaming callers).
pub struct NoopEmitter;

impl Emitter for NoopEmitter {
    fn emit(&self, _event: CommandEvent) {}
}

pub trait Extension: Send + Sync {
    /// Namespace key (ADR-0003): storage and history are isolated beneath it.
    fn id(&self) -> &str;
    fn title(&self) -> &str;
    fn commands(&self) -> Vec<CommandMeta>;

    /// Apply a Command in the command palette.
    fn invoke(
        &self,
        command_id: &str,
        query: Option<&str>,
        selection: Option<&Selection>,
    ) -> Result<ActionResult, MoeError>;

    /// Execution entry with incremental events; falls back to [`Extension::invoke`] by default (non-streaming extensions need not implement it).
    fn invoke_streaming(
        &self,
        command_id: &str,
        query: Option<&str>,
        selection: Option<&Selection>,
        _emitter: Arc<dyn Emitter>,
    ) -> Result<ActionResult, MoeError> {
        self.invoke(command_id, query, selection)
    }

    /// The "catch-all" command offered when search has no matches (e.g. AI: Ask "…"); none by default.
    /// `selection` is the selection context captured before summoning the panel (text selection + files selected in Finder),
    /// letting the extension hint "context/files attached" in its subtitle.
    fn fallback_command(
        &self,
        _query: &str,
        _selection: Option<&Selection>,
    ) -> Option<CommandMeta> {
        None
    }

    /// "Record list" entry (Browse, ⌘P): AI = conversation history. None by default —
    /// Extensions without records don't declare it, and the platform shows a one-off inline hint for that keybinding (ADR-0014).
    fn browse_command(&self) -> Option<CommandMeta> {
        None
    }

    /// "New record" entry (New, ⌘N): AI = new conversation. None by default (same as above).
    fn new_command(&self) -> Option<CommandMeta> {
        None
    }

    /// Delete the current record (general action Delete, default ⌃X, ADR-0022):
    /// acts on the given item (e.g. one conversation in AI history), returning the number of records actually deleted. Not available by default.
    fn delete_item(&self, _command_id: &str, _item: &Item) -> Result<usize, MoeError> {
        Err(MoeError::NotFound)
    }

    /// Delete all records (general action DeleteAll, default ⌃⇧X, ADR-0022):
    /// acts on the record space of that Command (isolated by Namespace), returning the number of records actually deleted. Not available by default.
    fn delete_all(&self, _command_id: &str) -> Result<usize, MoeError> {
        Err(MoeError::NotFound)
    }

    /// Next step of the Item stream: run one of the Item's actions (no action available by default).
    fn run_item_action(
        &self,
        _command_id: &str,
        _item: &Item,
        _action: &Action,
    ) -> Result<ActionResult, MoeError> {
        Err(MoeError::NotFound)
    }

    /// Continue a conversation in the Side View (ADR-0004): an empty id means a new conversation; returns the actual conversation id.
    /// Replies are streamed in the background and delivered incrementally via `CommandEvent` (command_id convention: `ai.side`).
    fn side_continue(
        &self,
        _conversation_id: &str,
        _message: &str,
        _emitter: Arc<dyn Emitter>,
    ) -> Result<String, MoeError> {
        Err(MoeError::NotFound)
    }

    /// Request stopping this Extension's in-flight generation; returns the number of generations aborted (not available by default).
    /// The platform-level Esc prioritizes calling it when the Focused Item is `pending` (IIE4AD-365).
    fn stop_generation(&self) -> usize {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression: the enum's rename_all only renames variant names; struct variant fields need rename_all_fields;
    /// the UI reads `payload.itemUpdated.commandId` (snake_case once silently dropped updates).
    #[test]
    fn command_event_serializes_camel_case_for_ui() {
        let event = CommandEvent::ItemUpdated {
            command_id: "ai.quick-ask".into(),
            item: Item {
                id: "ai.answer".into(),
                title: "AI Answer".into(),
                subtitle: None,
                actions: vec![],
                payload: serde_json::Value::Null,
                detail: None,
                pending: false,
                icon: None,
            },
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["itemUpdated"]["commandId"], "ai.quick-ask");
        assert_eq!(json["itemUpdated"]["item"]["id"], "ai.answer");
    }

    /// Automatic write-back event (ADR-0024): the shape is stable; used by the platform layer for interception.
    #[test]
    fn write_back_event_serializes() {
        let json = serde_json::to_value(CommandEvent::WriteBack {
            text: "rewritten text".into(),
        })
        .unwrap();
        assert_eq!(json["writeBack"]["text"], "rewritten text");
    }

    /// Selection context (ADR-0021): text + files round-trip together; the camelCase shape is stable.
    #[test]
    fn selection_round_trips_through_serde() {
        let selection = Selection {
            text: Some("hello".into()),
            files: vec!["/tmp/a b.md".into()],
        };
        let json = serde_json::to_value(&selection).unwrap();
        assert_eq!(json["text"], "hello");
        assert_eq!(json["files"][0], "/tmp/a b.md");
        let back: Selection = serde_json::from_value(json).unwrap();
        assert_eq!(back, selection);
        assert_eq!(back.text(), Some("hello"));
        assert!(back.has_files());
        // Empty contexts (files only / text only) are both expressible
        assert!(!Selection::default().has_files());
        assert_eq!(Selection::default().text(), None);
    }

    fn answer() -> Item {
        Item {
            id: "ai.answer".into(),
            title: "AI Answer".into(),
            subtitle: None,
            icon: None,
            actions: vec![],
            payload: serde_json::Value::Null,
            detail: Some("Answer body".into()),
            pending: false,
        }
    }

    /// View layout is declared by the Extension (ADR-0013): the UI uses `list.detailFull` to decide
    /// between full-screen detail and "list on the left + detail on the right", instead of guessing from item count.
    #[test]
    fn action_result_declares_view_layout() {
        let detail = serde_json::to_value(ActionResult::detail(vec![answer()])).unwrap();
        assert_eq!(detail["list"]["detailFull"], true);

        let list = serde_json::to_value(ActionResult::list(vec![answer()])).unwrap();
        // List view omits this field (old list results still deserialize as lists, backward compatible)
        assert!(list["list"].get("detailFull").is_none());
        let legacy: ActionResult = serde_json::from_value(list).unwrap();
        assert_eq!(legacy, ActionResult::list(vec![answer()]));
    }
}
