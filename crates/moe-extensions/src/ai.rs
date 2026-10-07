//! AI Q&A: real streaming answers from an OpenAI-compatible endpoint (ADR-0005).
//!
//! - Quick ask: a question creates a conversation (Namespace `ai`), answers are persisted (IIE4AD-360).
//! - Side view continuation: the whole history is the context; events use the `ai.side` command_id convention.
//! - Streaming goes through `CommandEvent::ItemUpdated`: invoke immediately returns a placeholder item,
//!   a worker thread reads SSE and emits chunk by chunk (re-rendered in place in the detail card/side view).

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use moe_core::contract::{
    Action, ActionKind, ActionResult, CommandEvent, CommandMeta, Emitter, Extension, InputKind,
    Item, MoeError, Selection,
};
use moe_core::conversation::{AttachmentRef, Conversation, Message, Role};
use moe_platform::config::MoeConfig;
use moe_platform::db::Db;
use moe_platform::keychain;

use crate::ai_client::{SseLine, chat_body_from_messages, chat_completions_url, sse_delta};
use crate::attachment;

pub struct AiShell;

/// Event command_id convention for side view continuation (the chat window filters by `item.payload.conversationId`).
pub const SIDE_COMMAND_ID: &str = "ai.side";

/// Max entries in the history list.
const HISTORY_LIMIT: usize = 20;

/// Conversation title length (truncated with an ellipsis when exceeded).
const TITLE_CHARS: usize = 40;

fn answer_item(detail: &str, conversation_id: Option<&str>, pending: bool) -> Item {
    Item {
        id: "ai.answer".into(),
        title: "AI Answer".into(),
        subtitle: None,
        icon: Some("sparkles".into()),
        actions: vec![
            Action {
                id: "write-back".into(),
                title: "Write Back".into(),
                kind: ActionKind::Primary,
                keybinding: None,
            },
            Action {
                id: "copy".into(),
                title: "Copy".into(),
                kind: ActionKind::Secondary,
                keybinding: Some("⌥⏎".into()),
            },
            Action {
                id: "materialize".into(),
                title: "Open in Side View".into(),
                kind: ActionKind::Secondary,
                keybinding: Some("⌘M".into()),
            },
        ],
        payload: conversation_payload(conversation_id),
        detail: Some(detail.into()),
        pending,
    }
}

/// Item payload carries the conversation id, passed back as-is on Apply/⌘M (the UI does not interpret it).
fn conversation_payload(conversation_id: Option<&str>) -> serde_json::Value {
    match conversation_id {
        Some(id) => serde_json::json!({ "conversationId": id }),
        None => serde_json::Value::Null,
    }
}

/// History/continuation list item: Apply opens that conversation in the side view; `preview` is a summary of the last answer (IIE4AD-370).
fn history_item(conversation: Conversation, now_unix: u64, preview: Option<String>) -> Item {
    let payload = conversation_payload(Some(&conversation.id));
    Item {
        id: format!("ai.conversation.{}", conversation.id),
        title: conversation.title,
        subtitle: Some(relative_time(conversation.updated_unix, now_unix)),
        icon: Some("message-square".into()),
        actions: vec![Action {
            id: "materialize".into(),
            title: "Open in Side View".into(),
            kind: ActionKind::Primary,
            keybinding: Some("⏎".into()),
        }],
        payload,
        detail: preview,
        pending: false,
    }
}

/// History preview summary: collapses extra whitespace, truncates by character (without breaking multibyte sequences).
fn preview_excerpt(text: &str) -> String {
    const LIMIT: usize = 240;
    let cleaned: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.chars().count() <= LIMIT {
        return cleaned;
    }
    let mut out: String = cleaned.chars().take(LIMIT).collect();
    out.push('…');
    out
}

/// Notice item without actions (empty history, read failure, etc.).
fn notice_item(title: &str, detail: &str) -> Item {
    Item {
        id: "ai.notice".into(),
        title: title.into(),
        subtitle: None,
        icon: Some("info".into()),
        actions: vec![],
        payload: serde_json::Value::Null,
        detail: Some(detail.into()),
        pending: false,
    }
}

/// Conversation title: first line of the question, truncated when too long.
fn conversation_title(question: &str) -> String {
    let first_line = question.lines().next().unwrap_or_default().trim();
    let mut title: String = first_line.chars().take(TITLE_CHARS).collect();
    if first_line.chars().count() > TITLE_CHARS {
        title.push('…');
    }
    title
}

/// Conversation title: first line of the question, truncated when too long; attachments-only (no text) falls back to the attachment name.
fn title_for(display: &str, attachments: &[AttachmentRef]) -> String {
    let title = conversation_title(display);
    if !title.is_empty() {
        return title;
    }
    match attachments.first() {
        Some(first) => format!("Attachment: {}", first.name),
        None => "New Chat".into(),
    }
}

