//! Command 契约：输入种类 × Item 流输出，回写由平台统一执行（ADR-0006）。

use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InputKind {
    /// 直接执行，不依赖输入。
    None,
    /// 依赖 Input Bar 的查询文本。
    Query,
    /// 呼出时自动抓取 Selection；缺失时降级到光标模式。
    Selection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ActionKind {
    /// Enter 触发的主操作（Apply）。每个 Item 有且只有一个 Primary 动作。
    Primary,
    /// 具名替代操作，各有固定快捷键。
    Secondary,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Action {
    /// Extension 内的稳定标识，路由回 `run_item_action`。
    pub id: String,
    pub title: String,
    pub kind: ActionKind,
    /// 展示用快捷键（如 "⌥⏎"）。系统级键位由平台 Keymap 提供，此处只放扩展自定义的。
    pub keybinding: Option<String>,
}

/// `Item.pending` 的 serde 辅助：false 时不下发（旧读取方不受影响）。
fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    /// 图标语义名（平台图标集，如 Lucide 的 "sparkles"；ADR-0012）。UI 决定具体图形，未知名回退默认。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// 第一个动作即 Apply 语义；Show All Actions（⌘K）展开全部。
    pub actions: Vec<Action>,
    /// 对 UI 不透明，动作执行时原样送回 Extension。
    pub payload: serde_json::Value,
    /// 详情内容（Markdown，详情视图卡片渲染）；流式回答在此累积。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// 仍在产出中（流式占位）：平台的键位层在 Esc 时优先请求停止生成（IIE4AD-365）。
    #[serde(default, skip_serializing_if = "is_false")]
    pub pending: bool,
}

/// Apply 或任一动作执行后的结果。输出类型不封闭枚举——List 中的 Item
/// 可再次产出 WriteBack / List，可组合性代替枚举（ADR-0006）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ActionResult {
    /// 交平台经 TextTarget 回写：替换 Selection，或插入光标处。
    WriteBack {
        text: String,
    },
    List {
        items: Vec<Item>,
        /// 详情是否整屏：结果只有一条且它本身就是内容时为 true（AI 回答、通知）。
        /// 多条结果（如历史搜索）保持「左列表 + 右详情」，由 Extension 声明而非 UI 猜（ADR-0013）。
        #[serde(default, skip_serializing_if = "is_false")]
        detail_full: bool,
    },
    /// 触发 Materialize：该 Extension 的 Side View，携带开窗所需载荷。
    OpenSideView {
        payload: serde_json::Value,
    },
    Silent,
}

impl ActionResult {
    /// 列表视图：左列表 + 右详情（多条结果，如历史搜索、动作清单）。
    pub fn list(items: Vec<Item>) -> Self {
        Self::List {
            items,
            detail_full: false,
        }
    }

