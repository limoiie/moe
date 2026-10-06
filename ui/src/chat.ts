import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import DOMPurify from "dompurify";
import { marked } from "marked";
import "./styles.css";
import { appendMention, validatePath } from "./attachment";
import { generatingEl } from "./generating";
import { iconEl } from "./icons";
import { GENERAL_KEY_LABELS, generalActionOf } from "./keymap";

// ---- 类型：镜像 Rust 契约（ADR-0006）----

interface AttachmentRef {
  name: string;
  path: string;
}
interface Message {
  role: "user" | "assistant";
  content: string;
  attachments?: AttachmentRef[];
}
interface Conversation {
  id: string;
  namespace: string;
  title: string;
  updatedUnix: number;
}
interface Item {
  id: string;
  title: string;
  subtitle?: string;
  icon?: string;
  payload: unknown;
  detail?: string | null;
  /** 仍在产出中（流式占位）。 */
  pending?: boolean;
}
interface CommandEventPayload {
  itemUpdated?: { commandId: string; item: Item };
}
interface SideOpenPayload {
  conversationId?: string | null;
}

/** 侧栏续聊事件约定（moe-extensions::ai::SIDE_COMMAND_ID）。 */
const SIDE_COMMAND_ID = "ai.side";

const chatIconEl = document.querySelector<HTMLSpanElement>("#chat-icon")!;
const titleEl = document.querySelector<HTMLSpanElement>("#chat-title")!;
const actionsButtonEl = document.querySelector<HTMLButtonElement>("#actions-button")!;
const historyButtonEl = document.querySelector<HTMLButtonElement>("#history-button")!;
const newChatEl = document.querySelector<HTMLButtonElement>("#new-chat")!;
const closeEl = document.querySelector<HTMLButtonElement>("#close")!;
const historyCardEl = document.querySelector<HTMLDivElement>("#history-card")!;
const historySearchEl = document.querySelector<HTMLInputElement>("#history-search")!;
const historyListEl = document.querySelector<HTMLUListElement>("#history-list")!;
const actionsMenuEl = document.querySelector<HTMLDivElement>("#actions-menu")!;
const messagesEl = document.querySelector<HTMLElement>("#messages")!;
const emptyEl = document.querySelector<HTMLDivElement>("#empty")!;
const errorEl = document.querySelector<HTMLDivElement>("#error")!;
const composerEl = document.querySelector<HTMLTextAreaElement>("#composer")!;
const sendEl = document.querySelector<HTMLButtonElement>("#send")!;
const attachEl = document.querySelector<HTMLButtonElement>("#attach")!;
const attachRowEl = document.querySelector<HTMLDivElement>("#attach-row")!;
const attachPathEl = document.querySelector<HTMLInputElement>("#attach-path")!;
const attachMsgEl = document.querySelector<HTMLSpanElement>("#attach-msg")!;
const toastEl = document.querySelector<HTMLDivElement>("#toast")!;
const toastIconEl = document.querySelector<HTMLSpanElement>("#toast-icon")!;
const toastTextEl = document.querySelector<HTMLSpanElement>("#toast-text")!;

/** 当前会话；null = 空状态（首次发送会自动新建）。 */
let conversationId: string | null = null;
/** 正在流式更新的回答气泡（首次事件时才建）。 */
let streaming: StreamingBubble | null = null;
let sending = false;
/** 后端仍在生成（收到 pending=false 或主动停止后结束）。 */
let generating = false;
/** 最近一次加载的会话列表（历史卡与 ⌃[/⌃] 步进共用）。 */
let conversations: Conversation[] = [];

interface StreamingBubble {
  root: HTMLElement;
  body: HTMLElement;
  indicator: HTMLElement;
}

// ---- 固定图标（三个头部按钮靠右：更多操作 / 历史 / 新建对话）----

