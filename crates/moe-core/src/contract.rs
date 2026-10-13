//! Command contract: input kinds × streaming Item output; write-back is performed centrally by the platform (ADR-0006).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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

/// The platform's placeholder for a **list with no rows** (ADR-0043): declared by the Extension so the
/// copy matches what the list is about, rendered non-focusable by the platform. It replaces the
/// Extension-authored notice item — a focusable "no results" row is a row the user can Apply.
///
/// This is a *list* concept (Raycast's `List.EmptyView`). A surface that needs rich guidance —
/// multi-paragraph Markdown, fenced code — is a detail card (`ActionResult::detail`), not an empty
/// list, and keeps its card.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmptyView {
    pub title: String,
    /// One line saying why the list is empty (Raycast's `EmptyView.description`). Prose, not Markdown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Semantic icon name (ADR-0012); unknown names fall back to a default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    // Deliberately no `actions` field, though Raycast's `EmptyView` has one: Moe's only action
    // dispatch path is `run_item_action`, which needs an `Item`, and an empty list has none. A field
    // with no way to run it would be dead API. ADR-0043's amendment records the narrowing, and both
    // migrated call sites declare zero actions, so nothing is lost today.
}

impl EmptyView {
    /// Title + description — the shape every current call site needs.
    pub fn new(title: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            description: Some(description.into()),
            icon: None,
        }
    }

    pub fn with_icon(mut self, icon: &str) -> Self {
        self.icon = Some(icon.to_string());
        self
    }
}

/// One filter dimension a list supports (ADR-0043): the platform renders the Filter Control in the
/// Input Bar and the Filter Card's rows, and narrows the list itself. An Extension states which axis
/// matters and what the values are; the trigger, the card and the focus rules stay platform-owned.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Filter {
    /// Stable id within the Extension: the platform keys the remembered value by it.
    pub id: String,
    /// The dimension's name, shown at rest in the Input Bar ("Tags") and in the reset row ("All Tags").
    pub label: String,
    /// Semantic icon name (ADR-0012); also the Filter Card's leading glyph.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Explicit values in display order. Empty = the platform derives them from the returned items'
    /// `tags`, so a declared list cannot drift from the tags actually present.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
}

impl Filter {
    /// A dimension whose values are derived from the items (the common case).
    pub fn derived(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
            values: Vec::new(),
        }
    }

    pub fn with_icon(mut self, icon: &str) -> Self {
        self.icon = Some(icon.to_string());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    /// Values of this list's filter dimension (ADR-0043). Read by the UI — which is why they are a
    /// field and not a convention inside `payload`, a channel documented as opaque to the UI.
    /// Meaningless without a matching `Filter` on the result.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Trailing accessories (ADR-0041): tags, text, dates, icons. At most three render, the title
    /// truncates before any is dropped, and none render while the detail pane is showing (`split`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accessories: Vec<Accessory>,
    /// Structured facts about this result, rendered under its detail (ADR-0042): labels, links, tag
    /// lists and separators, in the Extension's order. Also makes an item with no Markdown open a
    /// detail pane, because the metadata is something to show.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub metadata: Vec<Metadata>,
    /// Optional group label for list pages (e.g. recency buckets in a history list): the platform
    /// renders a non-focusable header before the first item of each group and keeps navigation,
    /// actions and delete counting the items only (ADR-0018 amendment). None = one unlabeled group.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
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

/// One trailing accessory on a result row (ADR-0041): Raycast's `List.Item.Accessory`, narrowed to
/// Moe's vocabulary. The platform owns the palette (a bounded set of semantic color names, never a
/// hue) and the date rendering (the wire carries a timestamp, not a formatted string), so an
/// Extension cannot ship a value the theme cannot check.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    rename_all = "camelCase",
    tag = "kind",
    rename_all_fields = "camelCase"
)]
pub enum Accessory {
    /// A value in a same-color tinted pill — the tag palette's home.
    Tag {
        value: String,
        /// Semantic color name (blue, green, magenta, orange, purple, red, yellow, primary,
        /// secondary); unknown names fall back to the secondary text step. None = secondary.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        color: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tooltip: Option<String>,
    },
    /// Plain trailing text, in the secondary step unless a color names another.
    Text {
        value: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        color: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tooltip: Option<String>,
    },
    /// A unix timestamp the platform renders relatively ("now", "1d").
    Date {
        value: i64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        color: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tooltip: Option<String>,
    },
    /// A Lucide icon name (ADR-0012), optionally tinted.
    Icon {
        value: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        color: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tooltip: Option<String>,
    },
}

