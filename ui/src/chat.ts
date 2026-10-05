import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import DOMPurify from "dompurify";
import { marked } from "marked";
import "./styles.css";

// ---- 类型：镜像 Rust 契约（ADR-0006）----

interface Message {
  role: "user" | "assistant";
  content: string;
}
interface Item {
  id: string;
  title: string;
  subtitle?: string;
  payload: unknown;
  detail?: string | null;
}
interface CommandEventPayload {
  itemUpdated?: { commandId: string; item: Item };
}
interface SideOpenPayload {
  conversationId?: string | null;
}

/** 侧栏续聊事件约定（moe-extensions::ai::SIDE_COMMAND_ID）。 */
const SIDE_COMMAND_ID = "ai.side";

const historyEl = document.querySelector<HTMLElement>("#history")!;
const composerEl = document.querySelector<HTMLTextAreaElement>("#composer")!;
const emptyEl = document.querySelector<HTMLDivElement>("#empty")!;
const errorEl = document.querySelector<HTMLDivElement>("#error")!;

/** 当前会话；null = 空状态（首次发送会自动新建）。 */
let conversationId: string | null = null;
/** 正在流式更新的回答气泡。 */
let streaming: HTMLElement | null = null;
let sending = false;

function scrollToBottom() {
  historyEl.scrollTop = historyEl.scrollHeight;
}

function setError(text: string | null) {
  errorEl.classList.toggle("hidden", !text);
  errorEl.textContent = text ?? "";
}

function markdown(text: string): string {
  return DOMPurify.sanitize(marked.parse(text, { async: false }));
}

/** 追加一条消息，返回其根元素（失败时可整块移除回滚）。 */
function appendMessage(message: Message): HTMLElement {
  const row = document.createElement("div");
  if (message.role === "user") {
    row.className = "flex justify-end";
    const card = document.createElement("div");
    card.className =
      "max-w-[85%] whitespace-pre-wrap rounded-2xl rounded-br-sm bg-sky-600/80 px-3 py-2 text-sm text-white";
    card.textContent = message.content;
    row.append(card);
  } else {
    row.className = "md text-sm text-zinc-200";
    row.innerHTML = markdown(message.content);
  }
  historyEl.append(row);
  scrollToBottom();
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
  historyEl.replaceChildren();
  streaming = null;
  if (!conversationId) {
    emptyEl.textContent =
      "这是一段新对话：在下方输入即可开始；回答会按会话保存，之后可用「AI: 搜索历史会话」找回。";
    emptyEl.classList.remove("hidden");
    composerEl.focus();
    return;
  }
  emptyEl.classList.add("hidden");
  try {
    const messages = await invoke<Message[]>("side_messages", {
      conversationId,
    });
    for (const message of messages) {
      appendMessage(message);
    }
  } catch (err) {
    setError(`读取历史失败：${String(err)}`);
  }
  scrollToBottom();
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
  streaming = null;
  try {
    conversationId = await invoke<string>("side_send", {
      conversationId: conversationId ?? "",
      message,
    });
    ensureStreamingBubble();
  } catch (err) {
    // 失败回滚：这条消息没有落库，恢复输入避免用户重打。
    row.remove();
    composerEl.value = message;
    setError(String(err));
  } finally {
    sending = false;
  }
}

// ---- 事件 ----

// 从命令盘 ⌘M 带会话进来（payload.conversationId 为空 = 新对话）。
void listen<SideOpenPayload>("side-open", (event) => {
  const raw = event.payload?.conversationId;
  conversationId = typeof raw === "string" && raw.length > 0 ? raw : null;
  setError(null);
  void loadHistory();
});

// 侧栏续聊的流式回答：commandId 与会话 id 都对上才采用。
void listen<CommandEventPayload>("command-event", (event) => {
  const update = event.payload?.itemUpdated;
  if (!update || update.commandId !== SIDE_COMMAND_ID) return;
  const payload = update.item.payload as { conversationId?: string } | null;
  if (!payload || payload.conversationId !== conversationId) return;
  const bubble = ensureStreamingBubble();
  bubble.innerHTML = markdown(update.item.detail ?? "");
  scrollToBottom();
});

// ---- 键盘：Enter 发送 / Shift+Enter 换行 / Esc 收起 ----

composerEl.addEventListener("keydown", (e) => {
  if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
    e.preventDefault();
    void send();
  }
});

window.addEventListener("keydown", (e) => {
  if (e.key === "Escape" || ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "w")) {
    e.preventDefault();
    void getCurrentWindow().hide();
  }
});

window.addEventListener("focus", () => composerEl.focus());

document
  .querySelector<HTMLButtonElement>("#send")!
  .addEventListener("click", () => void send());
document
  .querySelector<HTMLButtonElement>("#close")!
  .addEventListener("click", () => void getCurrentWindow().hide());