chatIconEl.replaceChildren(iconEl("sparkles", { size: 15 }));
actionsButtonEl.replaceChildren(iconEl("command", { size: 16 }));
historyButtonEl.replaceChildren(iconEl("history", { size: 16 }));
newChatEl.replaceChildren(iconEl("plus", { size: 16 }));
closeEl.replaceChildren(iconEl("close", { size: 16 }));
attachEl.replaceChildren(iconEl("paperclip", { size: 14 }), document.createTextNode("附件"));
// 三个通用动作的 tooltip 也取自共享键位（ADR-0014），避免与键位表漂移
actionsButtonEl.title = `更多操作（${GENERAL_KEY_LABELS.actions}）`;
historyButtonEl.title = `历史会话（${GENERAL_KEY_LABELS.browse}）`;
newChatEl.title = `新对话（${GENERAL_KEY_LABELS.new}）`;

// ---- 轻量 toast（菜单动作的即时反馈，如「已打开配置文件」）----

let toastTimer: ReturnType<typeof setTimeout> | undefined;

function toast(text: string, icon = "check") {
  toastIconEl.replaceChildren(iconEl(icon, { size: 12 }));
  toastTextEl.textContent = text;
  toastEl.classList.remove("hidden");
  toastEl.classList.add("flex");
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => {
    toastEl.classList.add("hidden");
    toastEl.classList.remove("flex");
  }, 1600);
}

// ---- 历史会话：顶部居中的悬浮卡（点击历史按钮 / ⌘P 展开）----

let historyOpen = false;
let historyIndex = 0;
let filterTimer: ReturnType<typeof setTimeout> | undefined;

function openHistoryCard() {
  historyOpen = true;
  historySearchEl.value = "";
  historyIndex = 0;
  historyCardEl.classList.remove("hidden");
  historySearchEl.focus();
  void loadConversations();
}

function closeHistoryCard() {
  if (!historyOpen) return;
  historyOpen = false;
  historyCardEl.classList.add("hidden");
}

function toggleHistoryCard() {
  if (historyOpen) closeHistoryCard();
  else openHistoryCard();
}

function rowClass(highlighted: boolean): string {
  return (
    "flex cursor-default items-center gap-2 rounded-lg px-2 py-1.5 text-xs " +
    (highlighted ? "bg-zinc-700/70 text-zinc-50" : "text-zinc-300 hover:bg-zinc-800/70")
  );
}

function paintHistoryHighlight() {
  for (const child of Array.from(historyListEl.children)) {
    const on = Number((child as HTMLElement).dataset.index) === historyIndex;
    child.className = rowClass(on);
  }
}

function moveHistory(delta: number) {
  if (conversations.length === 0) return;
  historyIndex = Math.min(Math.max(historyIndex + delta, 0), conversations.length - 1);
  paintHistoryHighlight();
  historyListEl.children[historyIndex]?.scrollIntoView({ block: "nearest" });
}

function conversationRow(conversation: Conversation, index: number): HTMLLIElement {
  const li = document.createElement("li");
  li.dataset.index = String(index);
  li.className = rowClass(false);
  li.append(iconEl("message-square", { size: 14, className: "text-zinc-500" }));
  const label = document.createElement("span");
  label.className = "min-w-0 flex-1 truncate";
  label.textContent = conversation.title;
  label.title = conversation.title;
  li.append(label);
  const time = document.createElement("span");
  if (conversation.id === conversationId) {
    time.className = "shrink-0 text-[10px] text-sky-400/80";
    time.textContent = "当前";
  } else {
    time.className = "shrink-0 text-[10px] text-zinc-500";
    time.textContent = relativeTime(conversation.updatedUnix);
  }
  li.append(time);
  li.addEventListener("mousemove", () => {
    if (historyIndex !== index) {
      historyIndex = index;
      paintHistoryHighlight();
    }
  });
  li.addEventListener("click", () => {
    void selectConversation(conversation.id);
    closeHistoryCard();
  });
  return li;
}

function emptyRow(text: string): HTMLLIElement {
  const li = document.createElement("li");
  li.className = "px-2 py-3 text-xs text-zinc-600";
  li.textContent = text;
  return li;
}