impl Accessory {
    /// A relative-date accessory (ADR-0041): the common case, and the one `ai.rs` migrates onto.
    pub fn date(unix: i64) -> Self {
        Self::Date {
            value: unix,
            color: None,
            tooltip: None,
        }
    }

    /// A tag accessory in the given semantic color (None = the secondary text step).
    pub fn tag(value: impl Into<String>, color: Option<String>) -> Self {
        Self::Tag {
            value: value.into(),
            color,
            tooltip: None,
        }
    }
}

/// One row of structured detail metadata (ADR-0042): Raycast's `Detail.Metadata`, with the same four
/// field types and the same placement rules. Order is the Extension's; the platform neither sorts nor
/// groups. Colors come from ADR-0041's palette, so there is no second color vocabulary here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    rename_all = "camelCase",
    tag = "kind",
    rename_all_fields = "camelCase"
)]
pub enum Metadata {
    /// A title with an optional icon and/or an optional value to its right.
    Label {
        title: String,
        /// Semantic icon name (ADR-0012).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        icon: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        /// Semantic color name for `text` (ADR-0041); unknown names fall back.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        color: Option<String>,
    },
    /// A title with a link to its right. Only http(s) targets reach the OS (`open::open_url`).
    Link {
        title: String,
        text: String,
        target: String,
    },
    /// A titled row of tags.
    TagList {
        title: String,
        items: Vec<MetadataTag>,
    },
    /// A separator line between rows.
    Separator,
}

/// One tag of a `Metadata::TagList` (ADR-0042). At least one of `text`/`icon` should be set; a tag
/// whose `action_id` names none of the item's own actions renders display-only rather than erroring.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataTag {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Semantic icon name (ADR-0012).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Semantic color name (ADR-0041); unknown names fall back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// The `Action::id` this tag dispatches through `run_item_action` (ADR-0042).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_id: Option<String>,
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
    /// The refine loop ("Improve Writing Again"): re-run the command that produced this Item with
    /// `text` as its input, **not** the Selection captured before summoning. The platform performs
    /// that invocation and hands its result back to the caller, so a new stream lands in the same card.
    Rerun {
        text: String,
    },
    List {
        items: Vec<Item>,
        /// Whether the detail fills the screen: true when there is a single result and it is itself the content (AI answers, notifications).
        /// Multiple results (e.g. history search) keep "list on the left + detail on the right", declared by the Extension instead of guessed by the UI (ADR-0013).
        #[serde(default, skip_serializing_if = "is_false")]
        detail_full: bool,
        /// Declared for the zero-row case (ADR-0043): the platform shows it instead of a blank panel,
        /// and never while a load is in flight on an empty query (Raycast's suppression rule).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        empty: Option<EmptyView>,
        /// The list's filter dimension, when it has one (ADR-0043). The values come from each item's
        /// `tags` unless the declaration names them explicitly.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        filter: Option<Filter>,
    },
    /// A conversation page hosted in the Command Panel (Quick Ask, ADR-0036): the platform renders
    /// the message stream and the Input Bar becomes the composer (Enter sends, ⌘N starts a new chat,
    /// ⌃X removes the current one, ⌃[ / ⌃] step through history). The Extension owns the storage and
    /// the continuation entry (`Extension::panel_continue`); `conversation_id` None = blank new chat.
    Conversation {
        conversation_id: Option<String>,
    },
    /// Triggers Materialize: opens the Extension's Side View, carrying the payload needed to open it.
    OpenSideView {
        payload: serde_json::Value,
    },
    /// Opens the settings window (MOE-0029): the platform shows it and dismisses the panel — the
    /// settings surface is a window of its own, never a panel page (ADR-0009 amendment).
    OpenSettings {},
    Silent,
}

