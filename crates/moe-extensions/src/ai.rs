//! AI 问答：OpenAI 兼容端点的真实流式回答（ADR-0005）。
//!
//! - 快捷问答：提问即建会话（Namespace `ai`），回答落库（IIE4AD-360）。
//! - 侧栏续聊：整段历史作为上下文；事件 command_id 约定 `ai.side`。
//! - 流式走 `CommandEvent::ItemUpdated`：invoke 立即返回占位 item，
//!   worker 线程收 SSE 并逐段 emit（就地在详情卡片/侧栏重渲）。

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

/// 进行中的生成：登记键 → 取消标记（IIE4AD-365）。
/// 键在聊天流 = 会话 id；AI 命令流 = 每次运行的唯一键（ADR-0024）。
fn active_streams() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    static ACTIVE: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();
    ACTIVE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 登记一次生成（空键不登记、不可中断，返回 None）。
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

/// 收尾：从登记表移除（worker 结束/被中止时调用）。
fn end_stream(key: &str) {
    if key.is_empty() {
        return;
    }
    active_streams()
        .lock()
        .expect("streams poisoned")
        .remove(key);
}

/// 标记全部进行中的生成为取消；worker 在下一个 SSE 行处收尾。返回中止数量。
pub(crate) fn stop_all_streams() -> usize {
    let streams = active_streams().lock().expect("streams poisoned");
    for flag in streams.values() {
        flag.store(true, Ordering::Relaxed);
    }
    streams.len()
}

/// 帧构造器：全文 + pending → Item（聊天流 / AI 命令流各自的卡片形状）。
pub(crate) type FrameBuilder = Arc<dyn Fn(&str, bool) -> Item + Send + Sync>;
/// 流收尾钩子：全文 → ()（落库 / 自动回写回调）。
pub(crate) type StreamFinish = Arc<dyn Fn(&str) + Send + Sync>;

/// 一次流式请求的完整规格（聊天流与 AI 命令流共用，ADR-0024）。
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

/// 收尾钩子的空实现（不落库、无完成回调的流）。
pub(crate) fn noop_sink(_text: &str) {}

/// 请求 + SSE 循环：增量逐段 emit（pending=true）；自然完成时落库并回调 `on_done`。
/// 被停止（IIE4AD-365）时保留已生成部分并落库、标记已停止，**不**触发 `on_done`。
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
            fail(format!("## 请求失败\n\n无法初始化 HTTP 客户端：{err}"));
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
            fail(format!("## 请求失败\n\n```\n{err}\n```"));
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
        fail(format!("## 端点返回 {status}\n\n```\n{body}\n```"));
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
            "（已停止生成）".to_string()
        } else {
            format!("{acc}\n\n_（已停止生成）_")
        };
        persist(&text);
        emit(text, false);
    } else if acc.is_empty() {
        fail("（端点没有返回内容）".into());
    } else {
        persist(&acc);
        emit(acc.clone(), false);
        on_done(&acc);
    }
}

/// 未配置端点的引导卡（AI 问答与 AI 命令共用，ADR-0024）。
pub(crate) const SETUP_MD: &str = "## 还没有配置 AI 端点\n\n\
1. 运行命令「Moe: 打开配置文件」，填入：\n\n\
```toml\n\
[ai]\n\
base_url = \"https://api.deepseek.com/v1\"\n\
model = \"deepseek-chat\"\n\
```\n\n\
2. 存 API key：输入 `key <你的key>` 回车（只存本地密钥文件，不回显；也可设 `MOE_AI_API_KEY` 环境变量）。\n\n\
支持任意 OpenAI 兼容端点（DeepSeek / OpenRouter / 本地 llama.cpp 等）。";

/// 缺 API key 的引导卡（同上共用）。
pub(crate) const KEY_MISSING_MD: &str = "## 端点已配置，但还缺 API key\n\n\
输入 `key <你的key>` 回车即可存入本地密钥文件（0600，不回显）；也可设 `MOE_AI_API_KEY` 环境变量。";

/// 选中文字作为上下文时的长度上限（字符）。
const SELECTION_LIMIT: usize = 4000;

/// 把选中文字拼进问题（ADR-0019 增补：呼出面板前抓到的 Selection 自动作为提问上下文）。
/// 问题里已经包含这段文字时不再重复；超长截断并注明。
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
        "\n…（选中文字过长，已截断）"
    } else {
        ""
    };
    format!("{question}\n\n以下是用户选中的文字，作为回答的上下文：\n```\n{excerpt}{suffix}\n```")
}