    /// 详情视图：只有一条结果且它本身即内容（AI 回答、系统通知），详情占满面板。
    pub fn detail(items: Vec<Item>) -> Self {
        Self::List {
            items,
            detail_full: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandMeta {
    /// 全局唯一，约定 `"{extension_id}.{command}"`。
    pub id: String,
    pub extension_id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    /// 图标语义名（ADR-0012）。缺省时 UI 用来源 Command 的图标，再回退默认图形。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub input: InputKind,
    /// Live 列表：Input Bar 变化即用新查询重跑本命令（如历史搜索）；
    /// false 时输入只用于命令盘检索（默认）。
    pub live: bool,
}

/// 平台通用入口（ADR-0014）：三个通用动作 Browse（⌘P）/ New（⌘N）
/// 的落地方式——平台定键位与路由，入口由 Extension 声明。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EntryKind {
    /// Browse（⌘P）：该 Extension 的记录列表（AI = 历史会话）。
    Browse,
    /// New（⌘N）：新建一条记录（AI = 新会话）。
    New,
}

#[derive(Debug)]
pub enum MoeError {
    /// 平台能力需要授权（macOS 辅助功能），调用方应展示内联引导。
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

/// 命令执行期间的增量事件（流式回答走 Item 语义：按 id 就地更新）。
/// 注意：枚举上的 `rename_all` 只改变体名；struct 变体字段需要 `rename_all_fields`。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum CommandEvent {
    ItemUpdated { command_id: String, item: Item },
}

/// 增量事件出口。worker 线程也会发事件，因此要求线程安全。
pub trait Emitter: Send + Sync {
    fn emit(&self, event: CommandEvent);
}

/// 无事件出口（非流式调用方）。
pub struct NoopEmitter;

impl Emitter for NoopEmitter {
    fn emit(&self, _event: CommandEvent) {}
}

pub trait Extension: Send + Sync {
    /// Namespace 键（ADR-0003）：存储与历史都隔离在它之下。
    fn id(&self) -> &str;
    fn title(&self) -> &str;
    fn commands(&self) -> Vec<CommandMeta>;

    /// 命令盘里 Apply 一个 Command。
    fn invoke(
        &self,
        command_id: &str,
        query: Option<&str>,
        selection: Option<&str>,
    ) -> Result<ActionResult, MoeError>;

    /// 带增量事件的执行入口；默认回落 [`Extension::invoke`]（非流式扩展不需实现）。
    fn invoke_streaming(
        &self,
        command_id: &str,
        query: Option<&str>,
        selection: Option<&str>,
        _emitter: Arc<dyn Emitter>,
    ) -> Result<ActionResult, MoeError> {
        self.invoke(command_id, query, selection)
    }

    /// 搜索无匹配时提供的「捕获式」命令（如 AI: 提问「…」）；默认无。
    /// `selection` 是呼出面板前抓到的选区，供扩展在副标题里提示「已附上下文」等。
    fn fallback_command(&self, _query: &str, _selection: Option<&str>) -> Option<CommandMeta> {
        None
    }

    /// 「记录列表」入口（Browse，⌘P）：AI = 历史会话。默认无——
    /// 没有记录的 Extension 不声明，平台对该键位给一次内联提示（ADR-0014）。
    fn browse_command(&self) -> Option<CommandMeta> {
        None
    }

    /// 「新建记录」入口（New，⌘N）：AI = 新会话。默认无（同上）。
    fn new_command(&self) -> Option<CommandMeta> {
        None
    }

    /// Item 流的下一步：对 Item 执行其某个动作（默认无动作可执行）。
    fn run_item_action(
        &self,
        _command_id: &str,
        _item: &Item,
        _action: &Action,
    ) -> Result<ActionResult, MoeError> {
        Err(MoeError::NotFound)
    }

    /// Side View 续聊（ADR-0004）：空 id 表示新建会话；返回实际会话 id。
    /// 回复在后台流式产出，经 `CommandEvent` 增量送达（command_id 约定 `ai.side`）。
    fn side_continue(
        &self,
        _conversation_id: &str,
        _message: &str,
        _emitter: Arc<dyn Emitter>,
    ) -> Result<String, MoeError> {
        Err(MoeError::NotFound)
    }

    /// 请求停止本 Extension 正在进行的生成，返回被中止的生成数（默认无此能力）。
    /// 平台级 Esc 在 Focused Item 为 `pending` 时优先调用它（IIE4AD-365）。
    fn stop_generation(&self) -> usize {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 回归：枚举的 rename_all 只改变体名，struct 变体字段需 rename_all_fields；
    /// UI 按 `payload.itemUpdated.commandId` 读取（曾因 snake_case 静默丢更新）。
    #[test]
    fn command_event_serializes_camel_case_for_ui() {
        let event = CommandEvent::ItemUpdated {
            command_id: "ai.quick-ask".into(),
            item: Item {
                id: "ai.answer".into(),
                title: "AI 回答".into(),
                subtitle: None,
                actions: vec![],
                payload: serde_json::Value::Null,
                detail: None,
                pending: false,
                icon: None,
            },
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["itemUpdated"]["commandId"], "ai.quick-ask");
        assert_eq!(json["itemUpdated"]["item"]["id"], "ai.answer");
    }

    fn answer() -> Item {
        Item {
            id: "ai.answer".into(),
            title: "AI 回答".into(),
            subtitle: None,
            icon: None,
            actions: vec![],
            payload: serde_json::Value::Null,
            detail: Some("正文".into()),
            pending: false,
        }
    }

    /// 视图形态由 Extension 声明（ADR-0013）：UI 按 `list.detailFull` 决定
    /// 详情整屏还是「左列表 + 右详情」，不再用 items 条数猜。
    #[test]
    fn action_result_declares_view_layout() {
        let detail = serde_json::to_value(ActionResult::detail(vec![answer()])).unwrap();
        assert_eq!(detail["list"]["detailFull"], true);

        let list = serde_json::to_value(ActionResult::list(vec![answer()])).unwrap();
        // 列表视图省略该字段（旧的 list 结果反序列化后仍是列表，向后兼容）
        assert!(list["list"].get("detailFull").is_none());
        let legacy: ActionResult = serde_json::from_value(list).unwrap();
        assert_eq!(legacy, ActionResult::list(vec![answer()]));
    }
}
