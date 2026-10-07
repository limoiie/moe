//! AI Commands (ADR-0024): batch text-transform commands — Improve Writing / Fix Spelling &
//! Grammar / Make Shorter / Make Longer / Simplify Language / Summarize / Translate / Tone /
//! Extract Key Ideas / Continue Writing, with Raycast Pro-style semantics.
//!
//! Input = the text selected before opening the palette, or the text in the input box when
//! there is no selection; when neither exists, a guidance card is shown.
//! Streaming output goes to a result card in the panel; **on natural completion the result is
//! automatically written back to the selection and the panel closes** (`CommandEvent::WriteBack`,
//! intercepted and executed by the platform layer). When stopped with Esc, only the generated
//! part is kept and nothing is written back automatically.
//! A single transform is never persisted and never creates a conversation (no history semantics).

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use moe_core::contract::{
    Action, ActionKind, ActionResult, CommandEvent, CommandMeta, Emitter, Extension, InputKind,
    Item, MoeError, Selection,
};
use moe_platform::config::MoeConfig;
use moe_platform::keychain;

use crate::ai::{self, KEY_MISSING_MD, SETUP_MD};
use crate::ai_client::{chat_body_with_stream, chat_completions_url};

pub struct AiCommands;

/// Spec of one AI command: id / presentation / system instruction.
struct Spec {
    id: &'static str,
    title: &'static str,
    subtitle: &'static str,
    icon: &'static str,
    instruction: &'static str,
}

const SPECS: [Spec; 12] = [
    Spec {
        id: "aicmd.improve",
        title: "Improve Writing",
        subtitle: "Clearer, smoother wording without changing the meaning",
        icon: "wand-2",
        instruction: "Improve the writing: make it clearer, smoother, and more professional, keeping the meaning and all information intact. Only output the transformed text.",
    },
    Spec {
        id: "aicmd.fix-grammar",
        title: "Fix Spelling & Grammar",
        subtitle: "Fix spelling and grammar errors",
        icon: "spell-check",
        instruction: "Fix spelling, grammar, and punctuation errors in the text, keeping the meaning and style unchanged. Only output the transformed text.",
    },
    Spec {
        id: "aicmd.shorten",
        title: "Make Shorter",
        subtitle: "Remove redundancy, keep the key points",
        icon: "shrink",
        instruction: "Condense the text: remove redundancy and repetition, keep the core information and key details, and introduce no new information. Only output the transformed text.",
    },
    Spec {
        id: "aicmd.lengthen",
        title: "Make Longer",
        subtitle: "Add details and arguments for more substance",
        icon: "expand",
        instruction: "Expand the text: add necessary details, examples, and arguments to make it more complete, keeping the meaning and style. Only output the transformed text.",
    },
    Spec {
        id: "aicmd.simplify",
        title: "Simplify Language",
        subtitle: "Rewrite in simpler language",
        icon: "pen-line",
        instruction: "Rewrite the text in simpler, plainer language to make it easier to read, keeping the meaning. Only output the transformed text.",
    },
    Spec {
        id: "aicmd.summarize",
        title: "Summarize",
        subtitle: "Condense the core points",
        icon: "list-checks",
        instruction: "Summarize this text: extract the core information, clearly structured and well organized. Only output the transformed text.",
    },
    Spec {
        id: "aicmd.translate-en",
        title: "Translate to English",
        subtitle: "Keep the meaning and tone",
        icon: "languages",
        instruction: "Translate the text below into English, keeping the meaning and tone. Only output the transformed text.",
    },
    Spec {
        id: "aicmd.translate-zh",
        title: "Translate to Chinese",
        subtitle: "Keep the meaning and tone",
        icon: "languages",
        instruction: "Translate the text below into Chinese, keeping the meaning and tone. Only output the transformed text.",
    },
    Spec {
        id: "aicmd.tone-professional",
        title: "Tone: Professional",
        subtitle: "Formal and objective, for work settings",
        icon: "briefcase",
        instruction: "Rewrite it to be more formal, professional, and objective, suitable for work and business contexts, keeping the meaning. Only output the transformed text.",
    },
    Spec {
        id: "aicmd.tone-friendly",
        title: "Tone: Friendly",
        subtitle: "Warm and approachable, natural tone",
        icon: "smile",
        instruction: "Rewrite it to be friendlier, warmer, and more approachable, with a natural tone, keeping the meaning. Only output the transformed text.",
    },
    Spec {
        id: "aicmd.key-ideas",
        title: "Extract Key Ideas",
        subtitle: "List the key points as bullets",
        icon: "lightbulb",
        instruction: "Extract the key points of this text and list them as concise bullets. Only output the transformed text.",
    },
    Spec {
        id: "aicmd.continue-writing",
        title: "Continue Writing",
        subtitle: "Keep writing naturally from this text",
        icon: "arrow-right",
        instruction: "Continue writing naturally from this text, keeping the style and tone consistent. Only output the transformed text.",
    },
];

