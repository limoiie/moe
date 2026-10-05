//! AI 问答：mock 流式回答（真实 OpenAI 兼容客户端是 M3a 的下一步）。
//!
//! 流式走 `CommandEvent::ItemUpdated`：invoke 立即返回占位 item，
//! worker 线程逐字 emit 就地更新（ADR-0006 的流式延伸）。

use std::sync::Arc;
use std::time::Duration;

use moe_core::contract::{
    Action, ActionKind, ActionResult, CommandEvent, CommandMeta, Emitter, Extension, InputKind,
    Item, MoeError,
};

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

impl AiShell {
    fn start_ask(&self, question: &str, emitter: Option<Arc<dyn Emitter>>) -> ActionResult {
        let question = question.trim().to_string();
        if question.is_empty() {
            return ActionResult::List {
                items: vec![answer_item("在输入框里写下问题，再按 Enter。")],
            };
        }
        if let Some(emitter) = emitter {
            std::thread::spawn(move || {
                let full = format!(
                    "## 对「{question}」的 mock 回答\n\n\
                     这是**结构化**的演示输出（真实模型接入后内容由模型生成）：\n\n\
                     - 第一条要点\n\
                     - 第二条要点，带 `行内代码`\n\
                     - 第三条要点\n\n\
                     ```rust\n\
                     fn main() {{\n\
                         println!(\"hello from Moe\");\n\
                     }}\n\
                     ```\n\n\
                     > 引用块：回答可回写、可复制、可实体化为侧栏。\n"
                );
                let mut acc = String::new();
                for ch in full.chars() {
                    acc.push(ch);
                    std::thread::sleep(Duration::from_millis(25));
                    emitter.emit(CommandEvent::ItemUpdated {
                        command_id: "ai.quick-ask".into(),
                        item: answer_item(&acc),
                    });
                }
            });
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
