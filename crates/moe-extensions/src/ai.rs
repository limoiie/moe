//! AI 问答：OpenAI 兼容端点的真实流式回答（ADR-0005）。
//!
//! - 快捷问答：提问即建会话（Namespace `ai`），回答落库（IIE4AD-360）。
//! - 侧栏续聊：整段历史作为上下文；事件 command_id 约定 `ai.side`。
//! - 流式走 `CommandEvent::ItemUpdated`：invoke 立即返回占位 item，
//!   worker 线程收 SSE 并逐段 emit（就地在详情卡片/侧栏重渲）。

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use moe_core::contract::{
    Action, ActionKind, ActionResult, CommandEvent, CommandMeta, Emitter, Extension, InputKind,
    Item, MoeError,
};
use moe_core::conversation::{AttachmentRef, Conversation, Message, Role};
use moe_platform::config::MoeConfig;
use moe_platform::db::Db;
use moe_platform::keychain;

use crate::ai_client::{SseLine, chat_body_from_messages, chat_completions_url, sse_delta};
use crate::attachment;

pub struct AiShell;

/// 侧栏续聊的事件 command_id 约定（chat 窗口按 `item.payload.conversationId` 过滤）。
pub const SIDE_COMMAND_ID: &str = "ai.side";

/// 历史列表条数上限。
const HISTORY_LIMIT: usize = 20;

/// 会话标题长度（超出截断并加省略号）。
const TITLE_CHARS: usize = 40;

fn answer_item(detail: &str, conversation_id: Option<&str>, pending: bool) -> Item {
    Item {
        id: "ai.answer".into(),
        title: "AI 回答".into(),
        subtitle: None,
        icon: Some("sparkles".into()),
        actions: vec![
            Action {
                id: "write-back".into(),
                title: "回写回答".into(),
                kind: ActionKind::Primary,
                keybinding: None,
            },
            Action {
                id: "copy".into(),
                title: "复制".into(),
                kind: ActionKind::Secondary,
                keybinding: Some("⌥⏎".into()),
            },
            Action {
                id: "materialize".into(),
                title: "实体化为侧栏".into(),
                kind: ActionKind::Secondary,
                keybinding: Some("⌘M".into()),
            },
        ],
        payload: conversation_payload(conversation_id),
        detail: Some(detail.into()),
        pending,
    }
}

/// Item payload 载会话 id，Apply/⌘M 时原样带回（UI 不解释内容）。
fn conversation_payload(conversation_id: Option<&str>) -> serde_json::Value {
    match conversation_id {
        Some(id) => serde_json::json!({ "conversationId": id }),
        None => serde_json::Value::Null,
    }
}

/// 历史/续聊列表项：Apply 即在侧栏打开该会话；`preview` 是最后一条回答的摘要（IIE4AD-370）。
fn history_item(conversation: Conversation, now_unix: u64, preview: Option<String>) -> Item {
    let payload = conversation_payload(Some(&conversation.id));
    Item {
        id: format!("ai.conversation.{}", conversation.id),
        title: conversation.title,
        subtitle: Some(relative_time(conversation.updated_unix, now_unix)),
        icon: Some("message-square".into()),
        actions: vec![Action {
            id: "materialize".into(),
            title: "在侧栏打开".into(),
            kind: ActionKind::Primary,
            keybinding: Some("⏎".into()),
        }],
        payload,
        detail: preview,
        pending: false,
    }
}

/// 历史预览摘要：去掉多余空白，按字符截断（不破坏多字节）。
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

/// 无动作的提示项（空历史、读取失败等）。
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

/// 会话标题：问题首行，超长截断。
fn conversation_title(question: &str) -> String {
    let first_line = question.lines().next().unwrap_or_default().trim();
    let mut title: String = first_line.chars().take(TITLE_CHARS).collect();
    if first_line.chars().count() > TITLE_CHARS {
        title.push('…');
    }
    title
}

/// 会话标题：问题首行，超长截断；纯附件（无文本）时用附件名。
fn title_for(display: &str, attachments: &[AttachmentRef]) -> String {
    let title = conversation_title(display);
    if !title.is_empty() {
        return title;
    }
    match attachments.first() {
        Some(first) => format!("附件：{}", first.name),
        None => "新对话".into(),
    }
}