fn spec_of(command_id: &str) -> Option<&'static Spec> {
    SPECS.iter().find(|spec| spec.id == command_id)
}

/// Result card (streaming placeholder / final text): Apply = write back, ⌥⏎ = copy.
fn transform_item(text: &str, pending: bool) -> Item {
    Item {
        id: "aicmd.result".into(),
        title: "AI Command Result".into(),
        subtitle: None,
        icon: Some("sparkles".into()),
        actions: vec![
            Action {
                id: "write-back".into(),
                title: "Write Back Result".into(),
                kind: ActionKind::Primary,
                keybinding: None,
            },
            Action {
                id: "copy".into(),
                title: "Copy".into(),
                kind: ActionKind::Secondary,
                keybinding: Some("⌥⏎".into()),
            },
        ],
        payload: serde_json::Value::Null,
        detail: Some(text.into()),
        pending,
    }
}

/// Guidance/notice card without actions.
fn notice_item(detail: &str) -> Item {
    Item {
        id: "aicmd.notice".into(),
        title: "AI Command".into(),
        subtitle: None,
        icon: Some("sparkles".into()),
        actions: vec![],
        payload: serde_json::Value::Null,
        detail: Some(detail.into()),
        pending: false,
    }
}

/// Input text: selection first, then the input box; None when both are empty (guidance card).
fn resolve_text(query: Option<&str>, selection: Option<&Selection>) -> Option<String> {
    let non_empty = |text: &str| text.trim().chars().count() > 0;
    selection
        .and_then(|selection| selection.text())
        .filter(|text| non_empty(text))
        .or_else(|| query.filter(|text| non_empty(text)))
        .map(str::trim)
        .map(str::to_string)
}

/// Request body for a single transform: system instruction + user text (testable).
fn prompt_body(model: &str, instruction: &str, text: &str, stream: bool) -> serde_json::Value {
    chat_body_with_stream(
        model,
        vec![
            serde_json::json!({ "role": "system", "content": instruction }),
            serde_json::json!({ "role": "user", "content": text }),
        ],
        stream,
    )
}

/// Non-streaming response parsing (blocking invoke path): `choices[0].message.content`.
fn parse_completion(payload: &str) -> Option<String> {
    let json: serde_json::Value = serde_json::from_str(payload).ok()?;
    json["choices"][0]["message"]["content"]
        .as_str()
        .map(str::to_string)
        .filter(|text| !text.is_empty())
}

/// Unique cancellation key per run (no conversation to attach to; Esc stop must still work, ADR-0024).
fn run_key() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("aicmd:{nanos}")
}