/// 发给模型的请求体（抽出以便测试）：问题（可带选区上下文）+ 附件展开。
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
    /// 附件 = 输入里的 `@path` mention + 呼出前选中的文件（ADR-0021），按路径去重。
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

    /// 拆开一条输入：正文（clean）+ 附件引用。
    /// 纯附件（只有 `@path`、没有正文）时正文换成通用请求句——
    /// 不把原始 mention 发给模型或存进历史（IIE4AD-391）。
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
                "请结合这些附件回答。".to_string()
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
        // 附件以 `@path` mention 表达（ADR-0010）：内容现读，引用随消息落库。
        // 呼出前在 Finder 里选中的文件同样成为附件（ADR-0021），与 mention 按路径去重。
        let (cleaned, paths) = attachment::parse_mentions(&question);
        let selected_files: Vec<String> = selection.map(|s| s.files.clone()).unwrap_or_default();
        let attachments = Self::attachments_with(&paths, &selected_files);
        let text = selection.and_then(|s| s.text());
        // 纯附件（只有 @path）不把原始 mention 当问题：落到空问引导（与选中文件一致）。
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
                "在输入框里写下问题，再按 Enter。"
            } else {
                "在输入框里写下问题，再按 Enter（已附上附件）。"
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

        // 提问即建会话；随后 ⌘M 可带同一会话进侧栏续聊。
        // 历史只存问题本身（标题干净）；发给模型的内容带选中文字上下文。
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
        // 占位帧：正文留空——「正在生成」行内指示已经是唯一的状态反馈
        ActionResult::detail(vec![answer_item("", conversation_id.as_deref(), true)])
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

    /// 删除注入版（ADR-0022）：按 payload 的 conversationId 定位；测试不碰真实数据目录。
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
        selection: Option<&Selection>,
    ) -> Result<ActionResult, MoeError> {
        match command_id {
            "ai.quick-ask" => Ok(self.start_ask(query.unwrap_or_default(), selection, None)),
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

    /// 搜索无匹配时的「捕获式」命令（如 AI: 提问「…」）；
    /// 副标题注明上下文：选中文字 / 选中文件（Finder）各有提示。
    fn fallback_command(&self, query: &str, selection: Option<&Selection>) -> Option<CommandMeta> {
        // 标题剥离 mention（ADR-0010）：路径不当作提问内容展示。
        let (cleaned, paths) = attachment::parse_mentions(query);
        let selected_files = selection.map(|s| s.files.len()).unwrap_or(0);
        let title = match (cleaned.is_empty(), paths.is_empty(), selected_files) {
            (false, _, _) => format!("AI: 提问「{cleaned}」"),
            (true, false, _) | (true, true, 1..) => "AI: 带附件的提问".into(),
            (true, true, _) => format!("AI: 提问「{query}」"),
        };
        let total_files = paths.len() + selected_files;
        let has_text = selection
            .and_then(|s| s.text())
            .map(str::trim)
            .is_some_and(|t| !t.is_empty());
        let subtitle = match (total_files > 0, has_text) {
            (true, true) => format!("已附上 {total_files} 个附件与选中文字；Enter 发送"),
            (true, false) => format!("{total_files} 个附件；Enter 发送"),
            (false, true) => "已附上选中文字作为上下文；Enter 发送".to_string(),
            (false, false) => "Enter 发送；回答可回写（⌥⏎ 复制 · ⌘M 侧栏）".to_string(),
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

        // 附件 mention 与快捷提问同一语法（ADR-0010）；纯附件用通用请求句（IIE4AD-391）。
        let (display, attachments) = Self::split_message(message);

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

    /// 停止进行中的生成（IIE4AD-365）：标记取消位，worker 在下一个 SSE 行处收尾并落库已生成部分。
    fn stop_generation(&self) -> usize {
        stop_all_streams()
    }

    /// 删除当前记录（通用动作 Delete，⌃X，ADR-0022）：AI 历史里删一条会话。
    fn delete_item(&self, command_id: &str, item: &Item) -> Result<usize, MoeError> {
        if command_id != "ai.search-history" {
            return Err(MoeError::NotFound);
        }
        let db = Db::open_default().map_err(MoeError::Internal)?;
        Self::delete_item_in(&db, item)
    }

    /// 删除全部记录（通用动作 DeleteAll，⌃⇧X，ADR-0022）：清空 `ai` Namespace 全部会话。
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

/// `stop_all_streams` 作用于**全局**注册表：并行跑的两个流测试会互相取消对方的流
/// （一个测试的 stop 会把另一个测试刚登记的取消位一并标记），导致偶发失败。串行化。
/// pub(crate)：AI 命令（ai_commands.rs）的流测试共用同一把锁。
#[cfg(test)]
pub(crate) static STREAM_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;
    use moe_core::contract::Extension;
    use std::time::Instant;

    /// 同款守卫：fallback 合成的 id 必须可路由；标题剥离附件 mention（IIE4AD-358）。
    #[test]
    fn fallback_command_is_invocable_and_strips_mentions() {
        let ext = AiShell;
        let fallback = ext.fallback_command("hello", None).expect("fallback");
        assert!(
            ext.commands().iter().any(|c| c.id == fallback.id),
            "fallback id 必须在 commands() 中可路由：{}",
            fallback.id
        );

        let with_attachments = ext
            .fallback_command("总结 @\"/tmp/a b.md\"", None)
            .expect("fallback");
        assert_eq!(with_attachments.title, "AI: 提问「总结」");
        assert_eq!(
            with_attachments.subtitle.as_deref(),
            Some("1 个附件；Enter 发送")
        );

        let attachment_only = ext.fallback_command("@/tmp/a.md", None).expect("fallback");
        assert_eq!(attachment_only.title, "AI: 带附件的提问");
    }

    /// 选中文件（ADR-0021）：fallback 提示文件数，纯文件选择与文字选区可叠加。
    #[test]
    fn fallback_advertises_selected_files() {
        let ext = AiShell;
        let files = Selection {
            text: None,
            files: vec!["/tmp/a.md".into(), "/tmp/b.png".into()],
        };
        // 空输入 + 选中文件：带附件的提问入口
        let files_only = ext.fallback_command("", Some(&files)).expect("fallback");
        assert_eq!(files_only.title, "AI: 带附件的提问");
        assert_eq!(files_only.subtitle.as_deref(), Some("2 个附件；Enter 发送"));
        // 问题 + 选中文件：附件与选中文字提示叠加
        let both = ext
            .fallback_command(
                "总结",
                Some(&Selection {
                    text: Some("选中文字".into()),
                    files: files.files.clone(),
                }),
            )
            .expect("fallback");
        assert_eq!(both.title, "AI: 提问「总结」");
        assert_eq!(
            both.subtitle.as_deref(),
            Some("已附上 2 个附件与选中文字；Enter 发送")
        );
    }

    /// 附件合并（ADR-0021）：mention 与选中文件按路径去重，保序。
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

    /// 纯文件选择 + 空问题：不直接发请求，提示写下问题（文件已在手上）。
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
        assert!(detail.contains("已附上附件"), "{detail}");
    }

    /// 纯 mention（只有 @path、无正文）不把原始路径发给模型：落到空问引导（IIE4AD-391）。
    #[test]
    fn mention_only_ask_guides_instead_of_sending_raw_path() {
        let ActionResult::List { items, .. } = AiShell.start_ask("@/tmp/a.md", None, None) else {
            panic!("expected list");
        };
        let detail = items[0].detail.as_deref().unwrap_or_default();
        assert!(detail.contains("已附上附件"), "{detail}");
        assert!(
            !detail.contains("/tmp/a.md"),
            "原始路径不应出现在提示里：{detail}"
        );
    }

    /// split_message：有正文保正文；纯附件换通用请求句，不泄露原始 mention。
    #[test]
    fn split_message_swaps_raw_mention_for_prompt() {
        let (display, attachments) = AiShell::split_message("总结 @\"/tmp/a b.md\"");
        assert_eq!(display, "总结");
        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].path, "/tmp/a b.md");

        let (display, attachments) = AiShell::split_message("@/tmp/a.md @/tmp/b.png");
        assert_eq!(display, "请结合这些附件回答。");
        assert_eq!(attachments.len(), 2);

        // 无附件无正文：原样（空消息由上层拦）
        let (display, attachments) = AiShell::split_message("   ");
        assert_eq!(display, "   ");
        assert!(attachments.is_empty());
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
        let _guard = STREAM_TEST_LOCK.lock().expect("stream test lock");
        let flag = begin_stream("test-stop-conv").expect("登记取消位");
        assert!(!flag.load(Ordering::Relaxed));
        assert!(stop_all_streams() >= 1, "应至少命中刚登记的取消位");
        assert!(flag.load(Ordering::Relaxed), "取消位应被标记");
        end_stream("test-stop-conv");
        // 空键不登记、不可中断
        assert!(begin_stream("").is_none());
    }

    /// 端到端：慢速 SSE 流中途 `stop_generation` → 及时收尾，末帧 pending=false 且标记已停止。
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
            // 不存在的会话：落库是 no-op（db 层的悬空行防护），不会污染真实历史
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

    /// 删除槽（ADR-0022）：单条删除按 payload 的 conversationId 定位；缺 id NotFound。
    #[test]
    fn delete_item_removes_the_conversation() {
        let (db, path) = temp_db("delete");
        let a = db
            .create_conversation("ai", "会话 A", SystemTime::now())
            .unwrap();
        db.create_conversation("ai", "会话 B", SystemTime::now())
            .unwrap();
        let item = history_item(
            Conversation {
                id: a.clone(),
                namespace: "ai".into(),
                title: "会话 A".into(),
                updated_unix: 1,
            },
            1,
            None,
        );
        assert_eq!(AiShell::delete_item_in(&db, &item).unwrap(), 1);
        assert_eq!(db.conversations("ai", None, 10).unwrap().len(), 1);
        // 缺 conversationId：NotFound，不误删
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
        // 不支持的命令：NotFound（命令层不外露删除）
        assert!(matches!(
            AiShell.delete_item("ai.quick-ask", &item),
            Err(MoeError::NotFound)
        ));
        let _ = std::fs::remove_file(&path);
    }

    /// 删除全部（ADR-0022）：只清 `ai` Namespace，返回条数。
    #[test]
    fn delete_all_clears_ai_namespace() {
        let (db, path) = temp_db("delete-all");
        db.create_conversation("ai", "一", SystemTime::now())
            .unwrap();
        db.create_conversation("ai", "二", SystemTime::now())
            .unwrap();
        db.create_conversation("other", "别的扩展", SystemTime::now())
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

    /// 回答/引导是详情整屏（ADR-0013）：面板里它就是正文，不该跟列表分栏。
    #[test]
    fn ask_placeholder_declares_detail_layout() {
        let ActionResult::List { items, detail_full } = AiShell.start_ask("   ", None, None) else {
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

    /// 请求体组装：选区和问题都进 body（selection_becomes_question_context 只测拼接，这里测整条链路）。
    #[test]
    fn ask_body_carries_question_and_selection() {
        let body = ask_body("m", "翻译", Some("hello world"), &[]);
        let text = body.to_string();
        assert!(text.contains("翻译"), "问题在 body：{text}");
        assert!(text.contains("hello world"), "选区在 body：{text}");
        // 无选区时没有上下文包装
        let plain = ask_body("m", "翻译", None, &[]).to_string();
        assert!(!plain.contains("选中的文字"), "无选区不加包装：{plain}");
    }

    /// 选中文字自动成为提问上下文（ADR-0002 增补）：拼接格式稳定，重复/空白/超长有边界。
    #[test]
    fn selection_becomes_question_context() {
        let prompt = question_with_selection("翻译", Some("  hello world  "));
        assert!(prompt.starts_with("翻译"), "问题在前：{prompt}");
        assert!(prompt.contains("hello world"), "选区应拼进提示：{prompt}");

        // 问题里已含这段文字：不重复拼接
        assert_eq!(
            question_with_selection("解释 hello world", Some("hello world")),
            "解释 hello world"
        );
        // 空白/无选区：原样返回
        assert_eq!(question_with_selection("翻译", Some("   ")), "翻译");
        assert_eq!(question_with_selection("翻译", None), "翻译");

        // 超长截断并注明，不无限占用上下文
        let long: String = "x".repeat(5_000);
        let capped = question_with_selection("翻译", Some(&long));
        assert!(capped.contains("已截断"), "超长选区要注明截断：{capped}");
        assert!(
            capped.matches('x').count() <= SELECTION_LIMIT,
            "截断后不超过上限"
        );
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