impl ActionResult {
    /// List view: list on the left + detail on the right (multiple results, e.g. history search, action list).
    pub fn list(items: Vec<Item>) -> Self {
        Self::List {
            items,
            detail_full: false,
            empty: None,
            filter: None,
        }
    }

    /// List view that declares what to show when it has no rows (ADR-0043). Prefer this over a
    /// one-item list whose single item is a notice: that item is focusable, so ⏎ can Apply it.
    pub fn list_with_empty(items: Vec<Item>, empty: EmptyView) -> Self {
        Self::List {
            items,
            detail_full: false,
            empty: Some(empty),
            filter: None,
        }
    }

    /// Declare this list's filter dimension (ADR-0043). Chainable:
    /// `ActionResult::list(items).with_filter(Filter::derived("tag", "Tags"))`. A no-op on a
    /// non-list result, so an Extension can apply it unconditionally.
    pub fn with_filter(mut self, filter: Filter) -> Self {
        if let Self::List { filter: slot, .. } = &mut self {
            *slot = Some(filter);
        }
        self
    }

    /// Detail view: a single result that is itself the content (AI answers, system notifications); the detail fills the panel.
    pub fn detail(items: Vec<Item>) -> Self {
        Self::List {
            items,
            detail_full: true,
            empty: None,
            filter: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandMeta {
    /// Globally unique, by convention `"{extension_id}.{command}"`.
    pub id: String,
    pub extension_id: String,
    /// The command name **without** the extension prefix (e.g. "Open Config File", not "Moe: Open Config File"):
    /// rows show the extension name as secondary text instead (ADR-0030).
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
    /// Invocation shortcut, when the command has one (a platform keymap display string, e.g. "⌘,");
    /// rows render it as a Kbd after the extension name (ADR-0030). None = no shortcut.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keybinding: Option<String>,
    /// The owning extension's display name, filled by the Registry so mixed sections
    /// (Favorites / Suggestions) can label every row (ADR-0030).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extension_title: Option<String>,
    /// The kind of thing this row represents — the trailing badge on root rows (ADR-0030).
    /// Filled by the Registry from `Extension::command_kind`; None renders as "Command".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Input Bar arguments this Command declares (ADR-0045), in display order; empty = none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub arguments: Vec<CommandArgument>,
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
// The lint suggests boxing `Item`; a streaming event is rebuilt per model delta, so the allocation
// would ride every chunk for a few dozen bytes of size difference. Accepted here by design.
#[allow(clippy::large_enum_variant)]
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

/// One option of a Command argument (ADR-0045): the bounded set of values the panel offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArgumentOption {
    /// Stable id: what the Extension sees in `ArgumentValues` on invoke.
    pub id: String,
    /// The option's name, on the argument chip and in the picker.
    pub title: String,
}

/// One argument a Command declares (ADR-0045): a positional control in the Input Bar, before the
/// query text. The panel renders the chip and the picker and hands the chosen value back on invoke;
/// the Extension only declares the choice list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandArgument {
    /// Stable id within the command: the key in `ArgumentValues`.
    pub id: String,
    /// The argument's name ("Language"): the picker's section header.
    pub title: String,
    /// The option id selected before the user picks (absent = the first option).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    /// The choice list, in display order. An empty list is a declaration error: the panel shows no
    /// chip for it (a control with nothing to choose would be a control that lies).
    pub options: Vec<ArgumentOption>,
}

/// The argument values the panel collected for one invocation (ADR-0045), keyed by argument id.
/// Empty = a command that declares none, or a caller that passed none — the Extension applies its
/// own default then, so a direct invocation (a re-run, another surface) never depends on the panel.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ArgumentValues(BTreeMap<String, String>);

impl ArgumentValues {
    /// The value of an argument; None when the caller sent none for it.
    pub fn get(&self, id: &str) -> Option<&str> {
        self.0.get(id).map(String::as_str)
    }

    pub fn insert(&mut self, id: impl Into<String>, value: impl Into<String>) {
        self.0.insert(id.into(), value.into());
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

pub trait Extension: Send + Sync {
    /// Namespace key (ADR-0003): storage and history are isolated beneath it.
    fn id(&self) -> &str;
    fn title(&self) -> &str;
    fn commands(&self) -> Vec<CommandMeta>;

    /// Apply a Command in the command palette. `arguments` carries the values the panel collected
    /// for the Command's declared arguments (ADR-0045); empty for a command that declares none.
    fn invoke(
        &self,
        command_id: &str,
        query: Option<&str>,
        selection: Option<&Selection>,
        arguments: &ArgumentValues,
    ) -> Result<ActionResult, MoeError>;

    /// Execution entry with incremental events; falls back to [`Extension::invoke`] by default (non-streaming extensions need not implement it).
    fn invoke_streaming(
        &self,
        command_id: &str,
        query: Option<&str>,
        selection: Option<&Selection>,
        arguments: &ArgumentValues,
        _emitter: Arc<dyn Emitter>,
    ) -> Result<ActionResult, MoeError> {
        self.invoke(command_id, query, selection, arguments)
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

    /// The kind of thing this Extension's commands are (ADR-0030): the trailing badge on root
    /// rows. None = the platform's default ("Command"); the AI extensions declare "AI Command",
    /// a future file source could declare "File".
    fn command_kind(&self) -> Option<String> {
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

    /// Continue (or start, with an empty id) a conversation hosted in the Command Panel
    /// (`ActionResult::Conversation`, ADR-0036): returns the conversation id; replies stream via
    /// `CommandEvent` carrying the page's command id. `selection` is the panel's captured context
    /// (the selected text becomes question context when the conversation starts, ADR-0019 amendment).
    fn panel_continue(
        &self,
        _conversation_id: &str,
        _message: &str,
        _selection: Option<&Selection>,
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
                group: None,
                actions: vec![],
                payload: serde_json::Value::Null,
                detail: None,
                pending: false,
                icon: None,
                tags: Vec::new(),
                accessories: Vec::new(),
                metadata: Vec::new(),
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

    /// The refine loop (ADR-0024 amendment): the platform serializes it to the UI and intercepts it.
    #[test]
    fn rerun_result_serializes() {
        let json = serde_json::to_value(ActionResult::Rerun {
            text: "the result so far".into(),
        })
        .unwrap();
        assert_eq!(json["rerun"]["text"], "the result so far");
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
            group: None,
            icon: None,
            actions: vec![],
            payload: serde_json::Value::Null,
            detail: Some("Answer body".into()),
            pending: false,
            tags: Vec::new(),
            accessories: Vec::new(),
            metadata: Vec::new(),
        }
    }

    /// `Item.group` rides the wire for the list–detail template (ADR-0018 amendment): camelCase,
    /// omitted while None so ungrouped lists keep the old payload shape.
    #[test]
    fn item_group_serializes_only_when_set() {
        let mut item = answer();
        let json = serde_json::to_value(&item).unwrap();
        assert!(json.get("group").is_none(), "None is omitted");
        item.group = Some("Today".into());
        let json = serde_json::to_value(&item).unwrap();
        assert_eq!(json["group"], "Today");
    }

    /// `Filter.values` is omitted while empty so a derived dimension keeps the payload minimal, and the
    /// UI must read absent as empty. A required TS field over this omittable one once threw at runtime
    /// (`undefined.length`), so the omission is pinned here.
    #[test]
    fn filter_values_serializes_only_when_set() {
        let derived = serde_json::to_value(Filter::derived("tags", "Tags")).unwrap();
        assert!(derived.get("values").is_none(), "empty values are omitted");

        let declared = Filter {
            values: vec!["Agentic".into()],
            ..Filter::derived("tags", "Tags")
        };
        let json = serde_json::to_value(&declared).unwrap();
        assert_eq!(json["values"][0], "Agentic");
    }

    /// An accessory rides the wire internally tagged by `kind`, camelCase, omitting its optionals
    /// (ADR-0041). The UI switches on `kind`; an unset color/tooltip must arrive absent, not null,
    /// because the mirror declares them optional.
    #[test]
    fn accessory_serializes_tagged_and_omits_optionals() {
        let result = ActionResult::list(vec![Item {
            accessories: vec![
                Accessory::tag("Agentic", Some("magenta".into())),
                Accessory::date(100),
            ],
            ..answer()
        }]);
        let json = serde_json::to_value(&result).unwrap();
        let accessories = &json["list"]["items"][0]["accessories"];
        assert_eq!(accessories[0]["kind"], "tag");
        assert_eq!(accessories[0]["value"], "Agentic");
        assert_eq!(accessories[0]["color"], "magenta");
        assert!(
            accessories[0].get("tooltip").is_none(),
            "unset tooltip is omitted"
        );
        assert_eq!(accessories[1]["kind"], "date");
        assert_eq!(accessories[1]["value"], 100);
        // Empty accessories stay off the wire entirely, like tags and group.
        let plain = serde_json::to_value(answer()).unwrap();
        assert!(plain.get("accessories").is_none());
    }

    /// Metadata rides the wire as an ordered, internally tagged list (ADR-0042) and stays off it when
    /// empty. The tag list is carried whole even though only MOE-0048 renders it.
    #[test]
    fn metadata_serializes_tagged_and_omits_optionals() {
        let result = ActionResult::list(vec![Item {
            metadata: vec![
                Metadata::Label {
                    title: "Model".into(),
                    icon: None,
                    text: Some("claude-sonnet-4-5".into()),
                    color: Some("blue".into()),
                },
                Metadata::Link {
                    title: "Docs".into(),
                    text: "example.com".into(),
                    target: "https://example.com".into(),
                },
                Metadata::TagList {
                    title: "Capabilities".into(),
                    items: vec![MetadataTag {
                        text: Some("tools".into()),
                        icon: None,
                        color: Some("green".into()),
                        action_id: None,
                    }],
                },
                Metadata::Separator,
            ],
            ..answer()
        }]);
        let json = serde_json::to_value(&result).unwrap();
        let metadata = &json["list"]["items"][0]["metadata"];
        assert_eq!(metadata[0]["kind"], "label");
        assert_eq!(metadata[0]["text"], "claude-sonnet-4-5");
        assert_eq!(metadata[0]["color"], "blue");
        assert!(metadata[0].get("icon").is_none(), "unset icon is omitted");
        assert_eq!(metadata[1]["kind"], "link");
        assert_eq!(metadata[1]["target"], "https://example.com");
        assert_eq!(metadata[2]["kind"], "tagList");
        assert_eq!(metadata[2]["items"][0]["text"], "tools");
        assert!(
            metadata[2]["items"][0].get("actionId").is_none(),
            "unset actionId is omitted"
        );
        assert_eq!(metadata[3]["kind"], "separator");
        // Empty metadata stays off the wire entirely, like tags, group and accessories.
        let plain = serde_json::to_value(answer()).unwrap();
        assert!(plain.get("metadata").is_none());
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

    /// The panel conversation page (ADR-0036): the UI reads `conversation.conversationId` (null = blank new chat).
    #[test]
    fn conversation_result_serializes_camel_case() {
        let blank = serde_json::to_value(ActionResult::Conversation {
            conversation_id: None,
        })
        .unwrap();
        assert!(blank["conversation"]["conversationId"].is_null());

        let open = serde_json::to_value(ActionResult::Conversation {
            conversation_id: Some("7".into()),
        })
        .unwrap();
        assert_eq!(open["conversation"]["conversationId"], "7");
    }
}