function renderConversations() {
  if (conversations.length === 0) {
    const query = historySearchEl.value.trim();
    historyListEl.replaceChildren(
      emptyRow(query ? "没有匹配的会话" : "还没有历史会话：在下方输入即可开始新对话"),
    );
    return;
  }
  historyIndex = Math.min(historyIndex, conversations.length - 1);
  historyListEl.replaceChildren(
    ...conversations.map((conversation, index) => conversationRow(conversation, index)),
  );
  paintHistoryHighlight();
}

async function loadConversations() {
  const query = historySearchEl.value.trim();
  try {
    conversations = await invoke<Conversation[]>("side_conversations", {
      query: query || null,
    });
  } catch (err) {
    setError(`读取历史失败：${String(err)}`);
    return;
  }
  renderConversations();
}

historyButtonEl.addEventListener("click", () => {
  closeActionsMenu();
  toggleHistoryCard();
});

historySearchEl.addEventListener("input", () => {
  clearTimeout(filterTimer);
  filterTimer = setTimeout(() => void loadConversations(), 80);
});

historySearchEl.addEventListener("keydown", (e) => {
  if (e.key === "ArrowDown" || e.key === "ArrowUp") {
    e.preventDefault();
    moveHistory(e.key === "ArrowDown" ? 1 : -1);
  } else if (e.key === "Enter" && !e.isComposing) {
    e.preventDefault();
    const target = conversations[historyIndex];
    if (target) {
      void selectConversation(target.id);
      closeHistoryCard();
    }
  } else if (e.key === "Escape") {
    e.preventDefault();
    e.stopPropagation();
    closeHistoryCard();
    composerEl.focus();
  }
});

// ---- 更多操作（⌘ 图标按钮）：菜单式，点击外部 / Esc 收起 ----

let actionsOpen = false;

interface MenuAction {
  label: string;
  icon: string;
  shortcut?: string;
  run: () => void;
}

function openConfig() {
  void invoke("invoke_command", {
    commandId: "moe.open-config",
    query: null,
    record: false,
  }).then(
    () => toast("已打开配置文件"),
    (err) => setError(String(err)),
  );
}

function menuActions(): MenuAction[] {
  return [
    { label: "新对话", icon: "plus", shortcut: "⌘N", run: newChat },
    { label: "打开配置文件", icon: "settings-2", run: openConfig },
    { label: "收起侧栏", icon: "close", shortcut: "Esc", run: () => void getCurrentWindow().hide() },
  ];
}

function renderActionsMenu() {
  actionsMenuEl.replaceChildren(
    ...menuActions().map((action) => {
      const button = document.createElement("button");
      button.className =
        "flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left text-xs text-zinc-300 hover:bg-zinc-700/70 hover:text-zinc-50";
      button.append(iconEl(action.icon, { size: 14, className: "text-zinc-500" }));
      const label = document.createElement("span");
      label.className = "min-w-0 flex-1 truncate";
      label.textContent = action.label;
      button.append(label);
      if (action.shortcut) {
        const kbd = document.createElement("span");
        kbd.className = "shrink-0 text-[10px] text-zinc-500";
        kbd.textContent = action.shortcut;
        button.append(kbd);
      }
      button.addEventListener("click", () => {
        closeActionsMenu();
        action.run();
      });
      return button;
    }),
  );
}

function openActionsMenu() {
  closeHistoryCard();
  actionsOpen = true;
  renderActionsMenu();
  actionsMenuEl.classList.remove("hidden");
}

function closeActionsMenu() {
  if (!actionsOpen) return;
  actionsOpen = false;
  actionsMenuEl.classList.add("hidden");
}

actionsButtonEl.addEventListener("click", () => {
  if (actionsOpen) closeActionsMenu();
  else openActionsMenu();
});

// 点击浮层之外收起（按钮自己的 click 已处理，这里排除两张浮层与两个触发按钮）
document.addEventListener("mousedown", (e) => {
  const target = e.target as Node;
  if (historyOpen && !historyCardEl.contains(target) && !historyButtonEl.contains(target)) {
    closeHistoryCard();
  }
  if (actionsOpen && !actionsMenuEl.contains(target) && !actionsButtonEl.contains(target)) {
    closeActionsMenu();
  }
});

