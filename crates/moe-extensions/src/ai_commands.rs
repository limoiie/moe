//! AI Commands（ADR-0024）：批量文本变换命令——润色 / 修语法 / 缩短 / 扩写 / 简化 /
//! 总结 / 翻译 / 改语气 / 提取要点 / 续写，Raycast Pro 同款语义。
//!
//! 输入 = 呼出前选中的文字，没有选区则用输入框里的文字；两种都没有则给引导卡。
//! 流式产出到面板的结果卡；**自然完成后自动回写选区并收起面板**（`CommandEvent::WriteBack`，
//! 平台层拦截执行）。被 Esc 停止时只保留已生成部分，不自动回写。
//! 单次变换不落库、不建会话（无历史语义）。

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

/// 一条 AI 命令的规格：id / 展示 / 系统指令。
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
        title: "润色",
        subtitle: "让表达更清晰流畅，保持原意",
        icon: "wand-2",
        instruction: "改进写作：让表达更清晰、流畅、专业，保持原意与信息完整。只输出改写后的文本。",
    },
    Spec {
        id: "aicmd.fix-grammar",
        title: "修语法",
        subtitle: "修正拼写与语法错误",
        icon: "spell-check",
        instruction: "修正文本中的拼写、语法与标点错误，保持原意与风格不变。只输出修正后的文本。",
    },
    Spec {
        id: "aicmd.shorten",
        title: "缩短",
        subtitle: "去掉冗余，保留核心信息",
        icon: "shrink",
        instruction: "压缩文本：去掉冗余与重复表达，保留核心信息与关键细节，不引入新信息。只输出压缩后的文本。",
    },
    Spec {
        id: "aicmd.lengthen",
        title: "扩写",
        subtitle: "补充细节与论证，让内容更充实",
        icon: "expand",
        instruction: "扩写文本：补充必要的细节、例子与论证，让内容更充实完整，保持原意与风格。只输出扩写后的文本。",
    },
    Spec {
        id: "aicmd.simplify",
        title: "简化",
        subtitle: "用更简单的语言重写",
        icon: "pen-line",
        instruction: "用更简单、直白的语言重写文本，降低阅读难度，保持原意。只输出重写后的文本。",
    },
    Spec {
        id: "aicmd.summarize",
        title: "总结",
        subtitle: "提炼核心要点",
        icon: "list-checks",
        instruction: "总结这段文字：提炼核心信息，结构清晰、条理分明。只输出总结。",
    },
    Spec {
        id: "aicmd.translate-en",
        title: "翻译成英文",
        subtitle: "保持原意与语气",
        icon: "languages",
        instruction: "把下面的文本翻译成英文，保持原意与语气。只输出译文。",
    },
    Spec {
        id: "aicmd.translate-zh",
        title: "翻译成中文",
        subtitle: "保持原意与语气",
        icon: "languages",
        instruction: "把下面的文本翻译成中文，保持原意与语气。只输出译文。",
    },
    Spec {
        id: "aicmd.tone-professional",
        title: "语气更专业",
        subtitle: "正式、客观，适合工作场合",
        icon: "briefcase",
        instruction: "改写得更正式、专业、客观，适合工作与商务场合，保持原意。只输出改写后的文本。",
    },
    Spec {
        id: "aicmd.tone-friendly",
        title: "语气更友好",
        subtitle: "温暖亲切，语气自然",
        icon: "smile",
        instruction: "改写得更友好、温暖、亲切，语气自然不刻意，保持原意。只输出改写后的文本。",
    },
    Spec {
        id: "aicmd.key-ideas",
        title: "提取要点",
        subtitle: "用项目符号列出关键点",
        icon: "lightbulb",
        instruction: "提取这段文字的关键要点，用简洁的项目符号列出。只输出要点。",
    },
    Spec {
        id: "aicmd.continue-writing",
        title: "续写",
        subtitle: "接着这段文字自然地写下去",
        icon: "arrow-right",
        instruction: "接着这段文字自然地续写下去，风格与语气保持一致。只输出续写的内容。",
    },
];