/// Blocking one-shot (`invoke` path, not the panel's main path): returns result text or an error card.
fn completion_blocking(
    base_url: &str,
    key: &str,
    body: serde_json::Value,
) -> Result<String, String> {
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|err| format!("Failed to initialize HTTP client: {err}"))?;
    let response = client
        .post(chat_completions_url(base_url))
        .bearer_auth(key)
        .json(&body)
        .send()
        .map_err(|err| format!("Request failed: {err}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let text: String = response
            .text()
            .unwrap_or_default()
            .chars()
            .take(500)
            .collect();
        return Err(format!("Endpoint returned {status}: {text}"));
    }
    let payload = response.text().map_err(|err| err.to_string())?;
    parse_completion(&payload).ok_or_else(|| "(the endpoint returned no content)".to_string())
}

impl Extension for AiCommands {
    fn id(&self) -> &str {
        "ai-commands"
    }

    fn title(&self) -> &str {
        "AI Commands"
    }

    /// The trailing badge on root rows (ADR-0030).
    fn command_kind(&self) -> Option<String> {
        Some("AI Command".into())
    }

    fn commands(&self) -> Vec<CommandMeta> {
        SPECS
            .iter()
            .map(|spec| CommandMeta {
                id: spec.id.into(),
                extension_id: "ai-commands".into(),
                title: spec.title.into(),
                subtitle: Some(spec.subtitle.into()),
                icon: Some(spec.icon.into()),
                input: InputKind::Selection,
                live: false,
                keybinding: None,
                extension_title: None,
                kind: None,
            })
            .collect()
    }

    fn invoke(
        &self,
        command_id: &str,
        query: Option<&str>,
        selection: Option<&Selection>,
    ) -> Result<ActionResult, MoeError> {
        let Some(spec) = spec_of(command_id) else {
            return Err(MoeError::NotFound);
        };
        let Some(text) = resolve_text(query, selection) else {
            return Ok(ActionResult::detail(vec![notice_item(
                "First select the text to transform, or type it into the input box, then press Enter.",
            )]));
        };
        let config = MoeConfig::load();
        if !config.ai.configured() {
            return Ok(ActionResult::detail(vec![notice_item(SETUP_MD)]));
        }
        let Some(key) = keychain::ai_api_key() else {
            return Ok(ActionResult::detail(vec![notice_item(KEY_MISSING_MD)]));
        };
        let base_url = config.ai.base_url.clone().unwrap_or_default();
        let model = config.ai.model_or_default().to_string();
        let body = prompt_body(&model, spec.instruction, &text, false);
        match completion_blocking(&base_url, &key, body) {
            Ok(out) => Ok(ActionResult::WriteBack { text: out }),
            Err(err) => Ok(ActionResult::detail(vec![notice_item(&format!(
                "## Request failed\n\n```\n{err}\n```"
            ))])),
        }
    }

    fn invoke_streaming(
        &self,
        command_id: &str,
        query: Option<&str>,
        selection: Option<&Selection>,
        emitter: Arc<dyn Emitter>,
    ) -> Result<ActionResult, MoeError> {
        let Some(spec) = spec_of(command_id) else {
            return Err(MoeError::NotFound);
        };
        let Some(text) = resolve_text(query, selection) else {
            return Ok(ActionResult::detail(vec![notice_item(
                "First select the text to transform, or type it into the input box, then press Enter.",
            )]));
        };
        let config = MoeConfig::load();
        if !config.ai.configured() {
            return Ok(ActionResult::detail(vec![notice_item(SETUP_MD)]));
        }
        let Some(key) = keychain::ai_api_key() else {
            return Ok(ActionResult::detail(vec![notice_item(KEY_MISSING_MD)]));
        };
        let base_url = config.ai.base_url.clone().unwrap_or_default();
        let model = config.ai.model_or_default().to_string();
        let body = prompt_body(&model, spec.instruction, &text, true);

        let stream_key = run_key();
        let item_of: ai::FrameBuilder = Arc::new(transform_item);
        let on_done: ai::StreamFinish = {
            let emitter = emitter.clone();
            Arc::new(move |text| {
                // Natural completion: ask the platform to write back to the selection and close the panel (run_stream does not call back when stopped with Esc).
                emitter.emit(CommandEvent::WriteBack {
                    text: text.to_string(),
                });
            })
        };
        let command_id = command_id.to_string();
        let emitter = emitter.clone();
        std::thread::spawn(move || {
            ai::run_stream(
                ai::StreamRequest {
                    base_url,
                    key,
                    body,
                    command_id,
                    stream_key,
                    item_of,
                    persist: Arc::new(ai::noop_sink),
                    on_done,
                },
                emitter,
            );
        });
        // Placeholder frame: body left empty — the "generating" inline indicator is already the only status feedback
        Ok(ActionResult::detail(vec![transform_item("", true)]))
    }

