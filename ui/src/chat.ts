import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import "./styles.css";
import { createActionPlan } from "./actions";
import { appendMention, validatePath } from "./attachment";
import { createCard } from "./card";
import { iconEl } from "./icons";
import { kbdEl } from "./kbd";
import { GENERAL_KEY_LABELS, generalActionOf } from "./keymap";
import { conversationTitle, createConversationView } from "./messages";
import { initPointerIntent } from "./pointer";
import { streamCoalescer } from "./stream";
import { initTheme } from "./theme";
import type { CommandEventPayload, Conversation, Message, SideOpenPayload } from "./types";

// ---- Appearance (ADR-0035): resolve the theme before the first render ----
initTheme();
initPointerIntent();

const chatIconEl = document.querySelector<HTMLSpanElement>("#chat-icon")!;
const titleEl = document.querySelector<HTMLSpanElement>("#chat-title")!;
const actionsButtonEl = document.querySelector<HTMLButtonElement>("#actions-button")!;
const historyButtonEl = document.querySelector<HTMLButtonElement>("#history-button")!;
const newChatEl = document.querySelector<HTMLButtonElement>("#new-chat")!;
const closeEl = document.querySelector<HTMLButtonElement>("#close")!;
const historyCardEl = document.querySelector<HTMLDivElement>("#history-card")!;
const historySearchEl = document.querySelector<HTMLInputElement>("#history-search")!;
const historyListEl = document.querySelector<HTMLUListElement>("#history-list")!;
const actionsCardEl = document.querySelector<HTMLDivElement>("#actions-card")!;
const actionSearchEl = document.querySelector<HTMLInputElement>("#action-search")!;
const actionListEl = document.querySelector<HTMLUListElement>("#action-list")!;
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

/** Current conversation; null = empty state (a new one is created on first send). */
let conversationId: string | null = null;
/** The message stream (bubbles + streamed answers); the same renderer as the panel's Quick Ask page. */
const log = createConversationView(messagesEl);
let sending = false;
/** The backend is still generating (ends on pending=false or an explicit stop). */
let generating = false;
/** The most recently loaded conversation list (shared by the history card and ⌃[/⌃] stepping). */
let conversations: Conversation[] = [];

// ---- Fixed icons (three header buttons on the right: More Actions / History / New Chat) ----

chatIconEl.replaceChildren(iconEl("sparkles", { size: 15 }));
actionsButtonEl.replaceChildren(iconEl("command", { size: 16 }));
historyButtonEl.replaceChildren(iconEl("history", { size: 16 }));
newChatEl.replaceChildren(iconEl("plus", { size: 16 }));
closeEl.replaceChildren(iconEl("close", { size: 16 }));
attachEl.replaceChildren(iconEl("paperclip", { size: 14 }), document.createTextNode("Attachment"));
// The three generic-action tooltips also come from the shared keymap (ADR-0014), so they can't drift from the keymap table
actionsButtonEl.title = `More Actions (${GENERAL_KEY_LABELS.actions})`;
historyButtonEl.title = `Chat History (${GENERAL_KEY_LABELS.browse})`;
newChatEl.title = `New Chat (${GENERAL_KEY_LABELS.new})`;

// ---- Lightweight toast (instant feedback for menu actions, e.g. "Config file opened") ----

let toastTimer: ReturnType<typeof setTimeout> | undefined;

function toast(text: string, icon = "check") {
  toastIconEl.replaceChildren(
    iconEl(icon, { size: 12, className: icon === "check" ? "text-success" : "text-warning" }),
  );
  toastTextEl.textContent = text;
  toastEl.classList.remove("hidden");
  toastEl.classList.add("flex");
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => {
    toastEl.classList.add("hidden");
    toastEl.classList.remove("flex");
  }, 1600);
}

// ---- Chat history: floating card centered at the top (open via the history button / ⌘P) ----

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
    "moe-row flex cursor-default items-center gap-2 rounded-xl px-2 py-1.5 text-xs transition-colors duration-100 " +
    (highlighted
      ? "bg-surface-selected text-fg"
      : "text-fg")
  );
}

