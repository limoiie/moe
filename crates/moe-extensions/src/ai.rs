//! AI 问答：OpenAI 兼容端点的真实流式回答（ADR-0005）。
//!
//! 流式走 `CommandEvent::ItemUpdated`：invoke 立即返回占位 item，
//! worker 线程收 SSE 并逐段 emit（就地在详情卡片重渲）。

use std::io::{BufRead, BufReader};
use std::sync::Arc;

use moe_core::contract::{
    Action, ActionKind, ActionResult, CommandEvent, CommandMeta, Emitter, Extension, InputKind,
    Item, MoeError,
};
use moe_platform::config::MoeConfig;
use moe_platform::keychain;

use crate::ai_client::{SseLine, chat_body, chat_completions_url, sse_delta};

pub struct AiShell;

fn answer_item(detail: &str) -> Item {
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
        payload: serde_json::Value::Null,
        detail: Some(detail.into()),
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

/// 在 worker 线程里把问题和 SSE 流跑完，逐段更新回答 item。
fn stream_chat(base_url: &str, model: &str, key: &str, question: &str, emitter: Arc<dyn Emitter>) {
    let emit = |text: String| {
        emitter.emit(CommandEvent::ItemUpdated {
            command_id: "ai.quick-ask".into(),
            item: answer_item(&text),
        });
    };

    let client = match reqwest::blocking::Client::builder().build() {
        Ok(client) => client,
        Err(err) => {
            emit(format!("## 请求失败\n\n无法初始化 HTTP 客户端：{err}"));
            return;
        }
    };
    let response = client
        .post(chat_completions_url(base_url))
        .bearer_auth(key)
        .json(&chat_body(model, question))
        .send();
    let response = match response {
        Ok(response) => response,
        Err(err) => {
            emit(format!("## 请求失败\n\n```\n{err}\n```"));
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
        emit(format!("## 端点返回 {status}\n\n```\n{body}\n```"));
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
        emit("（端点没有返回内容）".into());
    }
}

impl AiShell {
    fn start_ask(&self, question: &str, emitter: Option<Arc<dyn Emitter>>) -> ActionResult {
        let question = question.trim().to_string();
        if question.is_empty() {
            return ActionResult::List {
                items: vec![answer_item("在输入框里写下问题，再按 Enter。")],
            };
        }

        let config = MoeConfig::load();
        if !config.ai.configured() {
            return ActionResult::List {
                items: vec![answer_item(SETUP_MD)],
            };
        }
        let Some(key) = keychain::ai_api_key() else {
            return ActionResult::List {
                items: vec![answer_item(KEY_MISSING_MD)],
            };
        };
        let base_url = config.ai.base_url.clone().unwrap_or_default();
        let model = config.ai.model_or_default().to_string();

        if let Some(emitter) = emitter {
            let question = question.clone();
            std::thread::spawn(move || stream_chat(&base_url, &model, &key, &question, emitter));
        }
        ActionResult::List {
            items: vec![answer_item("正在回答…")],
        }
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
            },
            CommandMeta {
                id: "ai.search-history".into(),
                extension_id: "ai".into(),
                title: "AI: 搜索历史会话".into(),
                subtitle: Some("M3b 实现（Namespace: ai）".into()),
                input: InputKind::Query,
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
            "ai.search-history" => Ok(ActionResult::Silent),
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
        Some(CommandMeta {
            id: "ai.quick-ask".into(),
            extension_id: "ai".into(),
            title: format!("AI: 提问「{query}」"),
            subtitle: Some("Enter 发送；回答可回写（⌥⏎ 复制 · ⌘M 侧栏）".into()),
            input: InputKind::Query,
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
            // 真剪贴板写入接 moe-platform 后替换（M3a 后续）。
            "copy" => Ok(ActionResult::Silent),
            "materialize" => Ok(ActionResult::OpenSideView),
            _ => Err(MoeError::NotFound),
        }
    }
}
