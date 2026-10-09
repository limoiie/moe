//! AI Q&A: real streaming answers from an OpenAI-compatible endpoint (ADR-0005).
//!
//! - Quick Ask: the Command Panel hosts the conversation (ADR-0036) — `ActionResult::Conversation`
//!   opens the page, `Extension::panel_continue` sends and continues it, answers are persisted (IIE4AD-360).
//! - Side view continuation: the whole history is the context; events use the `ai.side` command_id convention.
//! - Streaming goes through `CommandEvent::ItemUpdated`: invoke immediately returns the page, a worker
//!   thread reads SSE and emits chunk by chunk (rendered as the answer bubble's stream).

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use moe_core::contract::{
    Action, ActionKind, ActionResult, CommandEvent, CommandMeta, Emitter, Extension, InputKind,
    Item, MoeError, NoopEmitter, Selection,
};
use moe_core::conversation::{AttachmentRef, Conversation, Message, Role};
use moe_core::keymap::{SystemKey, display_of};
use moe_platform::config::MoeConfig;
use moe_platform::db::Db;
use moe_platform::keychain;

use crate::ai_client::{SseLine, chat_body_from_messages, chat_completions_url, sse_delta};
use crate::attachment;
use crate::recency;

pub struct AiShell;

/// Event command_id convention for side view continuation (the chat window filters by `item.payload.conversationId`).
pub const SIDE_COMMAND_ID: &str = "ai.side";

/// Max entries in the history list.
const HISTORY_LIMIT: usize = 20;

/// Conversation title length (truncated with an ellipsis when exceeded).
const TITLE_CHARS: usize = 40;

fn answer_item(
    detail: &str,
    reasoning: &str,
    conversation_id: Option<&str>,
    pending: bool,
) -> Item {
    Item {
        id: "ai.answer".into(),
        title: "AI Answer".into(),
        subtitle: None,
        group: None,
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
                keybinding: Some("⌘J".into()),
            },
        ],
        payload: conversation_payload(conversation_id, reasoning),
        detail: Some(detail.into()),
        pending,
    }
}

/// Item payload: the conversation id (passed back as-is on Apply/⌘J — the UI does not interpret it)
/// plus the model's thinking, which the UI renders as the bubble's collapsible Thinking block
/// (stripped from the body, MOE-0008).
fn conversation_payload(conversation_id: Option<&str>, reasoning: &str) -> serde_json::Value {
    serde_json::json!({
        "conversationId": conversation_id,
        "reasoning": reasoning,
    })
}