fn spec_of(command_id: &str) -> Option<&'static Spec> {
    SPECS.iter().find(|spec| spec.id == command_id)
}

/// 结果卡（流式占位 / 最终文本）：Apply = 回写，⌥⏎ = 复制。
fn transform_item(text: &str, pending: bool) -> Item {
    Item {
        id: "aicmd.result".into(),
        title: "AI 命令结果".into(),
        subtitle: None,
        icon: Some("sparkles".into()),
        actions: vec![
            Action {
                id: "write-back".into(),
                title: "回写结果".into(),
                kind: ActionKind::Primary,
                keybinding: None,
            },
            Action {
                id: "copy".into(),
                title: "复制".into(),
                kind: ActionKind::Secondary,
                keybinding: Some("⌥⏎".into()),
            },
        ],
        payload: serde_json::Value::Null,
        detail: Some(text.into()),
        pending,
    }
}

/// 无动作的引导/提示卡。
fn notice_item(detail: &str) -> Item {
    Item {
        id: "aicmd.notice".into(),
        title: "AI 命令".into(),
        subtitle: None,
        icon: Some("sparkles".into()),
        actions: vec![],
        payload: serde_json::Value::Null,
        detail: Some(detail.into()),
        pending: false,
    }
}

/// 输入文本：选区优先，其次输入框文字；两者皆空为 None（走引导卡）。
fn resolve_text(query: Option<&str>, selection: Option<&Selection>) -> Option<String> {
    let non_empty = |text: &str| text.trim().chars().count() > 0;
    selection
        .and_then(|selection| selection.text())
        .filter(|text| non_empty(text))
        .or_else(|| query.filter(|text| non_empty(text)))
        .map(str::trim)
        .map(str::to_string)
}

/// 单次变换的请求体：系统指令 + 用户文本（可测）。
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

/// 非流式响应解析（阻塞 invoke 路径）：`choices[0].message.content`。
fn parse_completion(payload: &str) -> Option<String> {
    let json: serde_json::Value = serde_json::from_str(payload).ok()?;
    json["choices"][0]["message"]["content"]
        .as_str()
        .map(str::to_string)
        .filter(|text| !text.is_empty())
}

/// 每次运行唯一的取消登记键（无会话可挂靠，Esc 停止仍要可用，ADR-0024）。
fn run_key() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("aicmd:{nanos}")
}

/// 阻塞式单发（`invoke` 路径，非面板主路径）：返回结果文本或错误卡。
fn completion_blocking(
    base_url: &str,
    key: &str,
    body: serde_json::Value,
) -> Result<String, String> {
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|err| format!("无法初始化 HTTP 客户端：{err}"))?;
    let response = client
        .post(chat_completions_url(base_url))
        .bearer_auth(key)
        .json(&body)
        .send()
        .map_err(|err| format!("请求失败：{err}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let text: String = response
            .text()
            .unwrap_or_default()
            .chars()
            .take(500)
            .collect();
        return Err(format!("端点返回 {status}：{text}"));
    }
    let payload = response.text().map_err(|err| err.to_string())?;
    parse_completion(&payload).ok_or_else(|| "（端点没有返回内容）".to_string())
}

impl Extension for AiCommands {
    fn id(&self) -> &str {
        "ai-commands"
    }