function paintHistoryHighlight() {
  for (const child of Array.from(historyListEl.children)) {
    const row = child as HTMLElement;
    const on = Number(row.dataset.index) === historyIndex;
    row.className = rowClass(on);
    if (on) row.dataset.focused = "true";
    else delete row.dataset.focused;
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
  li.append(iconEl("message-square", { size: 14, className: "text-fg-subtle" }));
  const label = document.createElement("span");
  label.className = "min-w-0 flex-1 truncate";
  label.textContent = conversation.title;
  label.title = conversation.title;
  li.append(label);
  const time = document.createElement("span");
  if (conversation.id === conversationId) {
    time.className = "shrink-0 text-[10px] text-accent";
    time.textContent = "Current";
  } else {
    time.className = "shrink-0 text-[10px] text-fg-subtle";
    time.textContent = relativeTime(conversation.updatedUnix);
  }
  li.append(time);
  li.addEventListener("click", () => {
    void selectConversation(conversation.id);
    closeHistoryCard();
  });
  return li;
}

function emptyRow(text: string): HTMLLIElement {
  const li = document.createElement("li");
  li.className = "px-2 py-3 text-xs text-fg-subtle";
  li.textContent = text;
  return li;
}

function renderConversations() {
  if (conversations.length === 0) {
    const query = historySearchEl.value.trim();
    historyListEl.replaceChildren(
      emptyRow(query ? "No matching conversations" : "No chat history yet: type below to start a new conversation"),
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
    setError(`Failed to load history: ${String(err)}`);
    return;
  }
  renderConversations();
}

/** Side View delete slot (ADR-0022): with the history card open it acts on the card's focused row / all, then refreshes the card. */
async function deleteInCard(all: boolean) {
  const target = conversations[historyIndex];
  if (!all && !target) return;
  try {
    const count = all
      ? await invoke<number>("delete_all", { commandId: "ai.search-history" })
      : await invoke<number>("delete_item", {
          commandId: "ai.search-history",
          item: conversationItem(target),
        });
    if (!count) return;
    toast(all ? `Deleted ${count} conversations` : "Deleted conversation");
    // The current conversation was deleted: back to the empty state so later messages don't write into a nonexistent conversation
    if (all || target.id === conversationId) newChat();
    else await loadConversations();
  } catch (err) {
    setError(String(err));
  }
}

/** Side View delete slot (ADR-0022): with the card closed it acts on the current conversation / all conversations. */
async function deleteCurrent(all: boolean) {
  try {
    if (all) {
      const count = await invoke<number>("delete_all", { commandId: "ai.search-history" });
      if (!count) {
        toast("No conversations to delete", "alert");
        return;
      }
      toast(`Deleted ${count} conversations`);
      newChat();
      return;
    }
    if (!conversationId) return;
    const item = {
      id: `ai.conversation.${conversationId}`,
      title: titleEl.textContent ?? "",
      actions: [],
      payload: { conversationId },
      pending: false,
    };
    const count = await invoke<number>("delete_item", {
      commandId: "ai.search-history",
      item,
    });
    if (!count) return;
    toast("Deleted current conversation");
    newChat();
  } catch (err) {
    setError(String(err));
  }
}

/** History entry → the Item the delete hook wants (payload carries conversationId; other fields synthesized). */
function conversationItem(conversation: Conversation) {
  return {
    id: `ai.conversation.${conversation.id}`,
    title: conversation.title,
    actions: [],
    payload: { conversationId: conversation.id },
    pending: false,
  };
}

historyButtonEl.addEventListener("click", () => {
  closeActionsCard();
  toggleHistoryCard();
});

historySearchEl.addEventListener("input", () => {
  clearTimeout(filterTimer);
  filterTimer = setTimeout(() => void loadConversations(), 80);
});

historySearchEl.addEventListener("keydown", (e) => {
  // The front-most list owns navigation (ADR-0031): ↓/↑ and the panel-wide ⌃N/⌃P move the history focus.
  const up =
    e.key === "ArrowUp" ||
    (e.ctrlKey && !e.metaKey && !e.shiftKey && e.key.toLowerCase() === "p");
  const down =
    e.key === "ArrowDown" ||
    (e.ctrlKey && !e.metaKey && !e.shiftKey && e.key.toLowerCase() === "n");
  if (up || down) {
    e.preventDefault();
    e.stopPropagation();
    moveHistory(down ? 1 : -1);
  } else if (e.key === "Enter" && !e.isComposing) {
    e.preventDefault();
    e.stopPropagation();
    const target = conversations[historyIndex];
    if (target) {
      void selectConversation(target.id);
      closeHistoryCard();
    }
  } else if (e.key === "Escape" || e.key === "Backspace") {
    // The front-most card owns these keys: the window-level "empty Backspace = Back" must
    // never see them (otherwise deleting a filter char would close the card, IIE4AD-406)
    e.stopPropagation();
    if (e.key === "Escape" || (historySearchEl.value === "" && !e.repeat)) {
      e.preventDefault();
      closeHistoryCard();
      composerEl.focus();
    }
  }
});

// ---- More Actions card (the ⌘ icon button / ⌘K): the same card as the panel's actions card (ADR-0028/0037) ----

function openConfig() {
  void invoke("invoke_command", {
    commandId: "moe.open-config",
    query: null,
    record: false,
  }).then(
    () => toast("Config file opened"),
    (err) => setError(String(err)),
  );
}

/**
 * The Side View's card (ADR-0037): the same registered-sources pipeline as the panel's, with this
 * surface's landing spots. Chat holds the page's general slots (Browse ⌘P opens the history card,
 * New ⌘N a blank conversation, Delete ⌃X / DeleteAll ⌃⇧X the current / all conversations), App and
 * Window the app rows.
 */
const sideActionPlan = createActionPlan([
  {
    title: "Chat",
    when: () => true,
    rows: () => [
      {
        title: "Chat History",
        icon: "history",
        keys: GENERAL_KEY_LABELS.browse,
        run: () => toggleHistoryCard(),
      },
      { title: "New Chat", icon: "plus", keys: GENERAL_KEY_LABELS.new, run: newChat },
      {
        title: "Remove Conversation",
        icon: "trash-2",
        keys: GENERAL_KEY_LABELS.delete,
        disabled: !conversationId,
        run: () => void deleteCurrent(false),
      },
      {
        title: "Remove All Conversations",
        icon: "trash-2",
        keys: GENERAL_KEY_LABELS.deleteAll,
        run: () => void deleteCurrent(true),
      },
    ],
  },
  {
    title: "App",
    when: () => true,
    rows: () => [
      { title: "Open Config File", icon: "settings-2", keys: GENERAL_KEY_LABELS.openConfig, run: openConfig },
    ],
  },
  {
    title: "Window",
    when: () => true,
    rows: () => [
      {
        title: "Hide Side View",
        icon: "close",
        keys: "Esc",
        run: () => void getCurrentWindow().hide(),
      },
    ],
  },
]);

const actionsCard = createCard({
  cardEl: actionsCardEl,
  listEl: actionListEl,
  inputEl: actionSearchEl,
  onClose: () => composerEl.focus(),
});

function openActionsCard() {
  closeHistoryCard();
  void sideActionPlan
    .sections({ surface: "side", mode: "chat" })
    .then((sections) => actionsCard.open(sections));
}

function closeActionsCard() {
  actionsCard.close();
}

function toggleActionsCard() {
  if (actionsCard.isOpen()) closeActionsCard();
  else openActionsCard();
}

actionsButtonEl.addEventListener("click", () => toggleActionsCard());

// Close on outside click (the buttons' own clicks are handled; exclude the two overlays and the two trigger buttons here)
document.addEventListener("mousedown", (e) => {
  const target = e.target as Node;
  if (historyOpen && !historyCardEl.contains(target) && !historyButtonEl.contains(target)) {
    closeHistoryCard();
  }
  if (actionsCard.isOpen() && !actionsCardEl.contains(target) && !actionsButtonEl.contains(target)) {
    closeActionsCard();
  }
});

// ---- Conversation switching ----

/**
 * ⌃[ / ⌃]: step through conversations per the current list (including the filter) — `⌃[` is Back (the
 * previous, older conversation), `⌃]` is Forward (the newer one). The list is newest-first, so
 * backward is +1 in the array; a conversation outside it (a blank new chat) sits at "now": backward
 * lands on the newest, forward goes nowhere. Both ends clamp.
 */
async function stepConversation(backward: boolean) {
  if (conversations.length === 0) return;
  const index = conversations.findIndex((c) => c.id === conversationId);
  const delta = backward ? 1 : -1;
  const next =
    index === -1
      ? backward
        ? 0
        : -1
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
  generating = false;
  updateSendUi();
  setError(null);
  void loadHistory();
  void loadConversations();
  composerEl.focus();
}

newChatEl.addEventListener("click", newChat);

// ---- Message area ----

function relativeTime(updatedUnix: number): string {
  const secs = Math.max(0, Math.floor(Date.now() / 1000) - updatedUnix);
  if (secs < 60) return "just now";
  if (secs < 3600) return `${Math.floor(secs / 60)}m ago`;
  if (secs < 86400) return `${Math.floor(secs / 3600)}h ago`;
  return `${Math.floor(secs / 86400)}d ago`;
}

function setError(text: string | null) {
  errorEl.classList.toggle("hidden", !text);
  errorEl.textContent = text ?? "";
}

function setTitle(text: string) {
  titleEl.textContent = text;
}

/** Append a finished local message (user bubble / answer from history). */
function appendMessage(message: Message): HTMLElement {
  return log.append(message);
}

async function loadHistory() {
  log.clear();
  // After switching conversations / a new chat, the previous generation state no longer belongs to this view
  generating = false;
  updateSendUi();
  if (!conversationId) {
    setTitle("New Chat");
    emptyEl.textContent =
      "This is a new chat: type below to get started; use ⌘P or the history button at the top right to view chat history.";
    emptyEl.classList.remove("hidden");
    composerEl.focus();
    return;
  }
  emptyEl.classList.add("hidden");
  try {
    const messages = await invoke<Message[]>("side_messages", { conversationId });
    setTitle(conversationTitle(messages) ?? "AI Chat");
    for (const message of messages) {
      appendMessage(message);
    }
  } catch (err) {
    setError(`Failed to load history: ${String(err)}`);
  }
  log.scrollToEnd();
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
  if (titleEl.textContent === "New Chat") {
    setTitle(conversationTitle([{ role: "user", content: message }]) ?? "AI Chat");
  }
  log.beginAnswer();
  try {
    conversationId = await invoke<string>("side_send", {
      conversationId: conversationId ?? "",
      message,
    });
    log.updateStreaming("", true);
    log.scrollToEnd();
    generating = true;
    updateSendUi();
    // The new conversation appears in the history card immediately (title = first message)
    void loadConversations();
  } catch (err) {
    // Rollback on failure: this message was not persisted; restore the input so the user doesn't retype it.
    row.remove();
    composerEl.value = message;
    setError(String(err));
    generating = false;
    updateSendUi();
  } finally {
    sending = false;
  }
}

// ---- Events ----

// Entered with a conversation from the command panel ⌘M / tray (empty payload.conversationId = new chat).
void listen<SideOpenPayload>("side-open", (event) => {
  const raw = event.payload?.conversationId;
  conversationId = typeof raw === "string" && raw.length > 0 ? raw : null;
  closeHistoryCard();
  closeActionsCard();
  setError(null);
  void loadHistory();
  void loadConversations();
});

// Streamed answers of the shown conversation, whichever surface started them: the Side View's own
// continuation (`ai.side` frames) and the panel's Quick Ask page (`ai.quick-ask` frames) both carry
// the conversation id, so both windows mirror the same answer (ADR-0036). Frames arrive per model
// delta and run through the coalescer (ADR-0038): the DOM re-renders at most every ~50 ms; reasoning
// blocks ride the payload (MOE-0008) and render as the bubble's Thinking block.
const frames = streamCoalescer<
  string,
  { text: string; pending: boolean; reasoning: string }
>((batch) => {
  const frame = batch.get(conversationId ?? "");
  if (!frame) return;
  log.updateStreaming(frame.text, frame.pending, frame.reasoning);
  log.scrollToEnd();
  if (generating !== frame.pending) {
    generating = frame.pending;
    updateSendUi();
  }
  // The final frame (pending=false) marks the end of generation (IIE4AD-365)
  if (!frame.pending) void loadConversations();
});

void listen<CommandEventPayload>("command-event", (event) => {
  const update = event.payload?.itemUpdated;
  if (!update) return;
  const meta = update.item.payload as { conversationId?: string; reasoning?: string } | null;
  if (!meta || meta.conversationId !== conversationId) return;
  frames.push(meta.conversationId, {
    text: update.item.detail ?? "",
    pending: update.item.pending === true,
    reasoning: meta.reasoning ?? "",
  });
});

// ---- Attachments (📎 = same path as the panel's ⌘⇧A, ADR-0010) ----

const ATTACH_ROW_BASE =
  "flex items-center gap-2 border-t border-accent-line bg-accent-soft px-3 py-2 text-xs";

function openAttachRow() {
  attachRowEl.className = `${ATTACH_ROW_BASE} text-accent`;
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
    attachMsgEl.className = "shrink-0 text-danger";
    attachMsgEl.textContent = String(err);
  }
}

attachPathEl.addEventListener("keydown", (e) => {
  if (e.key === "Enter" && !e.isComposing) {
    e.preventDefault();
    e.stopPropagation();
    void submitAttachPath();
  } else if (
    e.key === "Escape" ||
    (e.key === "Backspace" && attachPathEl.value === "" && !e.repeat)
  ) {
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

// ---- Keyboard: Enter sends / Shift+Enter newlines / Esc stops or closes / ⌘N new chat ----

function updateSendUi() {
  sendEl.className = generating
    ? "moe-focus-ring flex items-center gap-1.5 rounded-full bg-warning-fill px-3 py-1 text-xs text-on-warning transition-colors hover:bg-warning-fill-hover"
    : "moe-focus-ring flex items-center gap-1.5 rounded-full bg-accent-fill px-3 py-1 text-xs text-on-accent transition-colors hover:bg-accent-fill-hover";
  sendEl.replaceChildren(
    iconEl(generating ? "stop" : "send", { size: 13 }),
    document.createTextNode(generating ? "Stop" : "Send"),
    kbdEl(generating ? "Esc" : "⏎", { firstOnly: true }),
  );
}

composerEl.addEventListener("keydown", (e) => {
  if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
    e.preventDefault();
    void send();
  }
});

/** Stop generation (platform-wide: stops all in-progress generation, IIE4AD-365). */
async function stopGeneration() {
  try {
    await invoke<number>("stop_generation");
  } catch (err) {
    setError(String(err));
  }
  generating = false;
  log.settleStreaming();
  updateSendUi();
}

window.addEventListener("keydown", (e) => {
  const mod = e.metaKey || e.ctrlKey;
  // Generic actions (ADR-0014): Browse ⌘P / Actions ⌘K / New ⌘N.
  // Semantics shared with the command panel (./keymap); the landing spots are decided by this view: history card / actions menu / new chat.
  const general = generalActionOf(e);
  if (general === "actions") {
    e.preventDefault();
    toggleActionsCard();
    return;
  }
  if (general === "browse") {
    e.preventDefault();
    closeActionsCard();
    toggleHistoryCard();
    return;
  }
  if (general === "new") {
    e.preventDefault();
    newChat();
    return;
  }
  if (general === "openConfig") {
    // ⌘, opens the config file (the macOS Preferences convention, ADR-0027 amendment)
    e.preventDefault();
    closeActionsCard();
    openConfig();
    return;
  }
  // Delete slot (ADR-0022): ⌃X deletes the current conversation / focused row in the history card; ⌃⇧X deletes all conversations.
  // Semantics shared with the command panel (./keymap); the landing spots are decided by this view.
  // When an editor (composer/attachment input) is focused, let native cut through instead of deleting conversations by accident.
  if (general === "delete" || general === "deleteAll") {
    const target = e.target as HTMLElement | null;
    const inEditor =
      target &&
      (target.tagName === "TEXTAREA" || (target.tagName === "INPUT" && !historyOpen));
    if (inEditor) return;
    e.preventDefault();
    if (historyOpen) void deleteInCard(general === "deleteAll");
    else void deleteCurrent(general === "deleteAll");
    return;
  }
  // ⌃[ / ⌃]: step through conversations per the current list (including the filter) — ⌃[ backward to
  // the previous (older) conversation, ⌃] forward to the newer one
  if (e.ctrlKey && (e.key === "[" || e.key === "]")) {
    e.preventDefault();
    void stepConversation(e.key === "[");
    return;
  }
  if (e.key === "Escape" || (mod && e.key.toLowerCase() === "w")) {
    e.preventDefault();
    void back();
    return;
  }
  // The front-most list owns navigation and Backspace (ADR-0031): while a card is open, its own input
  // routing decides; this guard covers focus sitting elsewhere on the surface.
  const cardOpen = historyOpen || actionsCard.isOpen();
  // Backspace with empty input = Back (layered back, ADR-0017):
  // leave it alone when non-empty (normal delete); when empty, back out layer by layer, but the root layer does not hide the side view (quit:false).
  if (!cardOpen && e.key === "Backspace" && composerEl.value === "" && !e.isComposing && !e.repeat) {
    e.preventDefault();
    void back({ quit: false });
  }
});

/**
 * Esc / empty-input Backspace layered back: history card → actions menu → stop generation → (may hide the window).
 * Empty Backspace passes quit:false: when there is no layer left, stay in place instead of hiding the window.
 */
async function back(options: { quit?: boolean } = {}) {
  const { quit = true } = options;
  if (historyOpen) {
    closeHistoryCard();
    return;
  }
  if (actionsCard.isOpen()) {
    closeActionsCard();
    return;
  }
  if (generating) {
    await stopGeneration();
    return;
  }
  if (!quit) return; // Root layer: empty Backspace stops here
  await getCurrentWindow().hide();
}

window.addEventListener("focus", () => composerEl.focus());

sendEl.addEventListener("click", () => {
  if (generating) void stopGeneration();
  else void send();
});

closeEl.addEventListener("click", () => void getCurrentWindow().hide());

// ---- Startup ----

updateSendUi();
void loadConversations();
void loadHistory();
