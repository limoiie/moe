//! AI 问答：OpenAI 兼容端点的真实流式回答（ADR-0005）。
//!
//! - 快捷问答：提问即建会话（Namespace `ai`），回答落库（IIE4AD-360）。
//! - 侧栏续聊：整段历史作为上下文；事件 command_id 约定 `ai.side`。
//! - 流式走 `CommandEvent::ItemUpdated`：invoke 立即返回占位 item，
//!   worker 线程收 SSE 并逐段 emit（就地在详情卡片/侧栏重渲）。

use std::io::{BufRead, BufReader};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

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

fn answer_item(detail: &str, conversation_id: Option<&str>) -> Item {
    Item {
        id: "ai.answer".into(),
        title: "AI 回答".into(),
        subtitle: None,
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
    }
}

/// Item payload 载会话 id，Apply/⌘M 时原样带回（UI 不解释内容）。
fn conversation_payload(conversation_id: Option<&str>) -> serde_json::Value {
    match conversation_id {
        Some(id) => serde_json::json!({ "conversationId": id }),
        None => serde_json::Value::Null,
    }
}

/// 历史/续聊列表项：Apply 即在侧栏打开该会话。
fn history_item(conversation: Conversation, now_unix: u64) -> Item {
    let payload = conversation_payload(Some(&conversation.id));
    Item {
        id: format!("ai.conversation.{}", conversation.id),
        title: conversation.title,
        subtitle: Some(relative_time(conversation.updated_unix, now_unix)),
        actions: vec![Action {
            id: "materialize".into(),
            title: "在侧栏打开".into(),
            kind: ActionKind::Primary,
            keybinding: Some("⏎".into()),
        }],
        payload,
        detail: None,
    }
}

/// 无动作的提示项（空历史、读取失败等）。
fn notice_item(title: &str, detail: &str) -> Item {
    Item {
        id: "ai.notice".into(),
        title: title.into(),
        subtitle: None,
        actions: vec![],
        payload: serde_json::Value::Null,
        detail: Some(detail.into()),
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

/// 请求 + SSE 循环：增量逐段 emit，结束后落库（有会话时）。
fn run_stream(
    base_url: &str,
    key: &str,
    body: serde_json::Value,
    command_id: &str,
    conversation_id: Option<String>,
    emitter: Arc<dyn Emitter>,
) {
    let emit = |text: String| {
        emitter.emit(CommandEvent::ItemUpdated {
            command_id: command_id.to_string(),
            item: answer_item(&text, conversation_id.as_deref()),
        });
    };
    let fail = |text: String| {
        persist_assistant(conversation_id.as_deref(), &text);
        emit(text);
    };

    let client = match reqwest::blocking::Client::builder().build() {
        Ok(client) => client,
        Err(err) => {
            fail(format!("## 请求失败\n\n无法初始化 HTTP 客户端：{err}"));
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
        return;
    }

    let mut acc = String::new();
    for line in BufReader::new(response).lines() {
        let Ok(line) = line else { break };
        match sse_delta(&line) {
            SseLine::Delta(delta) => {
                acc.push_str(&delta);
                emit(acc.clone());
            }
            SseLine::Done => break,
            SseLine::Ignore => {}
        }
    }
    if acc.is_empty() {
        fail("（端点没有返回内容）".into());
    } else {
        persist_assistant(conversation_id.as_deref(), &acc);
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
            return ActionResult::List {
                items: vec![answer_item("在输入框里写下问题，再按 Enter。", None)],
            };
        }
        let attachments: Vec<AttachmentRef> = paths
            .iter()
            .map(|path| attachment::ref_for_path(path))
            .collect();

        let config = MoeConfig::load();
        if !config.ai.configured() {
            return ActionResult::List {
                items: vec![answer_item(SETUP_MD, None)],
            };
        }
        let Some(key) = keychain::ai_api_key() else {
            return ActionResult::List {
                items: vec![answer_item(KEY_MISSING_MD, None)],
            };
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
        ActionResult::List {
            items: vec![answer_item("正在回答…", conversation_id.as_deref())],
        }
    }

    /// 历史搜索：列表随输入重跑（CommandMeta.live），标题模糊匹配、最近优先。
    fn search_history(&self, query: &str) -> ActionResult {
        let now = unix_now();
        let items = match Db::open_default() {
            Ok(db) => match db.conversations("ai", Some(query), HISTORY_LIMIT) {
                Ok(list) if list.is_empty() => vec![notice_item(
                    "没有匹配的历史会话",
                    "在命令盘直接输入问题，即可开始一次新对话。",
                )],
                Ok(list) => list
                    .into_iter()
                    .map(|conversation| history_item(conversation, now))
                    .collect(),
                Err(err) => vec![notice_item("读取历史失败", &err)],
            },
            Err(err) => vec![notice_item("读取历史失败", &err)],
        };
        ActionResult::List { items }
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
                input: InputKind::Query,
                live: false,
            },
            CommandMeta {
                id: "ai.search-history".into(),
                extension_id: "ai".into(),
                title: "AI: 搜索历史会话".into(),
                subtitle: Some("输入即筛选标题；Enter 在侧栏打开".into()),
                input: InputKind::Query,
                live: true,
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
            _ => Err(MoeError::NotFound),
        }
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use moe_core::contract::Extension;

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
        );
        assert_eq!(item.subtitle.as_deref(), Some("1 分钟前"));
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

    /// 回答 item 的 ⌘M（materialize）同样带回会话 id，侧栏定位到同一会话。
    #[test]
    fn answer_item_carries_conversation_id_on_materialize() {
        let item = answer_item("正文", Some("9"));
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
}