/// History/continuation list item: Apply opens that conversation in the side view; `preview` is a summary of the last answer (IIE4AD-370).
fn history_item(conversation: Conversation, now_unix: u64, preview: Option<String>) -> Item {
    let payload = conversation_payload(Some(&conversation.id), "");
    Item {
        id: format!("ai.conversation.{}", conversation.id),
        title: conversation.title,
        subtitle: Some(relative_time(conversation.updated_unix, now_unix)),
        // Recency buckets shared with the Side View's history card (ADR-0018 amendment).
        group: Some(recency::group_label(conversation.updated_unix, now_unix).to_string()),
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
        group: None,
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

/// Frame builder: answer + reasoning + pending → Item (chat streams and AI command streams each have
/// their own card shape; the answer is the body, the reasoning rides the payload for the UI).
pub(crate) type FrameBuilder = Arc<dyn Fn(&str, &str, bool) -> Item + Send + Sync>;
/// Stream finish hook: the final answer → () (persist / completion callback).
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

/// Reasoning wrapper names (case-insensitive): a thinking-flavoured tag opens a block anywhere; a
/// `response` tag only opens one at the very start of the message (elsewhere it is content).
const OPEN_NAMES: [&str; 3] = ["thinking", "think", "reasoning"];
const CLOSE_NAMES: [&str; 4] = ["thinking", "think", "reasoning", "response"];
const START_OPEN_NAMES: [&str; 1] = ["response"];
/// A lone closing token (dropped opener) is only trusted for the unambiguous names: a stray
/// `</response>` is ordinary XML-ish content, not a wrapper.
const LONE_CLOSE_NAMES: [&str; 3] = ["thinking", "think", "reasoning"];
/// Older MiniMax/DeepSeek style: the **same** delimiter (an ordinary ellipsis too) on both ends.
const SAME_DELIM: &str = "…";

/// Case-insensitive name check against a name set.
fn is_named(names: &[&str], name: &str) -> bool {
    names
        .iter()
        .any(|candidate| name.eq_ignore_ascii_case(candidate))
}

/// One tag-like token at or after `from`: `<` [`/`] name [whitespace] `>`, names ASCII letters and no
/// attributes — wrappers are static template strings, and prose containing `<word ...>` must not
/// match. Returns (start, end, is_closing, name).
fn tag_token(text: &str, from: usize) -> Option<(usize, usize, bool, &str)> {
    let bytes = text.as_bytes();
    let mut at = from;
    while let Some(rel) = text[at..].find('<') {
        let start = at + rel;
        let mut cursor = start + 1;
        let closing = bytes.get(cursor) == Some(&b'/');
        if closing {
            cursor += 1;
        }
        let name_start = cursor;
        while bytes
            .get(cursor)
            .is_some_and(|byte| byte.is_ascii_alphabetic())
        {
            cursor += 1;
        }
        if cursor == name_start {
            at = start + 1;
            continue;
        }
        let name = &text[name_start..cursor];
        let mut end = cursor;
        while bytes
            .get(end)
            .is_some_and(|byte| byte.is_ascii_whitespace())
        {
            end += 1;
        }
        if bytes.get(end) != Some(&b'>') {
            at = start + 1;
            continue;
        }
        return Some((start, end + 1, closing, name));
    }
    None
}

/// The next reasoning closing token at or after the start of `text`: (start, end).
fn next_closing_tag(text: &str) -> Option<(usize, usize)> {
    let mut from = 0;
    while let Some((start, end, closing, name)) = tag_token(text, from) {
        if closing && is_named(&CLOSE_NAMES, name) {
            return Some((start, end));
        }
        from = end;
    }
    None
}

/// Append one reasoning piece; blocks are separated by a blank line, each trimmed.
fn push_reasoning(reasoning: &mut String, piece: &str) {
    let piece = piece.trim();
    if piece.is_empty() {
        return;
    }
    if !reasoning.is_empty() {
        reasoning.push_str("\n\n");
    }
    reasoning.push_str(piece);
}

/// Split on tag-like wrappers: the first thinking-flavoured token opens a block, the next closing
/// token ends it (any opener pairs with any closer — templates mix `<thinking>` and `</response>`);
/// a closing token without an opener means the opening tag was dropped. Unknown tokens stay answer
/// text, so ordinary `<word>` content is untouched. Works on partial text: an unterminated block
/// means "still thinking" (the answer stays empty).
fn split_tagged(text: &str) -> (String, String) {
    let mut reasoning = String::new();
    let mut answer = String::new();
    let mut rest = text;
    loop {
        let Some((start, end, closing, name)) = tag_token(rest, 0) else {
            answer.push_str(rest);
            break;
        };
        let opens = !closing
            && (is_named(&OPEN_NAMES, name) || (start == 0 && is_named(&START_OPEN_NAMES, name)));
        if opens {
            let after = &rest[end..];
            match next_closing_tag(after) {
                // Unterminated block: still thinking (the answer must stay empty)
                None => {
                    answer.push_str(&rest[..start]);
                    push_reasoning(&mut reasoning, after);
                    break;
                }
                Some((close, close_end)) => {
                    answer.push_str(&rest[..start]);
                    push_reasoning(&mut reasoning, &after[..close]);
                    rest = &after[close_end..];
                }
            }
        } else if closing && is_named(&LONE_CLOSE_NAMES, name) {
            // A lone closing tag: the opening tag was dropped — everything before it was reasoning
            push_reasoning(&mut reasoning, &rest[..start]);
            rest = &rest[end..];
        } else {
            // Not a reasoning wrapper: keep the token as answer text and keep scanning
            answer.push_str(&rest[..end]);
            rest = &rest[end..];
        }
    }
    (reasoning, answer)
}

/// Diagnostic (stderr): a tag-like token left in the result names a wrapper spelling the matcher
/// does not know yet — log it escaped (the exact bytes) once per completion.
fn log_unconsumed_tag(reasoning: &str, answer: &str) {
    for part in [answer, reasoning] {
        if let Some((start, end, _, name)) = tag_token(part, 0) {
            let token = &part[start..end];
            eprintln!(
                "moe: unrecognized tag-like token in the streamed result: {token:?} (name {name:?}) — extend OPEN_NAMES/CLOSE_NAMES if this wraps reasoning"
            );
            return;
        }
    }
}

/// Split an accumulated completion into (reasoning, answer): the answer is what streams, persists, is
/// copied and written back; the reasoning only rides the frame payload to the UI's Thinking block.
pub(crate) fn split_reasoning(raw: &str) -> (String, String) {
    let trimmed = raw.trim_start();

    // Older style: `…` … `…`. The delimiter doubles as an ordinary ellipsis, so it only counts at the
    // very start; each block's closer is the next delimiter.
    if let Some(body) = trimmed.strip_prefix(SAME_DELIM) {
        let mut reasoning = String::new();
        let mut tail = body;
        let answer = loop {
            let Some((block, after)) = tail.split_once(SAME_DELIM) else {
                // Unterminated block: still thinking (the answer must stay empty)
                push_reasoning(&mut reasoning, tail);
                break String::new();
            };
            push_reasoning(&mut reasoning, block);
            // Another block may open right after the closer ("…one……two…answer")
            match after.strip_prefix(SAME_DELIM) {
                Some(next) => tail = next,
                None => break after.to_string(),
            }
        };
        return (reasoning, answer.trim().to_string());
    }

    let (reasoning, answer) = split_tagged(trimmed);
    (reasoning.trim().to_string(), answer.trim().to_string())
}

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
    let emit = |text: String, reasoning: String, pending: bool| {
        emitter.emit(CommandEvent::ItemUpdated {
            command_id: command_id.clone(),
            item: item_of(&text, &reasoning, pending),
        });
    };
    let fail = |text: String| {
        persist(&text);
        emit(text, String::new(), false);
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

    let mut raw = String::new();
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
                raw.push_str(&delta);
                // The model's chain of thought is never the answer: it rides the payload, not the body
                let (reasoning, answer) = split_reasoning(&raw);
                emit(answer, reasoning, true);
            }
            SseLine::Done => break,
            SseLine::Ignore => {}
        }
    }
    end_stream(&stream_key);
    let (reasoning, answer) = split_reasoning(&raw);
    log_unconsumed_tag(&reasoning, &answer);

    if stopped {
        let text = if answer.trim().is_empty() {
            "(generation stopped)".to_string()
        } else {
            format!("{answer}\n\n_(generation stopped)_")
        };
        persist(&text);
        emit(text, reasoning, false);
    } else if answer.trim().is_empty() {
        fail("(the endpoint returned no content)".into());
    } else {
        persist(&answer);
        emit(answer.clone(), reasoning, false);
        on_done(&answer);
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

/// Request body for a panel/side continuation: the stored history ends with the new question
/// (attachments expanded, ADR-0010).
fn chat_body_of(model: &str, messages: Vec<Message>) -> serde_json::Value {
    chat_body_from_messages(
        model,
        messages.iter().map(attachment::expand_message).collect(),
    )
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

    /// One panel message → the model body (clean) + attachment references: `@path` mentions (ADR-0010)
    /// merge with the files selected before opening (ADR-0021), deduplicated by path.
    /// Attachments-only input gets a generic request sentence — the raw mention is never sent or
    /// stored in history (IIE4AD-391); a message with neither text nor attachments is rejected.
    fn panel_message(
        message: &str,
        selection: Option<&Selection>,
    ) -> Result<(String, Vec<AttachmentRef>), MoeError> {
        let selected_files: Vec<String> = selection.map(|s| s.files.clone()).unwrap_or_default();
        let (cleaned, paths) = attachment::parse_mentions(message);
        let attachments = Self::attachments_with(&paths, &selected_files);
        let display = if cleaned.trim().is_empty() {
            if attachments.is_empty() {
                return Err(MoeError::Internal("Message is empty".into()));
            }
            "Please answer with these attachments.".to_string()
        } else {
            cleaned
        };
        Ok((display, attachments))
    }

    /// One ask on the panel's Quick Ask page (ADR-0036): an empty conversation id starts a new
    /// conversation (the selected text becomes question context), a non-empty id continues it; the
    /// whole history is the model context either way. Returns the conversation id; the answer streams
    /// as `ai.quick-ask` events.
    fn panel_ask(
        &self,
        conversation_id: &str,
        message: &str,
        selection: Option<&Selection>,
        emitter: Arc<dyn Emitter>,
    ) -> Result<String, MoeError> {
        let starting = conversation_id.trim().is_empty();
        // The captured context (selected text + Finder files) belongs to the conversation's first
        // message only (ADR-0019/0021 amendments); follow-up messages must not repeat it.
        let selection = if starting { selection } else { None };
        let (display, attachments) = Self::panel_message(message, selection)?;

        let config = MoeConfig::load();
        if !config.ai.configured() {
            return Err(MoeError::Internal(
                "AI endpoint not configured yet: run Open Config File (⌘,) and fill in [ai] base_url and model".into(),
            ));
        }
        let Some(key) = keychain::ai_api_key() else {
            return Err(MoeError::Internal(
                "API key missing: type `key <your-key>` in the panel and press Enter".into(),
            ));
        };

        let db = Db::open_default().map_err(MoeError::Internal)?;
        let conversation_id = if starting {
            db.create_conversation("ai", &title_for(&display, &attachments), SystemTime::now())
                .map_err(MoeError::Internal)?
        } else {
            conversation_id.trim().to_string()
        };

        // Fetch history first, then append this one: the endpoint receives messages ending with the new
        // message. History stores the clean question; the selected text only enters the request body.
        let mut messages = db.messages(&conversation_id).map_err(MoeError::Internal)?;
        db.append_message(
            &conversation_id,
            Role::User,
            &display,
            &attachments,
            SystemTime::now(),
        )
        .map_err(MoeError::Internal)?;
        let content = question_with_selection(&display, selection.and_then(Selection::text));
        messages.push(Message {
            role: Role::User,
            content,
            attachments,
        });

        let base_url = config.ai.base_url.clone().unwrap_or_default();
        let model = config.ai.model_or_default().to_string();
        let body = chat_body_of(&model, messages);
        let conversation = conversation_id.clone();
        let stream_key = conversation.clone();
        let item_of: FrameBuilder = {
            let conversation = conversation.clone();
            Arc::new(move |text, reasoning, pending| {
                answer_item(text, reasoning, Some(&conversation), pending)
            })
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
                    command_id: "ai.quick-ask".into(),
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

    /// Quick Ask entry (ADR-0036): a query means the row captured the Input Bar text — the question is
    /// asked and the result hands the platform the conversation page to open (None emitter = the reply
    /// is only persisted, not delivered anywhere). No query = the page itself was applied: open it blank.
    fn quick_ask(
        &self,
        query: Option<&str>,
        selection: Option<&Selection>,
        emitter: Option<Arc<dyn Emitter>>,
    ) -> Result<ActionResult, MoeError> {
        let Some(question) = query else {
            return Ok(ActionResult::Conversation {
                conversation_id: None,
            });
        };
        let conversation_id = self.panel_ask(
            "",
            question,
            selection,
            emitter.unwrap_or_else(|| Arc::new(NoopEmitter)),
        )?;
        Ok(ActionResult::Conversation {
            conversation_id: Some(conversation_id),
        })
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
                subtitle: Some(
                    "Chat in the panel: Enter sends, answers stream; ⌘N new chat · ⌘P history"
                        .into(),
                ),
                icon: Some("sparkles".into()),
                // Apply opens the page (ADR-0036); the capture row (fallback) declares Query and
                // delivers the input text as the question. Its invocation shortcut is the platform's
                // ⌘/ (display from the keymap table, ADR-0030).
                input: InputKind::None,
                live: false,
                keybinding: display_of(SystemKey::QuickAsk).map(str::to_string),
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
                id: "ai.side-chat".into(),
                extension_id: "ai".into(),
                title: "Open Side Chat".into(),
                subtitle: Some("Open the AI chat in the side window directly".into()),
                icon: Some("panel-right".into()),
                input: InputKind::None,
                live: false,
                keybinding: display_of(SystemKey::OpenSideChat).map(str::to_string),
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
            "ai.quick-ask" => self.quick_ask(query, selection, None),
            "ai.search-history" => Ok(self.search_history(query.unwrap_or_default())),
            // Open the Side View directly (the tray's "AI Chat" path): the platform hides the panel
            // and shows the chat window; an empty payload = a blank conversation.
            "ai.side-chat" => Ok(ActionResult::OpenSideView {
                payload: serde_json::json!({}),
            }),
            _ => Err(MoeError::NotFound),
        }
    }

    /// Generic Browse action (⌘P, ADR-0014): AI's record list = chat history.
    fn browse_command(&self) -> Option<CommandMeta> {
        self.commands()
            .into_iter()
            .find(|c| c.id == "ai.search-history")
    }

    /// Generic New action (⌘N, ADR-0014): AI's "new" is the blank Quick Ask page. New Chat was
    /// the same action and folded into Quick Ask (ADR-0036 amendment), so the entry resolves to
    /// the Quick Ask command's meta.
    fn new_command(&self) -> Option<CommandMeta> {
        self.commands().into_iter().find(|c| c.id == "ai.quick-ask")
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
            "ai.quick-ask" => self.quick_ask(query, selection, Some(emitter)),
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
            (false, false) => "Enter to ask in the panel (⌥⏎ copy · ⌘J side view)".to_string(),
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
        let body = chat_body_of(&model, messages);
        let conversation = conversation_id.clone();
        let stream_key = conversation.clone();
        let item_of: FrameBuilder = {
            let conversation = conversation.clone();
            Arc::new(move |text, reasoning, pending| {
                answer_item(text, reasoning, Some(&conversation), pending)
            })
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

    /// Panel continuation (ADR-0036): the Quick Ask page's composer; the whole history is the model
    /// context, replies stream as `ai.quick-ask` events.
    fn panel_continue(
        &self,
        conversation_id: &str,
        message: &str,
        selection: Option<&Selection>,
        emitter: Arc<dyn Emitter>,
    ) -> Result<String, MoeError> {
        self.panel_ask(conversation_id, message, selection, emitter)
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

    /// Thinking blocks (MOE-0008): the reasoning never reaches the answer, whatever the tag shape.
    #[test]
    fn split_reasoning_separates_thought_from_answer() {
        assert_eq!(split_reasoning(""), (String::new(), String::new()));
        // No tags: everything is the answer
        assert_eq!(
            split_reasoning("just an answer"),
            (String::new(), "just an answer".into())
        );
        // MiniMax/DeepSeek style: the same delimiter on both ends
        assert_eq!(
            split_reasoning("\u{2026}weighing options\u{2026}Improved text"),
            ("weighing options".into(), "Improved text".into())
        );
        // The same delimiter only counts at the very start: an ellipsis in the prose is text
        assert_eq!(
            split_reasoning("just an answer\u{2026}maybe"),
            (String::new(), "just an answer\u{2026}maybe".into())
        );
        // The XML-ish pair
        assert_eq!(
            split_reasoning("<thinking>hmm</thinking>done"),
            ("hmm".into(), "done".into())
        );
        // A lone closing tag (servers that drop the opener): everything before it is reasoning
        assert_eq!(
            split_reasoning("thought so far</thinking>answer"),
            ("thought so far".into(), "answer".into())
        );
        // Unterminated (still streaming): the reasoning grows, the answer stays empty
        assert_eq!(
            split_reasoning("\u{2026}still thinking"),
            ("still thinking".into(), String::new())
        );
        // Multiple same-delimiter blocks join; the answer is what follows the last closer
        assert_eq!(
            split_reasoning("\u{2026}one\u{2026}\u{2026}two\u{2026}the answer"),
            ("one\n\ntwo".into(), "the answer".into())
        );
    }

    /// The stream the user reported (MOE-0010): the canonical tags around the model's analysis — the
    /// body must be only the final answer line, and neither part may carry tag bytes.
    #[test]
    fn split_reasoning_strips_the_reported_stream() {
        let raw = r#"<thinking>
The user wants me to fix spelling, grammar, and punctuation errors while keeping the meaning and style unchanged.

Original: "this si a test cast, do you known?"

Let me identify the errors:
1. "this" should be capitalized at the start of a sentence
2. "si" is a typo for "is"

Fixed: "This is a test case, do you know?"
</thinking>
This is a test case, do you know?"#;
        let (reasoning, answer) = split_reasoning(raw);
        assert!(reasoning.starts_with("The user wants me to fix spelling"));
        assert!(reasoning.contains("Fixed: \"This is a test case, do you know?\""));
        assert_eq!(answer, "This is a test case, do you know?");
        assert!(!reasoning.contains('<') && !answer.contains('<'));
    }

    /// Wrapper spellings the exact matcher missed (MOE-0011): internal whitespace, different case, a
    /// differently named closer, and a `response`-opened block — while XML-ish content stays content.
    #[test]
    fn split_reasoning_matches_tolerant_tag_spellings() {
        // Whitespace before the closing bracket
        assert_eq!(
            split_reasoning("<thinking >hmm</thinking >done"),
            ("hmm".into(), "done".into())
        );
        // Case-insensitive names
        assert_eq!(
            split_reasoning("<Thinking>hmm</Thinking>done"),
            ("hmm".into(), "done".into())
        );
        // Templates may open with `<thinking>` and close with `</response>`
        assert_eq!(
            split_reasoning("<thinking>hmm</response>done"),
            ("hmm".into(), "done".into())
        );
        // …or open with `<response>` (trusted at the very start only)
        assert_eq!(
            split_reasoning("<response>hmm</response>done"),
            ("hmm".into(), "done".into())
        );
        // Mid-answer `<response>` pairs are ordinary XML-ish content
        assert_eq!(
            split_reasoning("the api returns <response>ok</response> in json"),
            (
                String::new(),
                "the api returns <response>ok</response> in json".into()
            )
        );
        // Unknown tags stay answer text
        assert_eq!(
            split_reasoning("bold <b>text</b> here"),
            (String::new(), "bold <b>text</b> here".into())
        );
    }

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

    /// File-only selection + empty question: the captured files are the content — the message becomes
    /// the generic request sentence (the Side View's rule, IIE4AD-391).
    #[test]
    fn files_only_ask_uses_the_generic_sentence() {
        let selection = Selection {
            text: None,
            files: vec!["/tmp/a.md".into()],
        };
        let (display, attachments) = AiShell::panel_message("   ", Some(&selection)).unwrap();
        assert_eq!(display, "Please answer with these attachments.");
        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].path, "/tmp/a.md");
    }

    /// A bare mention (only @path, no body) never sends the raw path to the model: the display becomes
    /// the generic request sentence while the path rides along as an attachment (IIE4AD-391).
    #[test]
    fn mention_only_ask_uses_the_generic_sentence() {
        let (display, attachments) = AiShell::panel_message("@/tmp/a.md", None).unwrap();
        assert_eq!(display, "Please answer with these attachments.");
        assert!(
            !display.contains("/tmp/a.md"),
            "The raw path must not be the question: {display}"
        );
        assert_eq!(attachments.len(), 1);
    }

    /// Neither text nor attachments: the page refuses to send (the UI never gets here with an empty draft).
    #[test]
    fn empty_panel_message_is_rejected() {
        assert!(matches!(
            AiShell::panel_message("  ", None),
            Err(MoeError::Internal(_))
        ));
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
                payload: serde_json::json!({ "conversationId": "7", "reasoning": "" })
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
            let item_of: FrameBuilder = Arc::new(move |text, reasoning, pending| {
                answer_item(text, reasoning, Some(conversation), pending)
            });
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

    /// The answer item's ⌘J (materialize) also carries the conversation id back, so the side view locates the same conversation.
    #[test]
    fn answer_item_carries_conversation_id_on_materialize() {
        let item = answer_item("body", "weighing it", Some("9"), false);
        assert_eq!(item.payload["conversationId"], "9");
        assert_eq!(item.payload["reasoning"], "weighing it");
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
                payload: serde_json::json!({ "conversationId": "9", "reasoning": "weighing it" })
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

    /// Apply on the Quick Ask row (no query): the platform opens the blank panel page — no request is
    /// fired and the search text is not a question (ADR-0036).
    #[test]
    fn quick_ask_without_a_query_opens_the_blank_page() {
        let result = AiShell.quick_ask(None, None, None).unwrap();
        assert_eq!(
            result,
            ActionResult::Conversation {
                conversation_id: None
            }
        );
        // The same shape through the command path (Apply from the command list)
        assert_eq!(AiShell.invoke("ai.quick-ask", None, None).unwrap(), result);
    }

    /// Panel continuation (ADR-0036): the composer path refuses empty messages before touching the
    /// model config (the UI never sends an empty draft).
    #[test]
    fn panel_continue_rejects_empty_messages() {
        assert!(matches!(
            AiShell.panel_continue("", "   ", None, Arc::new(NoopEmitter)),
            Err(MoeError::Internal(_))
        ));
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
        assert_eq!(
            ext.new_command().unwrap().id,
            "ai.quick-ask",
            "New Chat folded into Quick Ask (ADR-0036 amendment)"
        );
    }

    /// Request body assembly for the panel page: history + the new question (with selection context) both go into the body.
    #[test]
    fn panel_body_carries_history_and_selection_context() {
        let history = vec![Message {
            role: Role::User,
            content: question_with_selection("translate", Some("hello world")),
            attachments: vec![],
        }];
        let text = chat_body_of("m", history).to_string();
        assert!(
            text.contains("translate"),
            "question is in the body: {text}"
        );
        assert!(
            text.contains("hello world"),
            "selection is in the body: {text}"
        );
        // No selection → no context wrapper
        let plain = chat_body_of(
            "m",
            vec![Message {
                role: Role::User,
                content: "translate".into(),
                attachments: vec![],
            }],
        )
        .to_string();
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

    /// New (⌘N gives the Quick Ask entry, ADR-0014): the blank panel conversation page. New Chat
    /// was the same action and is gone — the entry resolves to `ai.quick-ask` (ADR-0036 amendment).
    #[test]
    fn the_new_entry_opens_the_blank_panel_page() {
        let new = AiShell.new_command().expect("AI declares its New entry");
        assert_eq!(new.id, "ai.quick-ask", "New resolves to Quick Ask");
        let result = AiShell
            .invoke(&new.id, None, None)
            .expect("ai.quick-ask is routable");
        assert_eq!(
            result,
            ActionResult::Conversation {
                conversation_id: None
            }
        );
        let json = serde_json::to_value(&result).unwrap();
        assert!(json["conversation"]["conversationId"].is_null());
    }

    /// Quick Ask declares its ⌘/ shortcut from the platform keymap (ADR-0030), so the row's Kbd
    /// can never drift from the binding the panel implements.
    #[test]
    fn quick_ask_declares_its_launch_shortcut() {
        let quick = AiShell
            .commands()
            .into_iter()
            .find(|c| c.id == "ai.quick-ask")
            .expect("quick-ask registered");
        assert_eq!(
            quick.keybinding.as_deref(),
            display_of(SystemKey::QuickAsk),
            "the Kbd comes from the keymap table"
        );
    }

    /// Open Side Chat (⌘⇧/, ADR-0036 amendment): the command opens the Side View directly with an
    /// empty payload — a blank conversation, the tray's "AI Chat" path, no panel page entered.
    #[test]
    fn side_chat_opens_the_side_view_directly() {
        let side = AiShell
            .commands()
            .into_iter()
            .find(|c| c.id == "ai.side-chat")
            .expect("side-chat registered");
        assert_eq!(
            side.keybinding.as_deref(),
            display_of(SystemKey::OpenSideChat),
            "the Kbd comes from the keymap table"
        );
        assert_eq!(
            AiShell.invoke("ai.side-chat", None, None).unwrap(),
            ActionResult::OpenSideView {
                payload: serde_json::json!({})
            }
        );
    }
}