/// 列表副标题的相对时间。
fn relative_time(updated_unix: u64, now_unix: u64) -> String {
    let secs = now_unix.saturating_sub(updated_unix);
    match secs {
        0..=59 => "刚刚".into(),
        60..=3599 => format!("{} 分钟前", secs / 60),
        3600..=86399 => format!("{} 小时前", secs / 3600),
        _ => format!("{} 天前", secs / 86400),
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// 提问即建会话（历史按 `ai` Namespace 隔离，ADR-0003；附件只存引用，ADR-0010）；
/// 存储不可用时降级为不持久化。
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

/// 回答落库；失败静默（不影响面板交互）。
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

/// 进行中的生成：会话 id → 取消标记（IIE4AD-365）。
fn active_streams() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    static ACTIVE: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();
    ACTIVE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 登记一次生成（无持久化会话时不可中断，返回 None）。
fn begin_stream(conversation_id: Option<&str>) -> Option<Arc<AtomicBool>> {
    let conversation_id = conversation_id?;
    let flag = Arc::new(AtomicBool::new(false));
    active_streams()
        .lock()
        .expect("streams poisoned")
        .insert(conversation_id.to_string(), Arc::clone(&flag));
    Some(flag)
}

/// 收尾：从登记表移除（worker 结束/被中止时调用）。
fn end_stream(conversation_id: Option<&str>) {
    let Some(conversation_id) = conversation_id else {
        return;
    };
    active_streams()
        .lock()
        .expect("streams poisoned")
        .remove(conversation_id);
}

/// 标记全部进行中的生成为取消；worker 在下一个 SSE 行处收尾。返回中止数量。
fn stop_all_streams() -> usize {
    let streams = active_streams().lock().expect("streams poisoned");
    for flag in streams.values() {
        flag.store(true, Ordering::Relaxed);
    }
    streams.len()
}

/// 请求 + SSE 循环：增量逐段 emit（pending=true），结束后落库（有会话时）。
/// 被停止（IIE4AD-365）时保留已生成部分并落库，仅中止后续读取。
fn run_stream(
    base_url: &str,
    key: &str,
    body: serde_json::Value,
    command_id: &str,
    conversation_id: Option<String>,
    emitter: Arc<dyn Emitter>,
) {
    let cancel = begin_stream(conversation_id.as_deref());
    let emit = |text: String, pending: bool| {
        emitter.emit(CommandEvent::ItemUpdated {
            command_id: command_id.to_string(),
            item: answer_item(&text, conversation_id.as_deref(), pending),
        });
    };
    let fail = |text: String| {
        persist_assistant(conversation_id.as_deref(), &text);
        emit(text, false);
    };

    let client = match reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(300))
        .build()
    {
        Ok(client) => client,
        Err(err) => {
            fail(format!("## 请求失败\n\n无法初始化 HTTP 客户端：{err}"));
            end_stream(conversation_id.as_deref());
            return;
        }
    };
    let response = client
        .post(chat_completions_url(base_url))
        .bearer_auth(key)
        .json(&body)
        .send();
    let response = match response {
        Ok(response) => response,
        Err(err) => {
            fail(format!("## 请求失败\n\n```\n{err}\n```"));
            end_stream(conversation_id.as_deref());
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
        fail(format!("## 端点返回 {status}\n\n```\n{body}\n```"));
        end_stream(conversation_id.as_deref());
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
    end_stream(conversation_id.as_deref());

    if stopped {
        let text = if acc.is_empty() {
            "（已停止生成）".to_string()
        } else {
            format!("{acc}\n\n_（已停止生成）_")
        };
        persist_assistant(conversation_id.as_deref(), &text);
        emit(text, false);
    } else if acc.is_empty() {
        fail("（端点没有返回内容）".into());
    } else {
        persist_assistant(conversation_id.as_deref(), &acc);
        emit(acc, false);
    }
}

const SETUP_MD: &str = "## 还没有配置 AI 端点\n\n\
1. 运行命令「Moe: 打开配置文件」，填入：\n\n\
```toml\n\
[ai]\n\
base_url = \"https://api.deepseek.com/v1\"\n\
model = \"deepseek-chat\"\n\
```\n\n\
2. 存 API key：输入 `key <你的key>` 回车（只进 keychain，不回显；也可设 `MOE_AI_API_KEY` 环境变量）。\n\n\
支持任意 OpenAI 兼容端点（DeepSeek / OpenRouter / 本地 llama.cpp 等）。";

const KEY_MISSING_MD: &str = "## 端点已配置，但还缺 API key\n\n\
输入 `key <你的key>` 回车即可存入 keychain（不回显）；也可设 `MOE_AI_API_KEY` 环境变量。";

impl AiShell {
    fn start_ask(&self, question: &str, emitter: Option<Arc<dyn Emitter>>) -> ActionResult {
        let question = question.trim().to_string();
        // 附件以 `@path` mention 表达（ADR-0010）：内容现读，引用随消息落库。
        let (cleaned, paths) = attachment::parse_mentions(&question);
        let display = if cleaned.is_empty() {
            question.clone()
        } else {
            cleaned
        };
        if display.trim().is_empty() {
            return ActionResult::detail(vec![answer_item(
                "在输入框里写下问题，再按 Enter。",
                None,
                false,
            )]);
        }
        let attachments: Vec<AttachmentRef> = paths
            .iter()
            .map(|path| attachment::ref_for_path(path))
            .collect();

        let config = MoeConfig::load();
        if !config.ai.configured() {
            return ActionResult::detail(vec![answer_item(SETUP_MD, None, false)]);
        }
        let Some(key) = keychain::ai_api_key() else {
            return ActionResult::detail(vec![answer_item(KEY_MISSING_MD, None, false)]);
        };

        // 提问即建会话；随后 ⌘M 可带同一会话进侧栏续聊。
        let conversation_id = persist_question(&display, &attachments);

        let base_url = config.ai.base_url.clone().unwrap_or_default();
        let model = config.ai.model_or_default().to_string();

        if let Some(emitter) = emitter {
            let message = Message {
                role: Role::User,
                content: display.clone(),
                attachments,
            };
            let body = chat_body_from_messages(&model, vec![attachment::expand_message(&message)]);
            let conversation_id_in_thread = conversation_id.clone();
            std::thread::spawn(move || {
                run_stream(
                    &base_url,
                    &key,
                    body,
                    "ai.quick-ask",
                    conversation_id_in_thread,
                    emitter,
                );
            });
        }
        ActionResult::detail(vec![answer_item(
            "正在回答…",
            conversation_id.as_deref(),
            true,
        )])
    }

    /// 历史搜索：列表随输入重跑（CommandMeta.live），标题模糊匹配、最近优先。
    fn search_history(&self, query: &str) -> ActionResult {
        match Db::open_default() {
            Ok(db) => Self::search_history_in(&db, query),
            Err(err) => ActionResult::list(vec![notice_item("读取历史失败", &err)]),
        }
    }

    /// DB 注入版：测试用临时库断言形态与条目，不碰真实数据目录。
    ///
    /// 形态恒为列表（ADR-0013）：唯一一条命中也不整屏，UI 才有「左列表 + 右预览」。
    fn search_history_in(db: &Db, query: &str) -> ActionResult {
        let now = unix_now();
        let items = match db.conversations("ai", Some(query), HISTORY_LIMIT) {
            Ok(list) if list.is_empty() => vec![notice_item(
                "没有匹配的历史会话",
                "在命令盘直接输入问题，即可开始一次新对话。",
            )],
            Ok(list) => list
                .into_iter()
                .map(|conversation| {
                    // 预览 = 最后一条回答的摘要（进焦点预览卡片，IIE4AD-370）
                    let preview = db
                        .last_assistant_message(&conversation.id)
                        .ok()
                        .flatten()
                        .map(|text| preview_excerpt(&text));
                    history_item(conversation, now, preview)
                })
                .collect(),
            Err(err) => vec![notice_item("读取历史失败", &err)],
        };
        ActionResult::list(items)
    }
}

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
                subtitle: Some("直接输入问题也可（无匹配时自动出现提问项）".into()),
                icon: Some("sparkles".into()),
                input: InputKind::Query,
                live: false,
            },
            CommandMeta {
                id: "ai.search-history".into(),
                extension_id: "ai".into(),
                title: "AI: 搜索历史会话".into(),
                subtitle: Some("输入即筛选标题；Enter 在侧栏打开".into()),
                icon: Some("history".into()),
                input: InputKind::Query,
                live: true,
            },
            CommandMeta {
                id: "ai.new-chat".into(),
                extension_id: "ai".into(),
                title: "AI: 新对话".into(),
                subtitle: Some("清掉当前问答，回到干净的一轮新会话".into()),
                icon: Some("plus".into()),
                input: InputKind::None,
                live: false,
            },
        ]
    }

    fn invoke(
        &self,
        command_id: &str,
        query: Option<&str>,
        _selection: Option<&str>,
    ) -> Result<ActionResult, MoeError> {
        match command_id {
            "ai.quick-ask" => Ok(self.start_ask(query.unwrap_or_default(), None)),
            "ai.search-history" => Ok(self.search_history(query.unwrap_or_default())),
            // 通用动作 New（⌘N，ADR-0014）：面板不持有会话状态（一问一会话），
            // 这里给一张空态卡——下一条提问自然开一段新会话。
            "ai.new-chat" => Ok(ActionResult::detail(vec![notice_item(
                "新对话",
                "在输入栏写下问题即可开始；⌘M 可转进侧栏继续这段会话。",
            )])),
            _ => Err(MoeError::NotFound),
        }
    }

    /// 通用动作 Browse（⌘P，ADR-0014）：AI 的记录列表 = 历史会话。
    fn browse_command(&self) -> Option<CommandMeta> {
        self.commands()
            .into_iter()
            .find(|c| c.id == "ai.search-history")
    }

    /// 通用动作 New（⌘N，ADR-0014）：AI 的新建 = 新会话。
    fn new_command(&self) -> Option<CommandMeta> {
        self.commands().into_iter().find(|c| c.id == "ai.new-chat")
    }

    fn invoke_streaming(
        &self,
        command_id: &str,
        query: Option<&str>,
        _selection: Option<&str>,
        emitter: Arc<dyn Emitter>,
    ) -> Result<ActionResult, MoeError> {
        match command_id {
            "ai.quick-ask" => Ok(self.start_ask(query.unwrap_or_default(), Some(emitter))),
            _ => self.invoke(command_id, query, None),
        }
    }

    fn fallback_command(&self, query: &str) -> Option<CommandMeta> {
        // 标题剥离 mention（ADR-0010）：路径不当作提问内容展示。
        let (cleaned, paths) = attachment::parse_mentions(query);
        let title = match (cleaned.is_empty(), paths.is_empty()) {
            (false, _) => format!("AI: 提问「{cleaned}」"),
            (true, false) => "AI: 带附件的提问".into(),
            (true, true) => format!("AI: 提问「{query}」"),
        };
        let subtitle = if paths.is_empty() {
            "Enter 发送；回答可回写（⌥⏎ 复制 · ⌘M 侧栏）".to_string()
        } else {
            format!("{} 个附件；Enter 发送", paths.len())
        };
        Some(CommandMeta {
            id: "ai.quick-ask".into(),
            extension_id: "ai".into(),
            title,
            subtitle: Some(subtitle),
            icon: Some("sparkles".into()),
            input: InputKind::Query,
            live: false,
        })
    }

    fn run_item_action(
        &self,
        _command_id: &str,
        item: &Item,
        action: &Action,
    ) -> Result<ActionResult, MoeError> {
        match action.id.as_str() {
            // 回写全文（标题是固定的，正文在 detail）
            "write-back" => Ok(ActionResult::WriteBack {
                text: item.detail.clone().unwrap_or_else(|| item.title.clone()),
            }),
            // ⌥⏎ 复制：写系统剪贴板（平台能力），不回写宿主应用
            "copy" => {
                let text = item.detail.clone().unwrap_or_else(|| item.title.clone());
                moe_platform::clipboard::copy(&text)
                    .map_err(|err| MoeError::Internal(err.to_string()))?;
                Ok(ActionResult::Silent)
            }
            // 侧栏开窗：载荷原样交给平台的 chat 窗口。
            "materialize" => Ok(ActionResult::OpenSideView {
                payload: item.payload.clone(),
            }),
            _ => Err(MoeError::NotFound),
        }
    }

    /// 侧栏续聊（IIE4AD-360）：空 id 新建会话；整段历史作为上下文，回复后台流式落库。
    fn side_continue(
        &self,
        conversation_id: &str,
        message: &str,
        emitter: Arc<dyn Emitter>,
    ) -> Result<String, MoeError> {
        let message = message.trim();
        if message.is_empty() {
            return Err(MoeError::Internal("消息为空".into()));
        }
        let config = MoeConfig::load();
        if !config.ai.configured() {
            return Err(MoeError::Internal(
                "尚未配置 AI 端点（命令盘运行「Moe: 打开配置文件」）".into(),
            ));
        }
        let Some(key) = keychain::ai_api_key() else {
            return Err(MoeError::Internal(
                "缺少 API key（在命令盘输入 key <你的key> 回车）".into(),
            ));
        };

        let db = Db::open_default().map_err(MoeError::Internal)?;

        // 附件 mention 与快捷提问同一语法（ADR-0010）。
        let (cleaned, paths) = attachment::parse_mentions(message);
        let display = if cleaned.is_empty() {
            message.to_string()
        } else {
            cleaned
        };
        let attachments: Vec<AttachmentRef> = paths
            .iter()
            .map(|path| attachment::ref_for_path(path))
            .collect();

        let conversation_id = if conversation_id.trim().is_empty() {
            db.create_conversation("ai", &title_for(&display, &attachments), SystemTime::now())
                .map_err(MoeError::Internal)?
        } else {
            conversation_id.trim().to_string()
        };

        // 先取历史再追加本条：端点收到的 messages 以新消息结尾（附件逐条现读展开）。
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
        std::thread::spawn(move || {
            run_stream(
                &base_url,
                &key,
                body,
                SIDE_COMMAND_ID,
                Some(conversation),
                emitter,
            );
        });
        Ok(conversation_id)
    }

    /// 停止进行中的生成（IIE4AD-365）：标记取消位，worker 在下一个 SSE 行处收尾并落库已生成部分。
    fn stop_generation(&self) -> usize {
        stop_all_streams()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use moe_core::contract::Extension;
    use std::time::Instant;

    /// 同款守卫：fallback 合成的 id 必须可路由；标题剥离附件 mention（IIE4AD-358）。
    #[test]
    fn fallback_command_is_invocable_and_strips_mentions() {
        let ext = AiShell;
        let fallback = ext.fallback_command("hello").expect("fallback");
        assert!(
            ext.commands().iter().any(|c| c.id == fallback.id),
            "fallback id 必须在 commands() 中可路由：{}",
            fallback.id
        );

        let with_attachments = ext
            .fallback_command("总结 @\"/tmp/a b.md\"")
            .expect("fallback");
        assert_eq!(with_attachments.title, "AI: 提问「总结」");
        assert_eq!(
            with_attachments.subtitle.as_deref(),
            Some("1 个附件；Enter 发送")
        );

        let attachment_only = ext.fallback_command("@/tmp/a.md").expect("fallback");
        assert_eq!(attachment_only.title, "AI: 带附件的提问");
    }

    /// 历史搜索是 Live 命令（输入变化即重跑列表）。
    #[test]
    fn history_search_is_live_and_routable() {
        let ext = AiShell;
        let history = ext
            .commands()
            .into_iter()
            .find(|c| c.id == "ai.search-history")
            .expect("search-history registered");
        assert!(history.live, "历史搜索需随输入重跑");
    }

    #[test]
    fn conversation_title_takes_first_line_and_truncates() {
        assert_eq!(conversation_title("解释闭包"), "解释闭包");
        assert_eq!(conversation_title("第一行\n第二行"), "第一行");
        let long = "字".repeat(50);
        let title = conversation_title(&long);
        assert_eq!(title.chars().count(), TITLE_CHARS + 1);
        assert!(title.ends_with('…'));
    }

    /// 纯附件会话：标题回退到附件名（IIE4AD-358）。
    #[test]
    fn title_falls_back_to_attachment_name() {
        let refs = vec![AttachmentRef {
            name: "shot.png".into(),
            path: "/tmp/shot.png".into(),
        }];
        assert_eq!(title_for("总结图片", &refs), "总结图片");
        assert_eq!(title_for("", &refs), "附件：shot.png");
        assert_eq!(title_for("", &[]), "新对话");
    }

    /// 预览摘要：空白归一 + 按字符截断（IIE4AD-370）。
    #[test]
    fn preview_excerpt_collapses_and_truncates() {
        assert_eq!(preview_excerpt("第一行\n\n第二行"), "第一行 第二行");
        let long = "字".repeat(300);
        let excerpt = preview_excerpt(&long);
        assert_eq!(excerpt.chars().count(), 241);
        assert!(excerpt.ends_with('…'));
    }

    #[test]
    fn relative_time_buckets() {
        assert_eq!(relative_time(100, 100), "刚刚");
        assert_eq!(relative_time(100, 159), "刚刚");
        assert_eq!(relative_time(100, 160), "1 分钟前");
        assert_eq!(relative_time(100, 100 + 3599), "59 分钟前");
        assert_eq!(relative_time(100, 100 + 3600), "1 小时前");
        assert_eq!(relative_time(100, 100 + 86400), "1 天前");
        assert_eq!(relative_time(100, 100 + 3 * 86400), "3 天前");
    }

    /// 历史项的 Apply（materialize）必须把会话 id 交给平台开侧栏。
    #[test]
    fn history_item_materializes_with_conversation_id() {
        let item = history_item(
            Conversation {
                id: "7".into(),
                namespace: "ai".into(),
                title: "解释闭包".into(),
                updated_unix: 100,
            },
            160,
            Some("上次回答的摘要".into()),
        );
        assert_eq!(item.subtitle.as_deref(), Some("1 分钟前"));
        assert_eq!(item.detail.as_deref(), Some("上次回答的摘要"));
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

    /// 停止生成：登记后请求停止应命中取消位（IIE4AD-365）。
    #[test]
    fn stop_all_streams_marks_active_flags() {
        let flag = begin_stream(Some("test-stop-conv")).expect("登记取消位");
        assert!(!flag.load(Ordering::Relaxed));
        assert!(stop_all_streams() >= 1, "应至少命中刚登记的取消位");
        assert!(flag.load(Ordering::Relaxed), "取消位应被标记");
        end_stream(Some("test-stop-conv"));
        // 无持久化会话（id 为空）时不可中断
        assert!(begin_stream(None).is_none());
    }

    /// 端到端：慢速 SSE 流中途 `stop_generation` → 及时收尾，末帧 pending=false 且标记已停止。
    #[test]
    fn stop_generation_stops_running_stream() {
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
            let _ = socket.read(&mut buf); // 请求头
            let _ = socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\n");
            // 故意比测试的等待窗口长得多（~50s）：只要还没被停止，流就跑不完。
            // 否则 CI 上测试线程被调度走几秒，流可能自己结束，stop_all_streams() 会命中 0。
            for index in 0..5_000 {
                let delta = format!(
                    "data: {{\"choices\":[{{\"delta\":{{\"content\":\"{index},\"}}}}]}}\n\n"
                );
                if socket.write_all(delta.as_bytes()).is_err() {
                    break; // 客户端断开（被停止）
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        });

        struct Recorder(Mutex<Vec<Item>>);
        impl Emitter for Recorder {
            fn emit(&self, event: CommandEvent) {
                let CommandEvent::ItemUpdated { item, .. } = event;
                self.0.lock().unwrap().push(item);
            }
        }
        let recorder = Arc::new(Recorder(Mutex::new(Vec::new())));
        let emitter: Arc<dyn Emitter> = recorder.clone();

        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let base_url = format!("http://{addr}/v1");
        std::thread::spawn(move || {
            run_stream(
                &base_url,
                "test-key",
                serde_json::json!({}),
                "ai.quick-ask",
                // 不存在的会话：落库是 no-op（db 层的悬空行防护），不会污染真实历史
                Some("stop-e2e".into()),
                emitter,
            );
            let _ = done_tx.send(());
        });

        let deadline = Instant::now() + Duration::from_secs(5);
        while recorder.0.lock().unwrap().is_empty() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!recorder.0.lock().unwrap().is_empty(), "应先收到流式增量");
        assert!(stop_all_streams() >= 1, "应命中进行中的生成");
        done_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("停止后应立即收尾");

        let items = recorder.0.lock().unwrap();
        let last = items.last().expect("至少一帧");
        assert!(!last.pending, "末帧应为 pending=false");
        assert!(
            last.detail
                .as_deref()
                .unwrap_or_default()
                .contains("已停止生成"),
            "末帧应标记已停止：{:?}",
            last.detail
        );
        assert!(items.len() < 400, "不应跑完整个流：{} 帧", items.len());
        let _ = server.join();
    }

    /// 回答 item 的 ⌘M（materialize）同样带回会话 id，侧栏定位到同一会话。
    #[test]
    fn answer_item_carries_conversation_id_on_materialize() {
        let item = answer_item("正文", Some("9"), false);
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

    /// 临时库（不碰真实数据目录）：AI 历史在测试里可写可查。
    fn temp_db(tag: &str) -> (Db, std::path::PathBuf) {
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("moe-{tag}-{nanos}.db"));
        (Db::open(&path).expect("temp db"), path)
    }

    /// 回答/引导是详情整屏（ADR-0013）：面板里它就是正文，不该跟列表分栏。
    #[test]
    fn ask_placeholder_declares_detail_layout() {
        let ActionResult::List { items, detail_full } = AiShell.start_ask("   ", None) else {
            panic!("expected list");
        };
        assert!(detail_full, "回答是详情整屏");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "ai.answer");
    }

    /// 历史搜索恒为列表形态：唯一一条命中也不整屏（用户要看到那一行）——ADR-0013。
    #[test]
    fn history_search_stays_list_view_even_with_single_hit() {
        let (db, path) = temp_db("history");
        let now = SystemTime::now();
        let id = db
            .create_conversation("ai", "解释闭包", now)
            .expect("create");
        db.append_message(&id, Role::Assistant, "闭包是……", &[], now)
            .expect("append");

        let ActionResult::List { items, detail_full } = AiShell::search_history_in(&db, "闭包")
        else {
            panic!("expected list");
        };
        assert!(!detail_full, "历史搜索是列表视图（左列表 + 右预览）");
        assert_eq!(items.len(), 1, "唯一一条命中也不整屏");
        assert_eq!(items[0].id, format!("ai.conversation.{id}"));
        assert!(items[0].detail.is_some(), "右侧预览 = 最后一条回答摘要");

        // 无命中：仍是列表形态（一条通知进右预览，列表不消失）
        let ActionResult::List { items, detail_full } = AiShell::search_history_in(&db, "不存在")
        else {
            panic!("expected list");
        };
        assert!(!detail_full);
        assert_eq!(items[0].title, "没有匹配的历史会话");

        drop(db);
        let _ = std::fs::remove_file(&path);
    }

    /// 通用入口守卫（ADR-0014）：⌘P/⌘N 换出的 id 必须可路由，
    /// 否则面板 invoke 会 NotFound（与 fallback 同一守卫）。
    #[test]
    fn declared_entry_points_are_routable() {
        let ext = AiShell;
        let commands = ext.commands();
        for entry in [ext.browse_command(), ext.new_command()] {
            let entry = entry.expect("AI 声明了 Browse 与 New");
            assert!(
                commands.iter().any(|c| c.id == entry.id),
                "入口 id 必须在 commands() 中可路由：{}",
                entry.id
            );
        }
        assert_eq!(ext.browse_command().unwrap().id, "ai.search-history");
        assert_eq!(ext.new_command().unwrap().id, "ai.new-chat");
    }

    /// New（⌘N）在面板里给一张详情整屏的空态卡（不动会话状态：一问一会话）。
    #[test]
    fn new_chat_returns_a_detail_notice() {
        let ActionResult::List { items, detail_full } = AiShell
            .invoke("ai.new-chat", None, None)
            .expect("ai.new-chat 可路由")
        else {
            panic!("expected list");
        };
        assert!(detail_full, "空态卡是详情整屏");
        assert_eq!(items[0].id, "ai.notice");
        assert!(items[0].actions.is_empty(), "空态卡没有可执行动作");
    }
}