    fn run_item_action(
        &self,
        _command_id: &str,
        item: &Item,
        action: &Action,
    ) -> Result<ActionResult, MoeError> {
        match action.id.as_str() {
            "write-back" => Ok(ActionResult::WriteBack {
                text: item.detail.clone().unwrap_or_else(|| item.title.clone()),
            }),
            "copy" => {
                let text = item.detail.clone().unwrap_or_else(|| item.title.clone());
                moe_platform::clipboard::copy(&text)
                    .map_err(|err| MoeError::Internal(err.to_string()))?;
                Ok(ActionResult::Silent)
            }
            _ => Err(MoeError::NotFound),
        }
    }

    /// Stops in-progress generation (shares the same cancellation registry as AI Q&A).
    fn stop_generation(&self) -> usize {
        ai::stop_all_streams()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use moe_core::contract::Extension;

    fn selection(text: &str) -> Selection {
        Selection {
            text: Some(text.into()),
            files: vec![],
        }
    }

    /// Same guard: every command is registered in commands(), so Apply can route to it.
    #[test]
    fn every_command_is_registered_and_routable() {
        let ext = AiCommands;
        let ids: Vec<String> = ext.commands().into_iter().map(|c| c.id).collect();
        for spec in SPECS.iter() {
            assert!(
                ids.contains(&spec.id.to_string()),
                "{} not registered",
                spec.id
            );
        }
        assert!(
            ids.len() >= 10,
            "commands must be a substantial set: {}",
            ids.len()
        );
    }

    /// Input resolution: selection first; no selection → input box text; neither → None (guidance card).
    #[test]
    fn resolve_text_prefers_selection_then_query() {
        assert_eq!(
            resolve_text(None, Some(&selection("  selected text "))).as_deref(),
            Some("selected text")
        );
        assert_eq!(
            resolve_text(Some("input box text"), Some(&selection("selected text"))).as_deref(),
            Some("selected text"),
            "selection wins"
        );
        assert_eq!(
            resolve_text(Some("  input box text "), None).as_deref(),
            Some("input box text")
        );
        assert_eq!(resolve_text(Some("   "), None), None);
        assert_eq!(resolve_text(None, None), None);
        assert_eq!(resolve_text(Some(""), Some(&selection("  "))), None);
    }

    /// Request body: system instruction first, user text second; stream can be off (blocking path).
    #[test]
    fn prompt_body_puts_instruction_before_text() {
        let body = prompt_body("m", "Only output the translation.", "hello", false);
        assert_eq!(body["stream"], false);
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(
            body["messages"][0]["content"],
            "Only output the translation."
        );
        assert_eq!(body["messages"][1]["role"], "user");
        assert_eq!(body["messages"][1]["content"], "hello");
        let streaming = prompt_body("m", "instruction", "text", true);
        assert_eq!(streaming["stream"], true);
    }

    /// Non-streaming response parsing: takes message.content; empty content → None.
    #[test]
    fn parse_completion_reads_message_content() {
        let payload =
            r#"{"choices":[{"message":{"role":"assistant","content":"the rewritten text"}}]}"#;
        assert_eq!(
            parse_completion(payload).as_deref(),
            Some("the rewritten text")
        );
        assert_eq!(parse_completion(r#"{"choices":[]}"#), None);
        assert_eq!(parse_completion("not-json"), None);
        assert_eq!(
            parse_completion(r#"{"choices":[{"message":{"content":""}}]}"#),
            None
        );
    }

    /// Result card actions: Apply writes back, ⌥⏎ copies (the run_item_action landing spots).
    #[test]
    fn transform_item_actions_write_back_and_copy() {
        let item = transform_item("result", false);
        let write = &item.actions[0];
        assert_eq!(write.kind, ActionKind::Primary);
        assert_eq!(
            AiCommands
                .run_item_action("aicmd.improve", &item, write)
                .unwrap(),
            ActionResult::WriteBack {
                text: "result".into()
            }
        );
    }

    /// End to end: the stream finishes naturally → the last frame has pending=false, and on_done
    /// receives the final full text (AI commands turn it into a `WriteBack` auto write-back event,
    /// ADR-0024; the stop path is covered by the stop test in ai.rs).
    #[test]
    fn streaming_completion_invokes_on_done_with_final_text() {
        let _guard = ai::STREAM_TEST_LOCK.lock().expect("stream test lock");
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::Mutex;

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let server = std::thread::spawn(move || {
            let Ok((mut socket, _)) = listener.accept() else {
                return;
            };
            let mut buf = [0u8; 4096];
            let _ = socket.read(&mut buf); // request headers
            let _ = socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\n");
            let delta = |content: &str| {
                format!("data: {{\"choices\":[{{\"delta\":{{\"content\":\"{content}\"}}}}]}}\n\n")
            };
            let _ = socket.write_all(delta("Improved").as_bytes());
            let _ = socket.write_all(delta(" text").as_bytes());
            let _ = socket.write_all(b"data: [DONE]\n\n");
        });

        #[derive(Default)]
        struct Recorder {
            items: Mutex<Vec<Item>>,
            done_texts: Mutex<Vec<String>>,
        }
        impl Emitter for Recorder {
            fn emit(&self, event: CommandEvent) {
                match event {
                    CommandEvent::ItemUpdated { item, .. } => {
                        self.items.lock().unwrap().push(item);
                    }
                    CommandEvent::WriteBack { text } => {
                        self.done_texts.lock().unwrap().push(text);
                    }
                }
            }
        }
        let recorder = Arc::new(Recorder::default());
        let emitter: Arc<dyn Emitter> = recorder.clone();
        let base_url = format!("http://{addr}/v1");
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        {
            // on_done = the AI commands' auto write-back callback (this is the shape used in invoke_streaming)
            let recorder = recorder.clone();
            std::thread::spawn(move || {
                ai::run_stream(
                    ai::StreamRequest {
                        base_url,
                        key: "test-key".into(),
                        body: prompt_body("m", "Only output the translation.", "hello", true),
                        command_id: "aicmd.improve".into(),
                        stream_key: "aicmd:test-run".into(),
                        item_of: Arc::new(transform_item),
                        persist: Arc::new(ai::noop_sink),
                        on_done: Arc::new(move |text| {
                            recorder.done_texts.lock().unwrap().push(text.to_string());
                        }),
                    },
                    emitter,
                );
                let _ = done_tx.send(());
            });
        }
        done_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the stream should finish naturally");

        let items = recorder.items.lock().unwrap();
        let last = items.last().expect("at least one frame");
        assert!(!last.pending, "the last frame should have pending=false");
        assert_eq!(last.detail.as_deref(), Some("Improved text"));
        let done_texts = recorder.done_texts.lock().unwrap();
        assert_eq!(
            done_texts.as_slice(),
            ["Improved text"],
            "on natural completion the callback gets the final full text"
        );
        let _ = server.join();
    }
}