// ---- 会话切换 ----

async function stepConversation(delta: number) {
  if (conversations.length === 0) return;
  const index = conversations.findIndex((c) => c.id === conversationId);
  const next =
    index === -1
      ? delta > 0
        ? 0
        : conversations.length - 1
      : Math.min(Math.max(index + delta, 0), conversations.length - 1);
  const target = conversations[next];
  if (!target || target.id === conversationId) return;
  await selectConversation(target.id);
}

async function selectConversation(id: string) {
  if (id === conversationId) return;
  conversationId = id;
  setError(null);
  await loadHistory();
  void loadConversations();
}

function newChat() {
  conversationId = null;
  streaming = null;
  generating = false;
  updateSendUi();
  setError(null);
  void loadHistory();
  void loadConversations();
  composerEl.focus();
}

newChatEl.addEventListener("click", newChat);

// ---- 消息区 ----

function relativeTime(updatedUnix: number): string {
  const secs = Math.max(0, Math.floor(Date.now() / 1000) - updatedUnix);
  if (secs < 60) return "刚刚";
  if (secs < 3600) return `${Math.floor(secs / 60)} 分钟前`;
  if (secs < 86400) return `${Math.floor(secs / 3600)} 小时前`;
  return `${Math.floor(secs / 86400)} 天前`;
}

function setError(text: string | null) {
  errorEl.classList.toggle("hidden", !text);
  errorEl.textContent = text ?? "";
}

function markdown(text: string): string {
  return DOMPurify.sanitize(marked.parse(text, { async: false }));
}

/** 标题：取第一条用户消息首行（与历史列表的会话标题规则一致）。 */
function titleFrom(messages: Message[]): string | null {
  const first = messages.find((message) => message.role === "user" && message.content.trim());
  if (!first) return null;
  const line = first.content.trim().split("\n")[0];
  return line.length > 24 ? `${line.slice(0, 24)}…` : line;
}

function setTitle(text: string) {
  titleEl.textContent = text;
}

function scrollToBottom() {
  messagesEl.scrollTop = messagesEl.scrollHeight;
}

/**
 * 滚到最后一行的底部：markdown/代码块/图片是异步撑开的，
 * 单次赋值可能没到底，用 rAF + 短延时各校正一次（IIE4AD-369）。
 */
function scrollToEnd() {
  scrollToBottom();
  requestAnimationFrame(scrollToBottom);
  setTimeout(scrollToBottom, 120);
}

/** 追加一条已完成的本地消息（用户气泡 / 历史里的回答）。 */
function appendMessage(message: Message): HTMLElement {
  const row = document.createElement("div");
  if (message.role === "user") {
    row.className = "flex flex-col items-end gap-1";
    const card = document.createElement("div");
    card.className =
      "max-w-[85%] whitespace-pre-wrap rounded-2xl rounded-br-sm bg-sky-600/80 px-3 py-2 text-sm text-white";
    card.textContent = message.content;
    row.append(card);
    if (message.attachments?.length) {
      const chips = document.createElement("div");
      chips.className = "flex max-w-[85%] flex-wrap justify-end gap-1";
      for (const reference of message.attachments) {
        const chip = document.createElement("span");
        chip.className =
          "max-w-full truncate rounded-full border border-zinc-600/60 bg-zinc-800/70 px-2 py-0.5 text-[11px] text-zinc-300";
        chip.textContent = `📎 ${reference.name}`;
        chip.title = reference.path;
        chips.append(chip);
      }
      row.append(chips);
    }
  } else {
    row.className = "md text-sm text-zinc-200";
    row.innerHTML = markdown(message.content);
  }
  messagesEl.append(row);
  return row;
}

/**
 * 流式气泡：正文与「正在生成」指示分开，
 * 每次事件只换正文，避免三点动画被重建打断（IIE4AD-36x 反馈）。
 */
