import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import DOMPurify from "dompurify";
import { marked } from "marked";
import "./styles.css";
import { appendMention, validatePath } from "./attachment";
import { iconEl } from "./icons";

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

const conversationsEl = document.querySelector<HTMLUListElement>("#conversations")!;
const filterEl = document.querySelector<HTMLInputElement>("#history-filter")!;
const messagesEl = document.querySelector<HTMLElement>("#messages")!;
const composerEl = document.querySelector<HTMLTextAreaElement>("#composer")!;
const emptyEl = document.querySelector<HTMLDivElement>("#empty")!;
const errorEl = document.querySelector<HTMLDivElement>("#error")!;
const titleEl = document.querySelector<HTMLSpanElement>("#chat-title")!;
const sendEl = document.querySelector<HTMLButtonElement>("#send")!;
const newChatEl = document.querySelector<HTMLButtonElement>("#new-chat")!;
const closeEl = document.querySelector<HTMLButtonElement>("#close")!;
const attachEl = document.querySelector<HTMLButtonElement>("#attach")!;

/** 当前会话；null = 空状态（首次发送会自动新建）。 */
let conversationId: string | null = null;
/** 正在流式更新的回答气泡。 */
let streaming: HTMLElement | null = null;
let sending = false;
/** 后端仍在生成（收到 pending=false 或主动停止后结束）。 */
let generating = false;

// ---- 固定图标 ----

newChatEl.replaceChildren(iconEl("plus", { size: 16 }));
newChatEl.title = "新对话（⌘N）";
closeEl.replaceChildren(iconEl("close", { size: 16 }));
attachEl.replaceChildren(iconEl("paperclip", { size: 14 }), document.createTextNode("附件"));

function updateSendUi() {
  sendEl.className = generating
    ? "flex items-center gap-1 rounded-md bg-amber-600 px-3 py-1 text-xs text-white hover:bg-amber-500"
    : "flex items-center gap-1 rounded-md bg-sky-600 px-3 py-1 text-xs text-white hover:bg-sky-500";
  sendEl.replaceChildren(
    iconEl(generating ? "stop" : "send", { size: 13 }),
    document.createTextNode(generating ? "停止" : "发送 ⏎"),
  );
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

function setError(text: string | null) {
  errorEl.classList.toggle("hidden", !text);
  errorEl.textContent = text ?? "";
}

function markdown(text: string): string {
  return DOMPurify.sanitize(marked.parse(text, { async: false }));
}

/** 列表副标题的相对时间（与后端 `relative_time` 同一分档）。 */
function relativeTime(updatedUnix: number): string {
  const secs = Math.max(0, Math.floor(Date.now() / 1000) - updatedUnix);
  if (secs < 60) return "刚刚";
  if (secs < 3600) return `${Math.floor(secs / 60)} 分钟前`;
  if (secs < 86400) return `${Math.floor(secs / 3600)} 小时前`;
  return `${Math.floor(secs / 86400)} 天前`;
}

/** 标题：取第一条用户消息首行（与历史列表的会话标题规则一致）。 */
function titleFrom(messages: Message[]): string | null {
  const first = messages.find(
    (message) => message.role === "user" && message.content.trim(),
  );
  if (!first) return null;
  const line = first.content.trim().split("\n")[0];
  return line.length > 24 ? `${line.slice(0, 24)}…` : line;
}

function setTitle(text: string) {
  titleEl.textContent = text;
}

// ---- 左栏：历史会话 ----

let filterTimer: ReturnType<typeof setTimeout> | undefined;
filterEl.addEventListener("input", () => {
  clearTimeout(filterTimer);
  filterTimer = setTimeout(() => void loadConversations(), 80);
});

async function loadConversations() {
  const query = filterEl.value.trim();
  let list: Conversation[] = [];
  try {
    list = await invoke<Conversation[]>("side_conversations", {
      query: query || null,
    });
  } catch (err) {
    setError(`读取历史失败：${String(err)}`);
    return;
  }
  if (list.length === 0) {
    const hint = document.createElement("li");
    hint.className = "px-2 py-3 text-xs text-zinc-600";
    hint.textContent = query ? "没有匹配的会话" : "还没有历史会话";
    conversationsEl.replaceChildren(hint);
    return;
  }
  conversationsEl.replaceChildren(
    ...list.map((conversation) => {
      const li = document.createElement("li");
      const active = conversation.id === conversationId;
      li.className =
        "flex cursor-default items-center gap-2 rounded-lg px-2 py-1.5 text-xs " +
        (active ? "bg-zinc-700/70 text-zinc-50" : "text-zinc-300 hover:bg-zinc-800/70");
      li.append(
        iconEl("message-square", {
          size: 14,
          className: active ? "text-zinc-200" : "text-zinc-500",
        }),
      );
      const label = document.createElement("span");
      label.className = "min-w-0 flex-1 truncate";
      label.textContent = conversation.title;
      label.title = conversation.title;
      li.append(label);
      const time = document.createElement("span");
      time.className = "shrink-0 text-[10px] text-zinc-500";
      time.textContent = relativeTime(conversation.updatedUnix);
      li.append(time);
      li.addEventListener("click", () => void selectConversation(conversation.id));
      return li;
    }),
  );
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

// ---- 右栏：消息 ----

/** 追加一条消息，返回其根元素（失败时可整块移除回滚）。 */
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

/** 流式气泡：没有就建一个（后续事件就地在它上面更新）。 */
function ensureStreamingBubble(): HTMLElement {
  if (!streaming) {
    streaming = appendMessage({ role: "assistant", content: "正在回答…" });
  }
  return streaming;
}

async function loadHistory() {
  messagesEl.replaceChildren();
  streaming = null;
  if (!conversationId) {
    setTitle("新对话");
    emptyEl.textContent =
      "这是一段新对话：在下方输入即可开始；左侧是历史会话，输入即筛选标题。";
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
    ensureStreamingBubble();
    scrollToEnd();
    generating = true;
    updateSendUi();
    // 新会话立刻出现在左栏（标题 = 首条消息）
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
  const bubble = ensureStreamingBubble();
  bubble.innerHTML = markdown(update.item.detail ?? "");
  scrollToEnd();
  // 末帧（pending=false）标志生成结束（IIE4AD-365）
  if (update.item.pending === false && generating) {
    generating = false;
    updateSendUi();
    void loadConversations();
  }
});

// ---- 附件（📎 = 与面板 ⌘⇧A 同一条路径，ADR-0010）----

const attachRowEl = document.querySelector<HTMLDivElement>("#attach-row")!;
const attachPathEl = document.querySelector<HTMLInputElement>("#attach-path")!;
const attachMsgEl = document.querySelector<HTMLSpanElement>("#attach-msg")!;
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
  updateSendUi();
}

window.addEventListener("keydown", (e) => {
  if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "n") {
    e.preventDefault();
    newChat();
    return;
  }
  if (e.key === "Escape" || ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "w")) {
    e.preventDefault();
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
