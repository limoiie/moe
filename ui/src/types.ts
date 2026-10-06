// 契约镜像：与 `moe-core` 的 serde camelCase 形状一一对应（ADR-0006）。
// 命令盘（index.html）与侧栏（chat.html）共用这一份，避免两个窗口各自漂移。

export type ActionKind = "primary" | "secondary";

export interface Action {
  id: string;
  title: string;
  kind: ActionKind;
  keybinding?: string | null;
}

export interface Item {
  id: string;
  title: string;
  subtitle?: string;
  /** 图标语义名（ADR-0012）；缺省时用来源 Command 的图标。 */
  icon?: string;
  /** 第一个动作即 Apply 语义；⌘K 展开全部。 */
  actions: Action[];
  payload: unknown;
  /** 详情内容（Markdown）；两栏页面把它渲染在右侧（ADR-0018）。 */
  detail?: string | null;
  /** 仍在产出中（流式占位）：Esc 时优先请求停止生成。 */
  pending?: boolean;
}

export interface CommandMeta {
  id: string;
  extensionId: string;
  title: string;
  subtitle?: string;
  /** 图标语义名（ADR-0012）。 */
  icon?: string;
  input: "none" | "query" | "selection";
  /** Live 列表：进入后输入变化即重跑（如历史搜索）。 */
  live: boolean;
}

// 外部 tagged 枚举：单位变体（silent）序列化为裸字符串；
// openSideView/writeBack 等带载荷变体为对象（侧栏开窗由后端执行）。
export type ActionResult =
  | string
  | { writeBack: { text: string } }
  | { list: { items: Item[]; detailFull?: boolean } }
  | { openSideView: { payload: unknown } };

/** 流式增量：按 item id 就地更新。 */
export interface CommandEventPayload {
  itemUpdated?: { commandId: string; item: Item };
}

// ---- AI 会话（`ai` Namespace）----

export interface AttachmentRef {
  name: string;
  path: string;
}

export interface Message {
  role: "user" | "assistant";
  content: string;
  attachments?: AttachmentRef[];
}

export interface Conversation {
  id: string;
  namespace: string;
  title: string;
  updatedUnix: number;
}

/** 面板 ⌘M / tray 带会话进侧栏的载荷（空 = 新对话）。 */
export interface SideOpenPayload {
  conversationId?: string | null;
}