function ensureStreamingBubble(): StreamingBubble {
  if (!streaming) {
    const root = document.createElement("div");
    root.className = "md text-sm text-zinc-200";
    const body = document.createElement("div");
    const indicator = generatingEl();
    root.append(body, indicator);
    messagesEl.append(root);
    streaming = { root, body, indicator };
  }
  return streaming;
}

function updateStreamingBubble(text: string, pending: boolean) {
  const bubble = ensureStreamingBubble();
  bubble.body.innerHTML = markdown(text);
  bubble.indicator.classList.toggle("hidden", !pending);
}

/** 隐藏当前气泡的生成指示（收到末帧或用户停止时）。 */
function settleStreamingBubble() {
  streaming?.indicator.classList.add("hidden");
}

async function loadHistory() {
  messagesEl.replaceChildren();
  streaming = null;
  // 切会话/新对话后，之前的生成状态不再属于当前视图
  generating = false;
  updateSendUi();
  if (!conversationId) {
    setTitle("新对话");
    emptyEl.textContent =
      "这是一段新对话：在下方输入即可开始；⌘P 或右上角历史按钮查看历史会话。";
    emptyEl.classList.remove("hidden");
    composerEl.focus();
    return;
  }
  emptyEl.classList.add("hidden");
  try {
    const messages = await invoke<Message[]>("side_messages", { conversationId });
    setTitle(titleFrom(messages) ?? "AI 对话");
    for (const message of messages) {
      appendMessage(message);
    }
  } catch (err) {
    setError(`读取历史失败：${String(err)}`);
  }
  scrollToEnd();
  composerEl.focus();
}

async function send() {
  const message = composerEl.value.trim();
  if (!message || sending) return;
  sending = true;
  setError(null);
  composerEl.value = "";
  emptyEl.classList.add("hidden");
  const row = appendMessage({ role: "user", content: message });
  if (titleEl.textContent === "新对话") {
    setTitle(titleFrom([{ role: "user", content: message }]) ?? "AI 对话");
  }
  streaming = null;
  try {
    conversationId = await invoke<string>("side_send", {
      conversationId: conversationId ?? "",
      message,
    });
    updateStreamingBubble("", true);
    scrollToEnd();
    generating = true;
    updateSendUi();
    // 新会话立刻出现在历史卡（标题 = 首条消息）
    void loadConversations();
  } catch (err) {
    // 失败回滚：这条消息没有落库，恢复输入避免用户重打。
    row.remove();
    composerEl.value = message;
    setError(String(err));
    generating = false;
    updateSendUi();
  } finally {
    sending = false;
  }
}

// ---- 事件 ----

// 从命令盘 ⌘M / tray 带会话进来（payload.conversationId 为空 = 新对话）。
void listen<SideOpenPayload>("side-open", (event) => {
  const raw = event.payload?.conversationId;
  conversationId = typeof raw === "string" && raw.length > 0 ? raw : null;
  closeHistoryCard();
  closeActionsMenu();
  setError(null);
  void loadHistory();
  void loadConversations();
});

// 侧栏续聊的流式回答：commandId 与会话 id 都对上才采用。
void listen<CommandEventPayload>("command-event", (event) => {
  const update = event.payload?.itemUpdated;
  if (!update || update.commandId !== SIDE_COMMAND_ID) return;
  const payload = update.item.payload as { conversationId?: string } | null;
  if (!payload || payload.conversationId !== conversationId) return;
  // 末帧（pending=false）标志生成结束（IIE4AD-365）
  const pending = update.item.pending === true;
  updateStreamingBubble(update.item.detail ?? "", pending);
  scrollToEnd();
  if (!pending && generating) {
    generating = false;
    updateSendUi();
    void loadConversations();
  }
});

// ---- 附件（📎 = 与面板 ⌘⇧A 同一条路径，ADR-0010）----

const ATTACH_ROW_BASE =
  "flex items-center gap-2 border-t border-sky-500/30 bg-sky-500/10 px-3 py-2 text-xs";