/// Relative time for list subtitles.
fn relative_time(updated_unix: u64, now_unix: u64) -> String {
    let secs = now_unix.saturating_sub(updated_unix);
    match secs {
        0..=59 => "just now".into(),
        60..=3599 => format!("{}m ago", secs / 60),
        3600..=86399 => format!("{}h ago", secs / 3600),
        _ => format!("{}d ago", secs / 86400),
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Asking creates the conversation (history isolated by the `ai` Namespace, ADR-0003;
/// attachments stored as references only, ADR-0010);
/// degrades to no persistence when storage is unavailable.
fn persist_question(display: &str, attachments: &[AttachmentRef]) -> Option<String> {
    let db = Db::open_default().ok()?;
    let now = SystemTime::now();
    let id = db
        .create_conversation("ai", &title_for(display, attachments), now)
        .ok()?;
    db.append_message(&id, Role::User, display, attachments, now)
        .ok()?;
    Some(id)
}

/// Persists the answer; failures are silent (panel interaction unaffected).
fn persist_assistant(conversation_id: Option<&str>, text: &str) {
    let Some(conversation_id) = conversation_id else {
        return;
    };
    if let Ok(db) = Db::open_default() {
        let _ = db.append_message(
            conversation_id,
            Role::Assistant,
            text,
            &[],
            SystemTime::now(),
        );
    }
}

/// In-progress generation: registry key → cancellation flag (IIE4AD-365).
/// For chat streams the key is the conversation id; for AI command streams it is a unique key per run (ADR-0024).
fn active_streams() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    static ACTIVE: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();
    ACTIVE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Registers one generation (an empty key is not registered and cannot be interrupted; returns None).
fn begin_stream(key: &str) -> Option<Arc<AtomicBool>> {
    if key.is_empty() {
        return None;
    }
    let flag = Arc::new(AtomicBool::new(false));
    active_streams()
        .lock()
        .expect("streams poisoned")
        .insert(key.to_string(), Arc::clone(&flag));
    Some(flag)
}

/// Cleanup: removes from the registry (called when the worker finishes or is aborted).
fn end_stream(key: &str) {
    if key.is_empty() {
        return;
    }
    active_streams()
        .lock()
        .expect("streams poisoned")
        .remove(key);
}

/// Marks all in-progress generations as cancelled; workers finish at the next SSE line. Returns the number aborted.
pub(crate) fn stop_all_streams() -> usize {
    let streams = active_streams().lock().expect("streams poisoned");
    for flag in streams.values() {
        flag.store(true, Ordering::Relaxed);
    }
    streams.len()
}

/// Frame builder: full text + pending → Item (chat streams and AI command streams each have their own card shape).
pub(crate) type FrameBuilder = Arc<dyn Fn(&str, bool) -> Item + Send + Sync>;
/// Stream finish hook: full text → () (persist / auto write-back callback).
pub(crate) type StreamFinish = Arc<dyn Fn(&str) + Send + Sync>;

/// Full specification of one streaming request (shared by chat streams and AI command streams, ADR-0024).
pub(crate) struct StreamRequest {
    pub base_url: String,
    pub key: String,
    pub body: serde_json::Value,
    pub command_id: String,
    pub stream_key: String,
    pub item_of: FrameBuilder,
    pub persist: StreamFinish,
    pub on_done: StreamFinish,
}

/// No-op finish hook (streams with no persistence and no completion callback).
pub(crate) fn noop_sink(_text: &str) {}

/// Request + SSE loop: emits each incremental chunk (pending=true); on natural completion it persists and calls `on_done`.
/// When stopped (IIE4AD-365) it keeps the generated part, persists it, marks it stopped, and does **not** trigger `on_done`.
pub(crate) fn run_stream(request: StreamRequest, emitter: Arc<dyn Emitter>) {
    let StreamRequest {
        base_url,
        key,
        body,
        command_id,
        stream_key,
        item_of,
        persist,
        on_done,
    } = request;
    let cancel = begin_stream(&stream_key);
    let emit = |text: String, pending: bool| {
        emitter.emit(CommandEvent::ItemUpdated {
            command_id: command_id.clone(),
            item: item_of(&text, pending),
        });
    };
    let fail = |text: String| {
        persist(&text);
        emit(text, false);
    };

    let client = match reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(300))
        .build()
    {
        Ok(client) => client,
        Err(err) => {
            fail(format!(
                "## Request failed\n\nFailed to initialize HTTP client: {err}"
            ));
            end_stream(&stream_key);
            return;
        }
    };
    let response = client
        .post(chat_completions_url(&base_url))
        .bearer_auth(&key)
        .json(&body)
        .send();
    let response = match response {
        Ok(response) => response,
        Err(err) => {
            fail(format!("## Request failed\n\n```\n{err}\n```"));
            end_stream(&stream_key);
            return;
        }
    };
    if !response.status().is_success() {
        let status = response.status();
        let body: String = response
            .text()
            .unwrap_or_default()
            .chars()
            .take(500)
            .collect();
        fail(format!("## Endpoint returned {status}\n\n```\n{body}\n```"));
        end_stream(&stream_key);
        return;
    }

    let mut acc = String::new();
    let mut stopped = false;
    for line in BufReader::new(response).lines() {
        if cancel
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
        {
            stopped = true;
            break;
        }
        let Ok(line) = line else { break };
        match sse_delta(&line) {
            SseLine::Delta(delta) => {
                acc.push_str(&delta);
                emit(acc.clone(), true);
            }
            SseLine::Done => break,
            SseLine::Ignore => {}
        }
    }
    end_stream(&stream_key);

    if stopped {
        let text = if acc.is_empty() {
            "(generation stopped)".to_string()
        } else {
            format!("{acc}\n\n_(generation stopped)_")
        };
        persist(&text);
        emit(text, false);
    } else if acc.is_empty() {
        fail("(the endpoint returned no content)".into());
    } else {
        persist(&acc);
        emit(acc.clone(), false);
        on_done(&acc);
    }
}

/// Guidance card for a not-yet-configured endpoint (shared by AI Q&A and AI Commands, ADR-0024).
pub(crate) const SETUP_MD: &str = "## No AI endpoint configured yet\n\n\
1. Run the command \"Open Config File\" and fill in:\n\n\
```toml\n\
[ai]\n\
base_url = \"https://api.deepseek.com/v1\"\n\
model = \"deepseek-chat\"\n\
```\n\n\
2. Store an API key: type `key <your-key>` and press Enter (stored only in a local key file, never echoed; you can also set the `MOE_AI_API_KEY` environment variable).\n\n\
Works with any OpenAI-compatible endpoint (DeepSeek / OpenRouter / local llama.cpp, etc.).";

/// Guidance card for a missing API key (shared as above).
pub(crate) const KEY_MISSING_MD: &str = "## Endpoint configured, but the API key is missing\n\n\
Type `key <your-key>` and press Enter to store it in a local key file (0600, never echoed); you can also set the `MOE_AI_API_KEY` environment variable.";

/// Length limit (in characters) for selected text used as context.
const SELECTION_LIMIT: usize = 4000;

/// Appends the selected text to the question (ADR-0019 amendment: the Selection captured before
/// the panel opens automatically becomes question context).
/// Skipped when the question already contains that text; overlong selections are truncated with a note.
fn question_with_selection(question: &str, selection: Option<&str>) -> String {
    let Some(selection) = selection.map(str::trim).filter(|s| !s.is_empty()) else {
        return question.to_string();
    };
    if question.contains(selection) {
        return question.to_string();
    }
    let truncated = selection.chars().count() > SELECTION_LIMIT;
    let excerpt: String = selection.chars().take(SELECTION_LIMIT).collect();
    let suffix = if truncated {
        "\n…(selected text too long, truncated)"
    } else {
        ""
    };
    format!(
        "{question}\n\nThe following text is the user's selection, provided as context for the answer:\n```\n{excerpt}{suffix}\n```"
    )
}

/// Request body sent to the model (extracted for testing): question (with optional selection context) + expanded attachments.
fn ask_body(
    model: &str,
    question: &str,
    selection: Option<&str>,
    attachments: &[AttachmentRef],
) -> serde_json::Value {
    let message = Message {
        role: Role::User,
        content: question_with_selection(question, selection),
        attachments: attachments.to_vec(),
    };
    chat_body_from_messages(model, vec![attachment::expand_message(&message)])
}

impl AiShell {
    /// Attachments = `@path` mentions in the input + files selected before opening (ADR-0021), deduplicated by path.
    fn attachments_with(
        mention_paths: &[PathBuf],
        selected_files: &[String],
    ) -> Vec<AttachmentRef> {
        let mut seen = std::collections::HashSet::new();
        let mut refs = Vec::new();
        let all = mention_paths
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .chain(selected_files.iter().cloned());
        for path in all {
            if seen.insert(path.clone()) {
                refs.push(attachment::ref_for_path(Path::new(&path)));
            }
        }
        refs
    }

    /// Splits an input into body (clean) + attachment references.
    /// Attachments-only input (only `@path`, no body) gets a generic request sentence —
    /// the raw mention is never sent to the model or stored in history (IIE4AD-391).
    fn split_message(message: &str) -> (String, Vec<AttachmentRef>) {
        let (cleaned, paths) = attachment::parse_mentions(message);
        let attachments: Vec<AttachmentRef> = paths
            .iter()
            .map(|path| attachment::ref_for_path(path))
            .collect();
        let display = if cleaned.is_empty() {
            if attachments.is_empty() {
                message.to_string()
            } else {
                "Please answer with these attachments.".to_string()
            }
        } else {
            cleaned
        };
        (display, attachments)
    }

    fn start_ask(
        &self,
        question: &str,
        selection: Option<&Selection>,
        emitter: Option<Arc<dyn Emitter>>,
    ) -> ActionResult {
        let question = question.trim().to_string();
        // Attachments are expressed as `@path` mentions (ADR-0010): content is read on demand,
        // references are persisted with the message.
        // Files selected in Finder before opening the panel also become attachments (ADR-0021),
        // deduplicated against mentions by path.
        let (cleaned, paths) = attachment::parse_mentions(&question);
        let selected_files: Vec<String> = selection.map(|s| s.files.clone()).unwrap_or_default();
        let attachments = Self::attachments_with(&paths, &selected_files);
        let text = selection.and_then(|s| s.text());
        // Attachments-only input (only @path) does not treat the raw mention as a question: it falls through to the empty-question guidance (consistent with selected files).
        let display = if cleaned.is_empty() {
            if paths.is_empty() {
                question.clone()
            } else {
                String::new()
            }
        } else {
            cleaned
        };
        if display.trim().is_empty() {
            let hint = if attachments.is_empty() {
                "Type your question in the input box, then press Enter."
            } else {
                "Type your question in the input box, then press Enter (attachments included)."
            };
            return ActionResult::detail(vec![answer_item(hint, None, false)]);
        }

        let config = MoeConfig::load();
        if !config.ai.configured() {
            return ActionResult::detail(vec![answer_item(SETUP_MD, None, false)]);
        }
        let Some(key) = keychain::ai_api_key() else {
            return ActionResult::detail(vec![answer_item(KEY_MISSING_MD, None, false)]);
        };

        // Asking creates the conversation; ⌘M then takes the same conversation into the side view.
        // History stores only the question itself (clean title); what goes to the model includes the selected-text context.
        let conversation_id = persist_question(&display, &attachments);

        let base_url = config.ai.base_url.clone().unwrap_or_default();
        let model = config.ai.model_or_default().to_string();

        if let Some(emitter) = emitter {
            let body = ask_body(&model, &display, text, &attachments);
            let conversation_id_in_thread = conversation_id.clone();
            let stream_key = conversation_id_in_thread.clone().unwrap_or_default();
            let item_of: FrameBuilder = {
                let conversation_id = conversation_id_in_thread.clone();
                Arc::new(move |text, pending| {
                    answer_item(text, conversation_id.as_deref(), pending)
                })
            };
            let persist: StreamFinish = {
                let conversation_id = conversation_id_in_thread.clone();
                Arc::new(move |text| persist_assistant(conversation_id.as_deref(), text))
            };
            std::thread::spawn(move || {
                run_stream(
                    StreamRequest {
                        base_url,
                        key,
                        body,
                        command_id: "ai.quick-ask".into(),
                        stream_key,
                        item_of,
                        persist,
                        on_done: Arc::new(noop_sink),
                    },
                    emitter,
                );
            });
        }
        // Placeholder frame: body left empty — the "generating" inline indicator is already the only status feedback
        ActionResult::detail(vec![answer_item("", conversation_id.as_deref(), true)])
    }

    /// History search: the list re-runs as you type (CommandMeta.live), fuzzy title matching, most recent first.
    fn search_history(&self, query: &str) -> ActionResult {
        match Db::open_default() {
            Ok(db) => Self::search_history_in(&db, query),
            Err(err) => ActionResult::list(vec![notice_item("Failed to read history", &err)]),
        }
    }

    /// DB-injected version: tests use a temp DB to assert shape and entries without touching
    /// the real data directory.
    ///
    /// The shape is always a list (ADR-0013): even a single hit does not take the whole screen,
    /// which is what gives the UI its "left list + right preview".
    fn search_history_in(db: &Db, query: &str) -> ActionResult {
        let now = unix_now();
        let items = match db.conversations("ai", Some(query), HISTORY_LIMIT) {
            Ok(list) if list.is_empty() => vec![notice_item(
                "No matching chat history",
                "Type a question directly into the palette to start a new conversation.",
            )],
            Ok(list) => list
                .into_iter()
                .map(|conversation| {
                    // Preview = summary of the last answer (goes into the focused preview card, IIE4AD-370)
                    let preview = db
                        .last_assistant_message(&conversation.id)
                        .ok()
                        .flatten()
                        .map(|text| preview_excerpt(&text));
                    history_item(conversation, now, preview)
                })
                .collect(),
            Err(err) => vec![notice_item("Failed to read history", &err)],
        };
        ActionResult::list(items)
    }

    /// Delete injected version (ADR-0022): locates via the payload's conversationId; tests do not touch the real data directory.
    fn delete_item_in(db: &Db, item: &Item) -> Result<usize, MoeError> {
        let id = item
            .payload
            .get("conversationId")
            .and_then(|v| v.as_str())
            .ok_or(MoeError::NotFound)?;
        db.delete_conversation(id).map_err(MoeError::Internal)
    }
}

impl Extension for AiShell {
    fn id(&self) -> &str {
        // Namespace key (ADR-0003/0005: conversation history lives in this Namespace).
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
                title: "Quick Ask".into(),
                subtitle: Some("Or just type a question (an ask item appears automatically when nothing matches)".into()),
                icon: Some("sparkles".into()),
                input: InputKind::Query,
                live: false,
            keybinding: None,
            extension_title: None,
            kind: None,
            },
            CommandMeta {
                id: "ai.search-history".into(),
                extension_id: "ai".into(),
                title: "Search Chat History".into(),
                subtitle: Some("Type to filter titles; Enter opens in the side view".into()),
                icon: Some("history".into()),
                input: InputKind::Query,
                live: true,
            keybinding: None,
            extension_title: None,
            kind: None,
            },
            CommandMeta {
                id: "ai.new-chat".into(),
                extension_id: "ai".into(),
                title: "New Chat".into(),
                subtitle: Some("Clear the current Q&A and start a fresh conversation".into()),
                icon: Some("plus".into()),
                input: InputKind::None,
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
        selection: Option<&Selection>,
    ) -> Result<ActionResult, MoeError> {
        match command_id {
            "ai.quick-ask" => Ok(self.start_ask(query.unwrap_or_default(), selection, None)),
            "ai.search-history" => Ok(self.search_history(query.unwrap_or_default())),
            // Generic New action (⌘N, ADR-0014): the panel holds no conversation state
            // (one question = one conversation); here we show an empty-state card —
            // the next question naturally starts a new conversation.
            "ai.new-chat" => Ok(ActionResult::detail(vec![notice_item(
                "New Chat",
                "Type a question into the input bar to start; ⌘M moves this conversation into the side view.",
            )])),
            _ => Err(MoeError::NotFound),
        }
    }

    /// Generic Browse action (⌘P, ADR-0014): AI's record list = chat history.
    fn browse_command(&self) -> Option<CommandMeta> {
        self.commands()
            .into_iter()
            .find(|c| c.id == "ai.search-history")
    }

    /// Generic New action (⌘N, ADR-0014): AI's "new" = new conversation.
    fn new_command(&self) -> Option<CommandMeta> {
        self.commands().into_iter().find(|c| c.id == "ai.new-chat")
    }

    /// The trailing badge on root rows (ADR-0030): AI's commands are AI Commands.
    fn command_kind(&self) -> Option<String> {
        Some("AI Command".into())
    }

    fn invoke_streaming(
        &self,
        command_id: &str,
        query: Option<&str>,
        selection: Option<&Selection>,
        emitter: Arc<dyn Emitter>,
    ) -> Result<ActionResult, MoeError> {
        match command_id {
            "ai.quick-ask" => {
                Ok(self.start_ask(query.unwrap_or_default(), selection, Some(emitter)))
            }
            _ => self.invoke(command_id, query, None),
        }
    }

    /// The "capturing" command shown when nothing matches (e.g. AI: Ask "…");
    /// the subtitle notes the context: selected text / selected files (Finder) each get their own hint.
    fn fallback_command(&self, query: &str, selection: Option<&Selection>) -> Option<CommandMeta> {
        // The title strips mentions (ADR-0010): paths are not shown as question content.
        let (cleaned, paths) = attachment::parse_mentions(query);
        let selected_files = selection.map(|s| s.files.len()).unwrap_or(0);
        let title = match (cleaned.is_empty(), paths.is_empty(), selected_files) {
            (false, _, _) => format!("Ask \"{cleaned}\""),
            (true, false, _) | (true, true, 1..) => "Ask with attachments".into(),
            (true, true, _) => format!("Ask \"{query}\""),
        };
        let total_files = paths.len() + selected_files;
        let has_text = selection
            .and_then(|s| s.text())
            .map(str::trim)
            .is_some_and(|t| !t.is_empty());
        let subtitle = match (total_files > 0, has_text) {
            (true, true) => {
                format!("{total_files} attachment(s) and selected text attached; Enter to send")
            }
            (true, false) => format!("{total_files} attachment(s); Enter to send"),
            (false, true) => "Selected text attached as context; Enter to send".to_string(),
            (false, false) => {
                "Enter to send; answers can be written back (⌥⏎ copy · ⌘M side view)".to_string()
            }
        };
        Some(CommandMeta {
            id: "ai.quick-ask".into(),
            extension_id: "ai".into(),
            title,
            subtitle: Some(subtitle),
            icon: Some("sparkles".into()),
            input: InputKind::Query,
            live: false,
            keybinding: None,
            extension_title: None,
            kind: None,
        })
    }

    fn run_item_action(
        &self,
        _command_id: &str,
        item: &Item,
        action: &Action,
    ) -> Result<ActionResult, MoeError> {
        match action.id.as_str() {
            // Write back the full text (the title is fixed, the body is in detail)
            "write-back" => Ok(ActionResult::WriteBack {
                text: item.detail.clone().unwrap_or_else(|| item.title.clone()),
            }),
            // ⌥⏎ copy: writes to the system clipboard (a platform capability), does not write back into the host app
            "copy" => {
                let text = item.detail.clone().unwrap_or_else(|| item.title.clone());
                moe_platform::clipboard::copy(&text)
                    .map_err(|err| MoeError::Internal(err.to_string()))?;
                Ok(ActionResult::Silent)
            }
            // Open the side view: the payload goes to the platform's chat window as-is.
            "materialize" => Ok(ActionResult::OpenSideView {
                payload: item.payload.clone(),
            }),
            _ => Err(MoeError::NotFound),
        }
    }

    /// Side view continuation (IIE4AD-360): an empty id creates a new conversation; the whole history is the context, replies are streamed and persisted in the background.
    fn side_continue(
        &self,
        conversation_id: &str,
        message: &str,
        emitter: Arc<dyn Emitter>,
    ) -> Result<String, MoeError> {
        let message = message.trim();
        if message.is_empty() {
            return Err(MoeError::Internal("Message is empty".into()));
        }
        let config = MoeConfig::load();
        if !config.ai.configured() {
            return Err(MoeError::Internal(
                "AI endpoint not configured yet (run \"Open Config File\" in the palette)".into(),
            ));
        }
        let Some(key) = keychain::ai_api_key() else {
            return Err(MoeError::Internal(
                "API key missing (type key <your-key> in the palette and press Enter)".into(),
            ));
        };

        let db = Db::open_default().map_err(MoeError::Internal)?;

        // Attachment mentions use the same syntax as quick ask (ADR-0010); attachments-only input gets a generic request sentence (IIE4AD-391).
        let (display, attachments) = Self::split_message(message);

        let conversation_id = if conversation_id.trim().is_empty() {
            db.create_conversation("ai", &title_for(&display, &attachments), SystemTime::now())
                .map_err(MoeError::Internal)?
        } else {
            conversation_id.trim().to_string()
        };

        // Fetch history first, then append this one: the endpoint receives messages ending with the new message (attachments are read and expanded one by one).
        let mut messages = db.messages(&conversation_id).map_err(MoeError::Internal)?;
        db.append_message(
            &conversation_id,
            Role::User,
            &display,
            &attachments,
            SystemTime::now(),
        )
        .map_err(MoeError::Internal)?;
        messages.push(Message {
            role: Role::User,
            content: display,
            attachments,
        });

        let base_url = config.ai.base_url.clone().unwrap_or_default();
        let model = config.ai.model_or_default().to_string();
        let body = chat_body_from_messages(
            &model,
            messages.iter().map(attachment::expand_message).collect(),
        );
        let conversation = conversation_id.clone();
        let stream_key = conversation.clone();
        let item_of: FrameBuilder = {
            let conversation = conversation.clone();
            Arc::new(move |text, pending| answer_item(text, Some(&conversation), pending))
        };
        let persist: StreamFinish = {
            let conversation = conversation.clone();
            Arc::new(move |text| persist_assistant(Some(&conversation), text))
        };
        std::thread::spawn(move || {
            run_stream(
                StreamRequest {
                    base_url,
                    key,
                    body,
                    command_id: SIDE_COMMAND_ID.into(),
                    stream_key,
                    item_of,
                    persist,
                    on_done: Arc::new(noop_sink),
                },
                emitter,
            );
        });
        Ok(conversation_id)
    }

    /// Stops in-progress generation (IIE4AD-365): sets the cancellation flag; the worker finishes at the next SSE line and persists the generated part.
    fn stop_generation(&self) -> usize {
        stop_all_streams()
    }

    /// Deletes the current record (generic Delete action, ⌃X, ADR-0022): removes one conversation from AI history.
    fn delete_item(&self, command_id: &str, item: &Item) -> Result<usize, MoeError> {
        if command_id != "ai.search-history" {
            return Err(MoeError::NotFound);
        }
        let db = Db::open_default().map_err(MoeError::Internal)?;
        Self::delete_item_in(&db, item)
    }

    /// Deletes all records (generic DeleteAll action, ⌃⇧X, ADR-0022): clears every conversation in the `ai` Namespace.
    fn delete_all(&self, command_id: &str) -> Result<usize, MoeError> {
        if command_id != "ai.search-history" {
            return Err(MoeError::NotFound);
        }
        Db::open_default()
            .map_err(MoeError::Internal)?
            .delete_all_conversations("ai")
            .map_err(MoeError::Internal)
    }
}

/// `stop_all_streams` acts on the **global** registry: two stream tests running in parallel would
/// cancel each other's streams (one test's stop also marks the cancellation flag the other just
/// registered), causing flaky failures. Serialized.
/// pub(crate): the AI commands (ai_commands.rs) stream tests share the same lock.
#[cfg(test)]
pub(crate) static STREAM_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;
    use moe_core::contract::Extension;
    use std::time::Instant;

    /// Same guard: the id synthesized by fallback must be routable; the title strips attachment mentions (IIE4AD-358).
    #[test]
    fn fallback_command_is_invocable_and_strips_mentions() {
        let ext = AiShell;
        let fallback = ext.fallback_command("hello", None).expect("fallback");
        assert!(
            ext.commands().iter().any(|c| c.id == fallback.id),
            "fallback id must be routable within commands(): {}",
            fallback.id
        );

        let with_attachments = ext
            .fallback_command("Summarize @\"/tmp/a b.md\"", None)
            .expect("fallback");
        assert_eq!(with_attachments.title, "Ask \"Summarize\"");
        assert_eq!(
            with_attachments.subtitle.as_deref(),
            Some("1 attachment(s); Enter to send")
        );

        let attachment_only = ext.fallback_command("@/tmp/a.md", None).expect("fallback");
        assert_eq!(attachment_only.title, "Ask with attachments");
    }

    /// Selected files (ADR-0021): the fallback advertises the file count; file-only selection and text selection can combine.
    #[test]
    fn fallback_advertises_selected_files() {
        let ext = AiShell;
        let files = Selection {
            text: None,
            files: vec!["/tmp/a.md".into(), "/tmp/b.png".into()],
        };
        // Empty input + selected files: the ask-with-attachments entry
        let files_only = ext.fallback_command("", Some(&files)).expect("fallback");
        assert_eq!(files_only.title, "Ask with attachments");
        assert_eq!(
            files_only.subtitle.as_deref(),
            Some("2 attachment(s); Enter to send")
        );
        // Question + selected files: attachment and selected-text hints combine
        let both = ext
            .fallback_command(
                "Summarize",
                Some(&Selection {
                    text: Some("selected text".into()),
                    files: files.files.clone(),
                }),
            )
            .expect("fallback");
        assert_eq!(both.title, "Ask \"Summarize\"");
        assert_eq!(
            both.subtitle.as_deref(),
            Some("2 attachment(s) and selected text attached; Enter to send")
        );
    }

    /// Attachment merging (ADR-0021): mentions and selected files are deduplicated by path, order preserved.
    #[test]
    fn attachments_merge_mentions_and_selected_files_dedup() {
        let refs = AiShell::attachments_with(
            &[PathBuf::from("/tmp/a.md"), PathBuf::from("/tmp/dup.md")],
            &["/tmp/dup.md".into(), "/tmp/c.png".into()],
        );
        let paths: Vec<&str> = refs.iter().map(|r| r.path.as_str()).collect();
        assert_eq!(paths, ["/tmp/a.md", "/tmp/dup.md", "/tmp/c.png"]);
        assert_eq!(refs[1].name, "dup.md");
    }

    /// File-only selection + empty question: no request is sent; prompt to type a question (the files are already in hand).
    #[test]
    fn ask_with_only_files_guides_for_a_question() {
        let selection = Selection {
            text: None,
            files: vec!["/tmp/a.md".into()],
        };
        let ActionResult::List { items, .. } = AiShell.start_ask("   ", Some(&selection), None)
        else {
            panic!("expected list");
        };
        let detail = items[0].detail.as_deref().unwrap_or_default();
        assert!(detail.contains("attachments included"), "{detail}");
    }

    /// A bare mention (only @path, no body) does not send the raw path to the model: it falls through to the empty-question guidance (IIE4AD-391).
    #[test]
    fn mention_only_ask_guides_instead_of_sending_raw_path() {
        let ActionResult::List { items, .. } = AiShell.start_ask("@/tmp/a.md", None, None) else {
            panic!("expected list");
        };
        let detail = items[0].detail.as_deref().unwrap_or_default();
        assert!(detail.contains("attachments included"), "{detail}");
        assert!(
            !detail.contains("/tmp/a.md"),
            "The raw path should not appear in the hint: {detail}"
        );
    }

    /// split_message: keeps the body when present; attachments-only input gets a generic request sentence, never leaking the raw mention.
    #[test]
    fn split_message_swaps_raw_mention_for_prompt() {
        let (display, attachments) = AiShell::split_message("Summarize @\"/tmp/a b.md\"");
        assert_eq!(display, "Summarize");
        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].path, "/tmp/a b.md");

        let (display, attachments) = AiShell::split_message("@/tmp/a.md @/tmp/b.png");
        assert_eq!(display, "Please answer with these attachments.");
        assert_eq!(attachments.len(), 2);

        // No attachments and no body: unchanged (empty messages are blocked upstream)
        let (display, attachments) = AiShell::split_message("   ");
        assert_eq!(display, "   ");
        assert!(attachments.is_empty());
    }

    /// History search is a Live command (the list re-runs as the input changes).
    #[test]
    fn history_search_is_live_and_routable() {
        let ext = AiShell;
        let history = ext
            .commands()
            .into_iter()
            .find(|c| c.id == "ai.search-history")
            .expect("search-history registered");
        assert!(
            history.live,
            "history search must re-run as the input changes"
        );
    }

    #[test]
    fn conversation_title_takes_first_line_and_truncates() {
        assert_eq!(conversation_title("explain closures"), "explain closures");
        assert_eq!(conversation_title("first line\nsecond line"), "first line");
        let long = "x".repeat(50);
        let title = conversation_title(&long);
        assert_eq!(title.chars().count(), TITLE_CHARS + 1);
        assert!(title.ends_with('…'));
    }

    /// Attachments-only conversation: the title falls back to the attachment name (IIE4AD-358).
    #[test]
    fn title_falls_back_to_attachment_name() {
        let refs = vec![AttachmentRef {
            name: "shot.png".into(),
            path: "/tmp/shot.png".into(),
        }];
        assert_eq!(
            title_for("summarize the image", &refs),
            "summarize the image"
        );
        assert_eq!(title_for("", &refs), "Attachment: shot.png");
        assert_eq!(title_for("", &[]), "New Chat");
    }

    /// Preview excerpt: whitespace normalization + character-based truncation (IIE4AD-370).
    #[test]
    fn preview_excerpt_collapses_and_truncates() {
        assert_eq!(
            preview_excerpt("first line\n\nsecond line"),
            "first line second line"
        );
        let long = "x".repeat(300);
        let excerpt = preview_excerpt(&long);
        assert_eq!(excerpt.chars().count(), 241);
        assert!(excerpt.ends_with('…'));
    }

    #[test]
    fn relative_time_buckets() {
        assert_eq!(relative_time(100, 100), "just now");
        assert_eq!(relative_time(100, 159), "just now");
        assert_eq!(relative_time(100, 160), "1m ago");
        assert_eq!(relative_time(100, 100 + 3599), "59m ago");
        assert_eq!(relative_time(100, 100 + 3600), "1h ago");
        assert_eq!(relative_time(100, 100 + 86400), "1d ago");
        assert_eq!(relative_time(100, 100 + 3 * 86400), "3d ago");
    }

    /// Applying a history item (materialize) must hand the conversation id to the platform to open the side view.
    #[test]
    fn history_item_materializes_with_conversation_id() {
        let item = history_item(
            Conversation {
                id: "7".into(),
                namespace: "ai".into(),
                title: "explain closures".into(),
                updated_unix: 100,
            },
            160,
            Some("summary of the last answer".into()),
        );
        assert_eq!(item.subtitle.as_deref(), Some("1m ago"));
        assert_eq!(item.detail.as_deref(), Some("summary of the last answer"));
        let action = &item.actions[0];
        assert_eq!(action.kind, ActionKind::Primary);
        let result = AiShell
            .run_item_action("ai.search-history", &item, action)
            .unwrap();
        assert_eq!(
            result,
            ActionResult::OpenSideView {
                payload: serde_json::json!({ "conversationId": "7" })
            }
        );
    }

    /// Stop generation: requesting a stop after registration should hit the cancellation flag (IIE4AD-365).
    #[test]
    fn stop_all_streams_marks_active_flags() {
        let _guard = STREAM_TEST_LOCK.lock().expect("stream test lock");
        let flag = begin_stream("test-stop-conv").expect("register the cancellation flag");
        assert!(!flag.load(Ordering::Relaxed));
        assert!(
            stop_all_streams() >= 1,
            "should hit at least the flag just registered"
        );
        assert!(
            flag.load(Ordering::Relaxed),
            "the cancellation flag should be marked"
        );
        end_stream("test-stop-conv");
        // Empty keys are not registered and cannot be interrupted
        assert!(begin_stream("").is_none());
    }

    /// End to end: `stop_generation` mid-way through a slow SSE stream → finishes promptly, the last frame has pending=false and is marked stopped.
    #[test]
    fn stop_generation_stops_running_stream() {
        let _guard = STREAM_TEST_LOCK.lock().expect("stream test lock");
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::Mutex;

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let server = std::thread::spawn(move || {
            let Ok((mut socket, _)) = listener.accept() else {
                return;
            };
            let mut buf = [0u8; 2048];
            let _ = socket.read(&mut buf); // request headers
            let _ = socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\n");
            // Deliberately much longer than the test's waiting window (~50s): as long as it is
            // not stopped, the stream never finishes. Otherwise, if the test thread is descheduled
            // for a few seconds on CI, the stream might finish on its own and stop_all_streams() would hit 0.
            for index in 0..5_000 {
                let delta = format!(
                    "data: {{\"choices\":[{{\"delta\":{{\"content\":\"{index},\"}}}}]}}\n\n"
                );
                if socket.write_all(delta.as_bytes()).is_err() {
                    break; // client disconnected (stopped)
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        });

        struct Recorder(Mutex<Vec<Item>>);
        impl Emitter for Recorder {
            fn emit(&self, event: CommandEvent) {
                match event {
                    CommandEvent::ItemUpdated { item, .. } => self.0.lock().unwrap().push(item),
                    CommandEvent::WriteBack { .. } => {}
                }
            }
        }
        let recorder = Arc::new(Recorder(Mutex::new(Vec::new())));
        let emitter: Arc<dyn Emitter> = recorder.clone();

        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let base_url = format!("http://{addr}/v1");
        std::thread::spawn(move || {
            // Nonexistent conversation: persistence is a no-op (dangling-row protection at the db layer), so real history stays clean
            let conversation = "stop-e2e";
            let item_of: FrameBuilder =
                Arc::new(move |text, pending| answer_item(text, Some(conversation), pending));
            let persist: StreamFinish =
                Arc::new(move |text| persist_assistant(Some(conversation), text));
            run_stream(
                StreamRequest {
                    base_url,
                    key: "test-key".into(),
                    body: serde_json::json!({}),
                    command_id: "ai.quick-ask".into(),
                    stream_key: "stop-e2e".into(),
                    item_of,
                    persist,
                    on_done: Arc::new(noop_sink),
                },
                emitter,
            );
            let _ = done_tx.send(());
        });

        let deadline = Instant::now() + Duration::from_secs(5);
        while recorder.0.lock().unwrap().is_empty() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            !recorder.0.lock().unwrap().is_empty(),
            "should have received streaming increments first"
        );
        assert!(
            stop_all_streams() >= 1,
            "should hit the in-progress generation"
        );
        done_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("should finish promptly after the stop");

        let items = recorder.0.lock().unwrap();
        let last = items.last().expect("at least one frame");
        assert!(!last.pending, "the last frame should have pending=false");
        assert!(
            last.detail
                .as_deref()
                .unwrap_or_default()
                .contains("generation stopped"),
            "the last frame should be marked stopped: {:?}",
            last.detail
        );
        assert!(
            items.len() < 400,
            "should not have run the whole stream: {} frames",
            items.len()
        );
        let _ = server.join();
    }

    /// The answer item's ⌘M (materialize) also carries the conversation id back, so the side view locates the same conversation.
    #[test]
    fn answer_item_carries_conversation_id_on_materialize() {
        let item = answer_item("body", Some("9"), false);
        assert_eq!(item.payload["conversationId"], "9");
        let action = item
            .actions
            .iter()
            .find(|a| a.id == "materialize")
            .expect("materialize action");
        let result = AiShell
            .run_item_action("ai.quick-ask", &item, action)
            .unwrap();
        assert_eq!(
            result,
            ActionResult::OpenSideView {
                payload: serde_json::json!({ "conversationId": "9" })
            }
        );
    }

    /// Temp DB (never touching the real data directory): AI history is writable and queryable in tests.
    fn temp_db(tag: &str) -> (Db, std::path::PathBuf) {
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("moe-{tag}-{nanos}.db"));
        (Db::open(&path).expect("temp db"), path)
    }

    /// Delete slot (ADR-0022): single delete locates via the payload's conversationId; missing id → NotFound.
    #[test]
    fn delete_item_removes_the_conversation() {
        let (db, path) = temp_db("delete");
        let a = db
            .create_conversation("ai", "Conversation A", SystemTime::now())
            .unwrap();
        db.create_conversation("ai", "Conversation B", SystemTime::now())
            .unwrap();
        let item = history_item(
            Conversation {
                id: a.clone(),
                namespace: "ai".into(),
                title: "Conversation A".into(),
                updated_unix: 1,
            },
            1,
            None,
        );
        assert_eq!(AiShell::delete_item_in(&db, &item).unwrap(), 1);
        assert_eq!(db.conversations("ai", None, 10).unwrap().len(), 1);
        // Missing conversationId: NotFound, nothing deleted by mistake
        let bare = history_item(
            Conversation {
                id: "9".into(),
                namespace: "ai".into(),
                title: "x".into(),
                updated_unix: 1,
            },
            1,
            None,
        );
        let bare = Item {
            payload: serde_json::Value::Null,
            ..bare
        };
        assert!(matches!(
            AiShell::delete_item_in(&db, &bare),
            Err(MoeError::NotFound)
        ));
        // Unsupported command: NotFound (deletion is not exposed at the command layer)
        assert!(matches!(
            AiShell.delete_item("ai.quick-ask", &item),
            Err(MoeError::NotFound)
        ));
        let _ = std::fs::remove_file(&path);
    }

    /// Delete all (ADR-0022): only clears the `ai` Namespace, returns the count.
    #[test]
    fn delete_all_clears_ai_namespace() {
        let (db, path) = temp_db("delete-all");
        db.create_conversation("ai", "one", SystemTime::now())
            .unwrap();
        db.create_conversation("ai", "two", SystemTime::now())
            .unwrap();
        db.create_conversation("other", "another extension", SystemTime::now())
            .unwrap();

        assert_eq!(db.delete_all_conversations("ai").unwrap(), 2);
        assert_eq!(db.conversations("ai", None, 10).unwrap().len(), 0);
        assert_eq!(db.conversations("other", None, 10).unwrap().len(), 1);
        assert!(matches!(
            AiShell.delete_all("ai.quick-ask"),
            Err(MoeError::NotFound)
        ));
        let _ = std::fs::remove_file(&path);
    }

    /// The answer/guidance is a full-screen detail (ADR-0013): in the panel it is the body, not something to split with a list.
    #[test]
    fn ask_placeholder_declares_detail_layout() {
        let ActionResult::List { items, detail_full } = AiShell.start_ask("   ", None, None) else {
            panic!("expected list");
        };
        assert!(detail_full, "the answer is a full-screen detail");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "ai.answer");
    }

    /// History search always stays a list: even a single hit does not take the whole screen (the user wants to see that row) — ADR-0013.
    #[test]
    fn history_search_stays_list_view_even_with_single_hit() {
        let (db, path) = temp_db("history");
        let now = SystemTime::now();
        let id = db
            .create_conversation("ai", "explain closures", now)
            .expect("create");
        db.append_message(&id, Role::Assistant, "A closure is…", &[], now)
            .expect("append");

        let ActionResult::List { items, detail_full } = AiShell::search_history_in(&db, "closure")
        else {
            panic!("expected list");
        };
        assert!(
            !detail_full,
            "history search is a list view (left list + right preview)"
        );
        assert_eq!(
            items.len(),
            1,
            "even a single hit does not take the whole screen"
        );
        assert_eq!(items[0].id, format!("ai.conversation.{id}"));
        assert!(
            items[0].detail.is_some(),
            "right preview = summary of the last answer"
        );

        // No hit: still a list (a notice goes into the right preview, the list stays)
        let ActionResult::List { items, detail_full } =
            AiShell::search_history_in(&db, "does not exist")
        else {
            panic!("expected list");
        };
        assert!(!detail_full);
        assert_eq!(items[0].title, "No matching chat history");

        drop(db);
        let _ = std::fs::remove_file(&path);
    }

    /// Generic entry guard (ADR-0014): ids exposed by ⌘P/⌘N must be routable,
    /// otherwise the panel's invoke would hit NotFound (same guard as fallback).
    #[test]
    fn declared_entry_points_are_routable() {
        let ext = AiShell;
        let commands = ext.commands();
        for entry in [ext.browse_command(), ext.new_command()] {
            let entry = entry.expect("AI declares Browse and New");
            assert!(
                commands.iter().any(|c| c.id == entry.id),
                "entry id must be routable within commands(): {}",
                entry.id
            );
        }
        assert_eq!(ext.browse_command().unwrap().id, "ai.search-history");
        assert_eq!(ext.new_command().unwrap().id, "ai.new-chat");
    }

    /// Request body assembly: selection and question both go into the body (selection_becomes_question_context only tests the concatenation; this tests the whole chain).
    #[test]
    fn ask_body_carries_question_and_selection() {
        let body = ask_body("m", "translate", Some("hello world"), &[]);
        let text = body.to_string();
        assert!(
            text.contains("translate"),
            "question is in the body: {text}"
        );
        assert!(
            text.contains("hello world"),
            "selection is in the body: {text}"
        );
        // No selection → no context wrapper
        let plain = ask_body("m", "translate", None, &[]).to_string();
        assert!(
            !plain.contains("user's selection"),
            "no selection → no wrapper: {plain}"
        );
    }

    /// Selected text automatically becomes question context (ADR-0002 amendment): the concatenation format is stable, with boundaries for duplicates/blank/overlong.
    #[test]
    fn selection_becomes_question_context() {
        let prompt = question_with_selection("translate", Some("  hello world  "));
        assert!(
            prompt.starts_with("translate"),
            "question comes first: {prompt}"
        );
        assert!(
            prompt.contains("hello world"),
            "the selection should be concatenated into the prompt: {prompt}"
        );

        // The question already contains this text: do not concatenate again
        assert_eq!(
            question_with_selection("explain hello world", Some("hello world")),
            "explain hello world"
        );
        // Blank / no selection: returned unchanged
        assert_eq!(
            question_with_selection("translate", Some("   ")),
            "translate"
        );
        assert_eq!(question_with_selection("translate", None), "translate");

        // Overlong selection is truncated with a note; the context is not consumed indefinitely
        let long: String = "x".repeat(5_000);
        let capped = question_with_selection("translate", Some(&long));
        assert!(
            capped.contains("truncated"),
            "an overlong selection must note the truncation: {capped}"
        );
        // The excerpt is one unbroken run of `x` (4000 of them); wrapper/suffix prose
        // may contain isolated letters, so assert on the longest consecutive run.
        let x_run = capped
            .split(|c: char| c != 'x')
            .map(str::len)
            .max()
            .unwrap_or(0);
        assert!(
            x_run <= SELECTION_LIMIT,
            "after truncation it stays within the limit"
        );
    }

    /// New (⌘N) shows a full-screen detail empty-state card in the panel (conversation state untouched: one question = one conversation).
    #[test]
    fn new_chat_returns_a_detail_notice() {
        let ActionResult::List { items, detail_full } = AiShell
            .invoke("ai.new-chat", None, None)
            .expect("ai.new-chat is routable")
        else {
            panic!("expected list");
        };
        assert!(detail_full, "the empty-state card is a full-screen detail");
        assert_eq!(items[0].id, "ai.notice");
        assert!(
            items[0].actions.is_empty(),
            "the empty-state card has no executable actions"
        );
    }
}