    fn title(&self) -> &str {
        "AI Commands"
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
                "先选中要处理的文字，或在输入框里输入文字，再按 Enter。",
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
                "## 请求失败\n\n```\n{err}\n```"
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
                "先选中要处理的文字，或在输入框里输入文字，再按 Enter。",
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
                // 自然完成：请求平台回写选区并收起面板（被 Esc 停止时 run_stream 不回调）。
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
        // 占位帧：正文留空——「正在生成」行内指示已经是唯一的状态反馈
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

    /// 停止进行中的生成（与 AI 问答共用同一套取消登记）。
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

    /// 同款守卫：每条命令都注册在 commands() 里，Apply 才能路由到。
    #[test]
    fn every_command_is_registered_and_routable() {
        let ext = AiCommands;
        let ids: Vec<String> = ext.commands().into_iter().map(|c| c.id).collect();
        for spec in SPECS.iter() {
            assert!(ids.contains(&spec.id.to_string()), "{} 未注册", spec.id);
        }
        assert!(ids.len() >= 10, "命令要成规模：{}", ids.len());
    }

    /// 输入解析：选区优先；没有选区用输入框文字；都没有 → None（引导卡）。
    #[test]
    fn resolve_text_prefers_selection_then_query() {
        assert_eq!(
            resolve_text(None, Some(&selection("  选中文字 "))).as_deref(),
            Some("选中文字")
        );
        assert_eq!(
            resolve_text(Some("输入框文字"), Some(&selection("选中文字"))).as_deref(),
            Some("选中文字"),
            "选区优先"
        );
        assert_eq!(
            resolve_text(Some("  输入框文字 "), None).as_deref(),
            Some("输入框文字")
        );
        assert_eq!(resolve_text(Some("   "), None), None);
        assert_eq!(resolve_text(None, None), None);
        assert_eq!(resolve_text(Some(""), Some(&selection("  "))), None);
    }

    /// 请求体：系统指令在前、用户文本在后；stream 可关（阻塞路径）。
    #[test]
    fn prompt_body_puts_instruction_before_text() {
        let body = prompt_body("m", "只输出译文。", "你好", false);
        assert_eq!(body["stream"], false);
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][0]["content"], "只输出译文。");
        assert_eq!(body["messages"][1]["role"], "user");
        assert_eq!(body["messages"][1]["content"], "你好");
        let streaming = prompt_body("m", "指令", "文本", true);
        assert_eq!(streaming["stream"], true);
    }

    /// 非流式响应解析：拿 message.content；空内容 None。
    #[test]
    fn parse_completion_reads_message_content() {
        let payload = r#"{"choices":[{"message":{"role":"assistant","content":"改写后的文本"}}]}"#;
        assert_eq!(parse_completion(payload).as_deref(), Some("改写后的文本"));
        assert_eq!(parse_completion(r#"{"choices":[]}"#), None);
        assert_eq!(parse_completion("not-json"), None);
        assert_eq!(
            parse_completion(r#"{"choices":[{"message":{"content":""}}]}"#),
            None
        );
    }

    /// 结果卡动作：Apply 回写、⌥⏎ 复制（run_item_action 的落点）。
    #[test]
    fn transform_item_actions_write_back_and_copy() {
        let item = transform_item("结果", false);
        let write = &item.actions[0];
        assert_eq!(write.kind, ActionKind::Primary);
        assert_eq!(
            AiCommands
                .run_item_action("aicmd.improve", &item, write)
                .unwrap(),
            ActionResult::WriteBack {
                text: "结果".into()
            }
        );
    }

    /// 端到端：流自然完成 → 末帧 pending=false，且 on_done 拿到最终全文（AI 命令把它发成
    /// `WriteBack` 自动回写事件，ADR-0024；停止路径已在 ai.rs 的 stop 测试覆盖）。
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
            let _ = socket.read(&mut buf); // 请求头
            let _ = socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\n");
            let delta = |content: &str| {
                format!("data: {{\"choices\":[{{\"delta\":{{\"content\":\"{content}\"}}}}]}}\n\n")
            };
            let _ = socket.write_all(delta("润色").as_bytes());
            let _ = socket.write_all(delta("完成").as_bytes());
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
            // on_done = AI 命令的自动回写回调（invoke_streaming 里就是这个形状）
            let recorder = recorder.clone();
            std::thread::spawn(move || {
                ai::run_stream(
                    ai::StreamRequest {
                        base_url,
                        key: "test-key".into(),
                        body: prompt_body("m", "只输出译文。", "你好", true),
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
            .expect("流应自然完成");

        let items = recorder.items.lock().unwrap();
        let last = items.last().expect("至少一帧");
        assert!(!last.pending, "末帧应为 pending=false");
        assert_eq!(last.detail.as_deref(), Some("润色完成"));
        let done_texts = recorder.done_texts.lock().unwrap();
        assert_eq!(
            done_texts.as_slice(),
            ["润色完成"],
            "自然完成时回调最终全文"
        );
        let _ = server.join();
    }
}