function openAttachRow() {
  attachRowEl.className = `${ATTACH_ROW_BASE} text-sky-200`;
  attachPathEl.value = "";
  attachMsgEl.textContent = "";
  attachMsgEl.className = "shrink-0";
  attachPathEl.focus();
}

function closeAttachRow() {
  attachRowEl.className = "hidden";
  attachMsgEl.textContent = "";
}

async function submitAttachPath() {
  const raw = attachPathEl.value.trim();
  if (!raw) {
    closeAttachRow();
    return;
  }
  try {
    const info = await validatePath(raw);
    composerEl.value = appendMention(composerEl.value, info.path);
    closeAttachRow();
    composerEl.focus();
  } catch (err) {
    attachMsgEl.className = "shrink-0 text-red-300";
    attachMsgEl.textContent = String(err);
  }
}

attachPathEl.addEventListener("keydown", (e) => {
  if (e.key === "Enter" && !e.isComposing) {
    e.preventDefault();
    e.stopPropagation();
    void submitAttachPath();
  } else if (e.key === "Escape") {
    e.preventDefault();
    e.stopPropagation();
    closeAttachRow();
    composerEl.focus();
  }
});

attachEl.addEventListener("click", () => {
  if (attachRowEl.className === "hidden") openAttachRow();
  else {
    closeAttachRow();
    composerEl.focus();
  }
});

// ---- 键盘：Enter 发送 / Shift+Enter 换行 / Esc 停止或收起 / ⌘N 新对话 ----

function updateSendUi() {
  sendEl.className = generating
    ? "flex items-center gap-1 rounded-md bg-amber-600 px-3 py-1 text-xs text-white hover:bg-amber-500"
    : "flex items-center gap-1 rounded-md bg-sky-600 px-3 py-1 text-xs text-white hover:bg-sky-500";
  sendEl.replaceChildren(
    iconEl(generating ? "stop" : "send", { size: 13 }),
    document.createTextNode(generating ? "停止" : "发送 ⏎"),
  );
}

composerEl.addEventListener("keydown", (e) => {
  if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
    e.preventDefault();
    void send();
  }
});

/** 停止生成（平台级：停止所有进行中的生成，IIE4AD-365）。 */
async function stopGeneration() {
  try {
    await invoke<number>("stop_generation");
  } catch (err) {
    setError(String(err));
  }
  generating = false;
  settleStreamingBubble();
  updateSendUi();
}

window.addEventListener("keydown", (e) => {
  const mod = e.metaKey || e.ctrlKey;
  // 通用动作（ADR-0014）：Browse ⌘P / Actions ⌘⇧P / New ⌘N。
  // 语义与命令盘同源（./keymap），落点由本视图决定：历史卡 / 操作菜单 / 新对话。
  const general = generalActionOf(e);
  if (general === "actions") {
    e.preventDefault();
    if (actionsOpen) closeActionsMenu();
    else openActionsMenu();
    return;
  }
  if (general === "browse") {
    e.preventDefault();
    closeActionsMenu();
    toggleHistoryCard();
    return;
  }
  if (general === "new") {
    e.preventDefault();
    newChat();
    return;
  }
  // ⌃[ / ⌃]：按当前列表（含筛选）前后切换会话
  if (e.ctrlKey && (e.key === "[" || e.key === "]")) {
    e.preventDefault();
    void stepConversation(e.key === "]" ? 1 : -1);
    return;
  }
  if (e.key === "Escape" || (mod && e.key.toLowerCase() === "w")) {
    e.preventDefault();
    if (historyOpen) {
      closeHistoryCard();
      return;
    }
    if (actionsOpen) {
      closeActionsMenu();
      return;
    }
    if (generating) {
      void stopGeneration();
      return;
    }
    void getCurrentWindow().hide();
  }
});

window.addEventListener("focus", () => composerEl.focus());

sendEl.addEventListener("click", () => {
  if (generating) void stopGeneration();
  else void send();
});

closeEl.addEventListener("click", () => void getCurrentWindow().hide());

// ---- 启动 ----

updateSendUi();
void loadConversations();
void loadHistory();
