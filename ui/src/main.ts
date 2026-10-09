import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import DOMPurify from "dompurify";
import { marked } from "marked";
import "./styles.css";
import { appendMention, humanBytes, validatePath } from "./attachment";
import { createActionPlan, type PageContext } from "./actions";
import { createCard, type CardRow } from "./card";
import { generatingEl } from "./generating";
import { iconEl } from "./icons";
import { kbdEl } from "./kbd";
import { GENERAL_KEY_LABELS, generalActionOf, type EntryAction } from "./keymap";
import { logoEl } from "./logo";
import {
  CONVERSATION_PANE_CLASS,
  DETAIL_PANE_CLASS,
  LIST_FULL_CLASS,
  LIST_NARROW_CLASS,
  PANEL_WIDTH,
  pageShapeOf,
  panelHeightFor,
  ROW_HEIGHT,
  SECTION_HEADER_HEIGHT,
  type PageShape,
} from "./layout";
import { conversationTitle, createConversationView, type ConversationView } from "./messages";
import { initPointerIntent } from "./pointer";
import { store } from "./store";
import { streamCoalescer } from "./stream";
import { initTheme, themePreference, type ThemePreference } from "./theme";
import type {
  Action,
  ActionResult,
  CommandEventPayload,
  CommandMeta,
  CommandSection,
  Conversation,
  Item,
  Message,
} from "./types";

// ---- Appearance (ADR-0035): the config theme override lands before the first render ----
initTheme();
initPointerIntent();

// ---- View state ----

/** The panel's page kinds: the command layer, a result page (any shape), the Quick Ask conversation page. */
type Mode = "commands" | "items" | "chat";
interface View {
  mode: Mode;
  focus: number;
  commands: CommandMeta[];
  /** Source grouping of the command layer (ADR-0020): used to render section headers; commands is its flattening, focus/navigation is based on the flat list. */
  sections: CommandSection[];
  /** The Suggestions section's flat-list range (ADR-0025): start = -1 when the section is absent. */
  suggestionsStart: number;
  /** The Suggestions section's item count (0 = none, ADR-0025). */
  suggestionsCount: number;
  items: Item[];
  /** Source command of items mode. */
  sourceCommandId?: string;
  /** Source command's icon (fallback when a result item has no icon of its own). */
  sourceIcon?: string;
  /** Source command's title (Input Bar placeholder; also outside the command list when entered via the ⌘P/⌘N entry). */
  sourceTitle?: string;
  /** Whether the source command is Live (re-runs the list as input changes). */
  sourceLive?: boolean;
  /** View shape declared by the result: true = the single result is the content, detail fills the panel (ADR-0013). */
  detailFull?: boolean;
}

const q = document.querySelector<HTMLInputElement>("#query")!;
const queryRowEl = document.querySelector<HTMLDivElement>("#query-row")!;
const listEl = document.querySelector<HTMLUListElement>("#list")!;
const detailEl = document.querySelector<HTMLDivElement>("#detail")!;
const actionBarEl = document.querySelector<HTMLDivElement>("#action-bar")!;
const actionsCardEl = document.querySelector<HTMLDivElement>("#actions-card")!;
const actionSearchEl = document.querySelector<HTMLInputElement>("#action-search")!;
const actionListEl = document.querySelector<HTMLUListElement>("#action-list")!;
const chipEl = document.querySelector<HTMLButtonElement>("#avatar-chip")!;
const aboutCardEl = document.querySelector<HTMLDivElement>("#about-card")!;
const aboutSearchEl = document.querySelector<HTMLInputElement>("#about-search")!;
const aboutListEl = document.querySelector<HTMLUListElement>("#about-list")!;
const inputIconEl = document.querySelector<HTMLSpanElement>("#input-icon")!;
const bannerIconEl = document.querySelector<HTMLSpanElement>("#banner-icon")!;
const confirmCardEl = document.querySelector<HTMLDivElement>("#confirm-card")!;
const confirmTextEl = document.querySelector<HTMLDivElement>("#confirm-text")!;
const confirmOkEl = document.querySelector<HTMLButtonElement>("#confirm-ok")!;
const confirmCancelEl = document.querySelector<HTMLButtonElement>("#confirm-cancel")!;

const view = store<View>({
  mode: "commands",
  focus: 0,
  commands: [],
  sections: [],
  suggestionsStart: -1,
  suggestionsCount: 0,
  items: [],
});

/// The item the current detail card belongs to (streaming events re-render by it)
let detailItemId: string | null = null;
/** Detail card shape: preview (focused preview, the list stays) / message (full-screen card, e.g. errors). */
type DetailMode = "none" | "preview" | "message";
let detailMode: DetailMode = "none";

/**
 * Actions card (the floating card opened by ⌘K / ⌘⇧P): all actions + filtering, without replacing the body.
 * It is the same card the Side View opens as More Actions (ADR-0028); the panel anchors it bottom-right
 * with the input at the bottom, and the pill follows its highlighted row.
 */
const actionsCard = createCard({
  cardEl: actionsCardEl,
  listEl: actionListEl,
  inputEl: actionSearchEl,
  emptyText: "This result has no actions",
  onFocusChange: () => renderActionBar(),
  onClose: () => {
    renderActionBar();
    q.focus();
  },
});

// ---- Quick Ask page (the panel's conversation, ADR-0036) ----

/** The conversation the page shows; null = blank new chat (the first send creates one). */
let chatConversationId: string | null = null;
/** An answer is streaming: Enter stops it instead of sending (ADR-0036). */
let chatGenerating = false;
/** The conversation title (first user message): the Input Bar's placeholder. */
let chatTitle: string | null = null;
/** Chat history, most recent first (⌃[ / ⌃] stepping; refreshed on entry, send and delete). */
let chatConversations: Conversation[] = [];

/** The page's DOM: message log + state hint + error line (rebuilt with the detail pane, like the detail parts). */
interface ChatParts {
  log: HTMLElement;
  hint: HTMLElement;
  error: HTMLElement;
  view: ConversationView;
}
let chatParts: ChatParts | null = null;

/** Back while generating asks first (ADR-0036/0038): the dialog owns the keys while it is up and runs the stop on OK. */
let confirmOpen = false;
let confirmAction: (() => void) | null = null;

// ---- Avatar chip (ADR-0026): the bottom-left card ----

/** Extension identity (mirror of the Rust `ExtensionMeta`): avatar = its representative icon. */
interface ExtensionMeta {
  id: string;
  title: string;
  icon?: string | null;
}

/** Cache by command id: entering a command fetches its extension's identity once. */
const extensionMetaCache = new Map<string, ExtensionMeta>();
/** Identity of the extension the current results layer belongs to (null = command layer / unknown). */
let extensionMeta: ExtensionMeta | null = null;

/** While a toast is showing the chip expands with it; null = resting state (avatar/extension). */
let toastState: { text: string; icon: string } | null = null;
let toastTimer: ReturnType<typeof setTimeout> | undefined;

/** Fallback identity for extensions that declare no icon: the brand mark, inline so it follows the
   theme (the old moe.png bitmap could not). */
function appAvatarEl(): SVGElement {
  return logoEl("moe-avatar-logo text-fg-muted");
}

/** The chip's label: the extension's name while inside a command, otherwise none. */
function chipLabel(text: string): HTMLSpanElement {
  const label = document.createElement("span");
  label.className = "moe-avatar-label";
  label.textContent = text;
  return label;
}

/**
 * Render the avatar chip (ADR-0026): toast > extension (inside a command) > the About button's
 * own icon. The chip is one row tall; its glyph/avatar sits centered on the item icons' x.
 */
function renderChip() {
  chipEl.replaceChildren();
  delete chipEl.dataset.labeled;
  delete chipEl.dataset.toast;
  if (toastState) {
    chipEl.dataset.labeled = "true";
    // A toast (unlike an extension name) may carry a long message: the wider label cap plus the
    // title tooltip keep errors readable instead of ellipsized at 170px (styles.css).
    chipEl.dataset.toast = "true";
    chipEl.title = toastState.text;
    chipEl.append(
      iconEl(toastState.icon, {
        size: 14,
        className: toastState.icon === "check" ? "text-success" : "text-warning",
      }),
      chipLabel(toastState.text),
    );
    return;
  }
  const v = view.get();
  // Inside a command (items) or on the Quick Ask page (chat): the extension's own icon + name (ADR-0026/0036)
  if (v.mode !== "commands" && extensionMeta) {
    chipEl.dataset.labeled = "true";
    chipEl.append(
      extensionMeta.icon
        ? iconEl(extensionMeta.icon, { size: 18, className: "text-fg-muted" })
        : appAvatarEl(),
      chipLabel(extensionMeta.title),
    );
    chipEl.title = aboutTitle(`About ${extensionMeta.title}`);
    return;
  }
  // Root: the About button's own icon (ADR-0026 amendment) — the chip is a button, not the brand
  chipEl.append(iconEl("message-circle-warning", { size: 18, className: "text-fg-muted" }));
  chipEl.title = aboutTitle("About Moe");
}

/** The chip's tooltip carries the About binding (from the Rust keymap, like every other display string). */
function aboutTitle(label: string): string {
  const keys = keyDisplay.get("about");
  return keys ? `${label} (${keys})` : label;
}

/** Fetch (and cache) the extension identity owning `commandId`, then refresh the chip. */
async function refreshExtensionMeta(commandId: string) {
  const cached = extensionMetaCache.get(commandId);
  if (cached) {
    extensionMeta = cached;
    renderChip();
    return;
  }
  try {
    const meta = await invoke<ExtensionMeta | null>("extension_meta", { commandId });
    if (meta) extensionMetaCache.set(commandId, meta);
    extensionMeta = meta ?? null;
  } catch {
    extensionMeta = null;
  }
  renderChip();
}

// ---- Lightweight feedback (for actions with no UI result, e.g. ⌥⏎ copy) ----
// Toast style (ADR-0026, Raycast-style): the bottom-left chip expands to show the message.

function toast(text: string, icon = "check") {
  toastState = { text, icon };
  renderChip();
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => {
    toastState = null;
    renderChip();
  }, 1400);
}

// ---- Floating action bar (Raycast-style): primary action + actions, keybindings still from the unified Rust keymap ----

/** Semantic → display string (e.g. apply → "⏎", showAllActions → "⌘K / ⌘⇧P"). */
const keyDisplay = new Map<string, string>();

async function initActionBar() {
  const entries = await invoke<[string, string][]>("keymap");
  for (const [display, semantic] of entries) {
    if (!keyDisplay.has(semantic)) keyDisplay.set(semantic, display);
  }
  renderActionBar();
  renderChip(); // the chip's About tooltip picks up its binding now that the keymap is loaded
}

/** Primary action of the focused item (same semantics as Apply): returns the title and where a click lands. */
function primaryActionOf(): { title: string; keys: string; run: () => void; disabled: boolean } {
  const v = view.get();
  const applyKeys = keyDisplay.get("apply") ?? "⏎";
  // When the actions card is open: the pill's primary action follows the highlighted action in the card (Raycast-style)
  if (actionsCard.isOpen()) {
    const row = actionsCard.focused();
    return {
      title: row?.title ?? "Apply",
      keys: row?.keys ?? applyKeys,
      run: () => actionsCard.runFocused(),
      disabled: !row,
    };
  }
  if (v.mode === "chat") {
    // Quick Ask page (ADR-0036): Enter sends the draft — or stops the stream while one is running.
    if (chatGenerating) {
      return {
        title: "Stop Generation",
        keys: applyKeys,
        run: () => void stopChatGeneration(),
        disabled: false,
      };
    }
    return {
      title: "Send",
      keys: applyKeys,
      run: () => void sendChatMessage(),
      disabled: q.value.trim() === "",
    };
  }
  if (v.mode === "items") {
    const item = v.items[v.focus];
    // While generating the primary action yields to Stop: Enter is the stop binding (⏎), Esc keeps its
    // first-priority stop on this layer (IIE4AD-365, ADR-0036 amendment)
    if (item?.pending) {
      return {
        title: "Stop Generation",
        keys: applyKeys,
        run: () => void stopGeneration(),
        disabled: false,
      };
    }
    const action = item ? primaryOf(item) : undefined;
    return {
      title: action?.title ?? "Apply",
      keys: action?.keybinding ?? applyKeys,
      run: () => void applyFocused(false),
      disabled: !action,
    };
  }
  return { title: "Apply", keys: applyKeys, run: () => void applyFocused(false), disabled: false };
}

function renderActionBar() {
  const primary = primaryActionOf();
  const primaryButton = document.createElement("button");
  primaryButton.id = "primary-action";
  primaryButton.disabled = primary.disabled;
  primaryButton.title = `${primary.title} (${primary.keys})`;
  primaryButton.append(
    document.createTextNode(primary.title),
    kbdEl(primary.keys, { firstOnly: true }),
  );
  primaryButton.addEventListener("click", () => {
    primary.run();
    q.focus(); // the action bar does not steal the input bar focus (keyboard-first)
  });

  const actionsKeys = keyDisplay.get("showAllActions") ?? "⌘K";
  const actionsButton = document.createElement("button");
  actionsButton.id = "actions-action";
  actionsButton.title = `All Actions (${actionsKeys})`;
  actionsButton.append(document.createTextNode("Actions"), kbdEl(actionsKeys, { firstOnly: true }));
  actionsButton.addEventListener("click", () => {
    toggleActionsCard();
  });

  // Buttons must not steal the input bar focus (otherwise a later Enter would re-click the button)
  for (const button of [primaryButton, actionsButton]) {
    button.addEventListener("mousedown", (e) => e.preventDefault());
  }

  actionBarEl.replaceChildren(primaryButton, actionsButton);
}

// ---- Rendering ----

/** Current page shape: list/split/detail (details in ui/src/layout.ts). */
let currentShape: PageShape | null = null;

/**
 * Resize the panel window by page shape (ADR-0018): the size is constant and stays the same across shape switches.
 * Height is measured so the row grid is exact (ADR-0026): input bar + exactly VISIBLE_ROWS rows + bottom padding.
 * The window is untouched while the shape is unchanged; failure does not affect rendering (window size is only polish).
 */
function applyPanelSize(shape: PageShape) {
  if (shape === currentShape) return;
  currentShape = shape;
  void invoke("resize_panel", {
    width: PANEL_WIDTH,
    height: measuredPanelHeight(),
  }).catch(() => {
    // Ignore: without a window handle, leave it alone
  });
}

/** Panel height measured from the real input bar: exactly VISIBLE_ROWS rows fit below it (ADR-0026). */
function measuredPanelHeight(): number {
  const inputHeight = queryRowEl.getBoundingClientRect().height || 48;
  return panelHeightFor(inputHeight);
}

interface Row {
  title: string;
  subtitle?: string;
  key?: string;
  /** Semantic name; undefined = use the source command's icon, or fall back if there is none. */
  icon?: string;
  /** Fallback shape (no icon specified) is displayed dimmed. */
  iconMuted?: boolean;
  /** Trailing type badge on command rows (ADR-0030): "Command" unless the extension declares otherwise. */
  badge?: string;
  /** Root page rows (ADR-0031): the declared shortcut reveals only while the row is focused or hovered. */
  revealKeys?: boolean;
}

function currentEntries(): Row[] {
  const v = view.get();
  return v.items.map((i) => {
    const icon = i.icon ?? v.sourceIcon;
    return {
      title: i.title,
      subtitle: i.subtitle,
      key: secondaryOf(i)?.keybinding ?? undefined,
      icon,
      iconMuted: !icon,
    };
  });
}

/** Section header (ADR-0020): source is the group; headers are not focusable, do not participate in navigation, and do not respond to hover. Fixed height so the ten-row grid can reserve it (ADR-0026). */
function sectionHeaderEl(title: string): HTMLLIElement {
  const li = document.createElement("li");
  li.className =
    "moe-section-header select-none px-3 text-[11px] font-medium uppercase tracking-wider text-fg-subtle";
  li.style.height = `${SECTION_HEADER_HEIGHT}px`;
  li.textContent = title;
  return li;
}

/** Section header inside the About card: the shared card's header type at the About card's row inset (ADR-0026: the two cards duplicate by intent). */
function cardSectionHeaderEl(title: string): HTMLLIElement {
  const li = document.createElement("li");
  li.className =
    "select-none px-2.5 pt-2 pb-0.5 text-[11px] font-medium uppercase tracking-wider text-fg-subtle";
  li.textContent = title;
  return li;
}

/** A single row: hover is an interactive hint (never steals focus) and only applies after real
 * pointer movement — the re-render flicker while keyboard-navigating is gated away in
 * styles.css (.moe-row + data-pointer); only the focused row gets the main highlight. */
function rowEl(e: Row, i: number, focused: boolean): HTMLLIElement {
  const li = document.createElement("li");
  li.className =
    "moe-row group flex cursor-default items-center gap-2.5 rounded-2xl px-3 py-2.5 text-sm transition-colors duration-100 " +
    (focused
      ? "bg-surface-selected text-fg"
      : "text-fg");
  if (focused) li.dataset.focused = "true";
  li.append(
    iconEl(e.icon, {
      className: e.iconMuted
        ? "text-fg-faint"
        : focused
          ? "text-fg"
          : "text-fg-subtle group-hover:text-fg-muted",
    }),
  );
  const left = document.createElement("div");
  left.className = "min-w-0 flex-1 truncate";
  left.textContent = e.title;
  if (e.subtitle) {
    const sub = document.createElement("span");
    sub.className = "ml-2 text-xs text-fg-subtle";
    sub.textContent = e.subtitle;
    left.append(sub);
  }
  li.append(left);
  if (e.key) {
    // Key blocks (shadcn Kbd style): the shared kbdEl component, one block per key (ADR-0015/0030)
    const keys = kbdEl(e.key, { firstOnly: true });
    keys.classList.add("shrink-0");
    // Root page (ADR-0031): reveal on focus/hover only (styles.css `.moe-row-keys`)
    if (e.revealKeys) keys.classList.add("moe-row-keys");
    li.append(keys);
  }
  if (e.badge) {
    // Trailing type badge (ADR-0030): what kind of row this is (Command / AI Command / File…);
    // its text size comes from the shared secondary-text token (styles.css, `.moe-kind-badge`)
    const badge = document.createElement("span");
    badge.className = "moe-kind-badge";
    badge.textContent = e.badge;
    li.append(badge);
  }
  li.addEventListener("mousedown", () => {
    view.update((s) => ({ ...s, focus: i }));
  });
  return li;
}

function render() {
  const v = view.get();
  // The Input Bar's leading slot follows the page layer (Moe logo → back arrow, ADR-0034)
  renderInputIcon();
  // Page shape decides window size (ADR-0018): split pages are wider and taller, others use the default size.
  // Only call IPC when the shape changes, so the window isn't touched on every render.
  const shape = pageShapeOf(
    v.mode === "items" ? v.items[v.focus] : undefined,
    v.detailFull === true,
    v.items.length,
  );
  applyPanelSize(shape);
  let focusedLi: HTMLLIElement | null = null;
  if (v.mode === "commands") {
    // Command layer: grouped by source (headers + rows); the focus index stays on the flat list.
    // Row anatomy (ADR-0030): [icon] command name · extension name [shortcut Kbd] [kind badge].
    // The extension name replaces the details subtitle here — the root page labels sources, not blurbs.
    const lis: HTMLLIElement[] = [];
    let flat = 0;
    for (const section of v.sections) {
      lis.push(sectionHeaderEl(section.title));
      for (const c of section.items) {
        const li = rowEl(
          {
            title: c.title,
            subtitle: c.extensionTitle ?? undefined,
            key: c.keybinding ?? undefined,
            icon: c.icon,
            iconMuted: !c.icon,
            badge: c.kind ?? "Command",
            revealKeys: true,
          },
          flat,
          flat === v.focus,
        );
        if (flat === v.focus) focusedLi = li;
        flat += 1;
        lis.push(li);
      }
    }
    listEl.replaceChildren(...lis);
  } else if (v.mode === "items") {
    const rows = currentEntries().map((e, i) => rowEl(e, i, i === v.focus));
    focusedLi = rows[v.focus] ?? null;
    listEl.replaceChildren(...rows);
  } else {
    // Quick Ask page (ADR-0036): the detail pane is the conversation, the list is away
    listEl.classList.add("hidden");
    detailEl.className = CONVERSATION_PANE_CLASS;
    syncChatHint();
  }
  // Keyboard navigation: the focused row always stays in the viewport (long lists)
  focusedLi?.scrollIntoView({ block: "nearest" });
  if (v.mode !== "chat") renderDetail();
  renderActionBar();
  renderChip();
  updatePlaceholder();
}

/** Input Bar placeholder follows the current layer (Raycast-style: shows whichever command you are in). */
function updatePlaceholder() {
  if (attaching) return; // attachment mode manages its own
  const v = view.get();
  if (v.mode === "commands") {
    q.placeholder = QUERY_PLACEHOLDER;
  } else if (v.mode === "chat") {
    q.placeholder = chatTitle ?? "Ask anything…";
  } else {
    const command = v.commands.find((c) => c.id === v.sourceCommandId);
    q.placeholder = v.sourceTitle ?? command?.title ?? "Results…";
  }
}

// ---- Page shapes (ADR-0018): list / split / detail, one shared layout for all three ----

// Detail container (shared by the split page and full-screen detail): takes the width left by the list, leaving bottom space for the floating action bar.
// The split page's "narrow list + wide detail" comes from the three classes in layout.ts; see there.
const DETAIL_MESSAGE_CLASS = DETAIL_PANE_CLASS;

/** Detail pane header on the split page: the focused item's icon + title + subtitle (same elements as the list row). */
function detailHeaderEl(item: Item): HTMLElement {
  const header = document.createElement("div");
  header.className =
    "mb-3 flex items-center gap-2.5 border-b border-line pb-2.5 text-sm not-prose";
  header.append(
    iconEl(item.icon, { size: 15, className: "shrink-0 text-fg-subtle" }),
  );
  const title = document.createElement("span");
  title.className = "min-w-0 flex-1 truncate font-medium text-fg";
  title.textContent = item.title;
  header.append(title);
  if (item.subtitle) {
    const sub = document.createElement("span");
    sub.className = "shrink-0 text-xs text-fg-subtle";
    sub.textContent = item.subtitle;
    header.append(sub);
  }
  return header;
}

function paintDetail(
  markdown: string,
  itemId: string | null,
  pending: boolean,
  headerItem?: Item,
) {
  const nearBottom =
    detailEl.scrollHeight - detailEl.scrollTop - detailEl.clientHeight < 40;
  const parts = ensureDetailParts();
  const body = parts.body;
  body.replaceChildren();
  if (headerItem) body.append(detailHeaderEl(headerItem));
  const prose = document.createElement("div");
  prose.className = "md";
  prose.innerHTML = DOMPurify.sanitize(marked.parse(markdown, { async: false }));
  body.append(prose);
  // Inline generating indicator (like ChatGPT's loading dots, rather than writing status into the body):
  // body and indicator are separate; streaming events only replace the body, so the dot animation is never restarted
  parts.indicator.classList.toggle("hidden", !pending);
  detailItemId = itemId;
  if (nearBottom) detailEl.scrollTop = detailEl.scrollHeight;
}

interface DetailParts {
  body: HTMLElement;
  indicator: HTMLElement;
}
let detailParts: DetailParts | null = null;

/** Detail body container + fixed inline generating indicator (rebuilt after clearDetail). */
function ensureDetailParts(): DetailParts {
  if (!detailParts || !detailEl.contains(detailParts.body)) {
    const body = document.createElement("div");
    const indicator = generatingEl();
    detailEl.replaceChildren(body, indicator);
    detailParts = { body, indicator };
  }
  return detailParts;
}

function clearDetail() {
  detailMode = "none";
  detailItemId = null;
  detailParts = null;
  detailEl.className = "md hidden";
  detailEl.replaceChildren();
  // Back to single column: the list regains full width (shape decided uniformly by layout.ts)
  listEl.className = LIST_FULL_CLASS;
}

/** Full-screen card (errors, write-back failures, etc.): hides the list. */
function showMessage(markdown: string) {
  detailMode = "message";
  detailEl.className = DETAIL_MESSAGE_CLASS;
  paintDetail(markdown, null, false);
  listEl.classList.add("hidden");
}

/**
 * Detail pane of items mode (ADR-0018): the shape is decided uniformly by `pageShapeOf`;
 * extensions only provide content (item.detail) and never lay it out themselves.
 * - detail: the whole screen is the content (AI answers, notifications); an inline indicator shows at the end of the body while generating;
 * - split : left list + right detail; the detail top carries the focused item's title/subtitle (same icon and copy set as the list).
 */
function renderDetail() {
  const v = view.get();
  const item = v.mode === "items" ? v.items[v.focus] : undefined;
  const shape = pageShapeOf(item, v.detailFull === true, v.items.length);
  if (detailMode === "message" || shape === "list" || !item) {
    if (detailMode !== "message") clearDetail();
    return;
  }
  detailMode = "preview";
  const full = shape === "detail";
  detailEl.className = DETAIL_MESSAGE_CLASS;
  paintDetail(item.detail ?? "", item.id, item.pending === true, full ? undefined : item);
  if (full) {
    listEl.classList.add("hidden");
  } else {
    // Split: the left side becomes the narrow column; detail takes the remaining width
    listEl.className = LIST_NARROW_CLASS;
  }
}

view.subscribe(render);

// ---- Quick Ask page (the panel's conversation, ADR-0036) ----
// A `Conversation` result opens this page: the detail pane holds the message stream, the Input Bar is
// the composer (Enter sends, ⌘N blanks it, ⌃X removes it, ⌃[ / ⌃] step through history), and replies
// stream into the last bubble. The side view shows the same conversations through its own surfaces.

/** The page's DOM (same lazy-rebuild pattern as the detail parts: `clearDetail` detaches it). */
function ensureChatParts(): ChatParts {
  if (!chatParts || !detailEl.contains(chatParts.log)) {
    // The log itself is the scroll container (CONVERSATION_PANE_CLASS is only the column): the
    // conversation view's scrollToEnd targets it, like the Side View's #messages. The hint reads at
    // the top of the blank page (order-first) while the log stretches below it.
    const log = document.createElement("div");
    log.className = "min-h-0 flex-1 space-y-4 overflow-y-auto";
    const error = document.createElement("div");
    error.className =
      "hidden shrink-0 rounded-xl border border-danger-line bg-danger-soft px-3 py-2 text-xs text-danger";
    const hint = document.createElement("div");
    hint.className = "order-first shrink-0 px-1 py-2 text-xs text-fg-subtle";
    hint.textContent =
      "New chat — type a question and press ⏎; ⌃[ / ⌃] step through chat history.";
    detailEl.replaceChildren(log, error, hint);
    chatParts = { log, hint, error, view: createConversationView(log) };
  }
  return chatParts;
}

/** The blank-state hint follows the log: visible while the page shows no message. */
function syncChatHint() {
  const parts = ensureChatParts();
  parts.hint.classList.toggle("hidden", !parts.view.isEmpty());
}

/** Chat state changed outside render(): hint + Input Bar placeholder + action pill. */
function refreshChatChrome() {
  syncChatHint();
  updatePlaceholder();
  renderActionBar();
}

/** Inline error line of the page (failed send, load, delete): never a modal, the page stays usable. */
function setChatError(text: string | null) {
  const parts = ensureChatParts();
  parts.error.classList.toggle("hidden", !text);
  parts.error.textContent = text ?? "";
}

/** The last answer's text (the ⌥⏎ copy / write-back target); null while the page has none. */
function lastAnswerText(): string | null {
  return chatParts?.view.lastAnswer() ?? null;
}

/** A synthesized history item for the delete / materialize / write-back hooks (the Side View does the same). */
function chatItem(conversationId: string) {
  return {
    id: `ai.conversation.${conversationId}`,
    title: chatTitle ?? "",
    actions: [],
    payload: { conversationId },
    pending: false,
  };
}

/** Load the shown conversation into the page; null = the blank new-chat state. */
async function loadChatMessages() {
  const parts = ensureChatParts();
  parts.view.clear();
  setChatError(null);
  chatTitle = null;
  const id = chatConversationId;
  if (!id) {
    refreshChatChrome();
    return;
  }
  try {
    const messages = await invoke<Message[]>("side_messages", { conversationId: id });
    if (chatConversationId !== id) return; // stepped away while loading
    chatTitle = conversationTitle(messages);
    for (const message of messages) parts.view.append(message);
    // An unanswered question means a reply is on its way (the ask that opened the page) — or was
    // interrupted: the page waits in generating mode, so Enter stops instead of stacking a question.
    chatGenerating = messages[messages.length - 1]?.role === "user";
    if (chatGenerating) parts.view.updateStreaming("", true);
    parts.view.scrollToEnd();
  } catch (err) {
    setChatError(`Failed to load the chat: ${String(err)}`);
  }
  refreshChatChrome();
}

/** The chat history list (⌃[ / ⌃] stepping); failures leave stepping empty, the page keeps working. */
async function loadChatConversations() {
  try {
    chatConversations = await invoke<Conversation[]>("side_conversations", { query: null });
  } catch {
    // Ignore
  }
}

/**
 * Enter the Quick Ask page: `conversationId` null = blank new chat; `commandId` = the command whose
 * Apply produced the page (it routes New/Browse/delete/materialize back to its extension).
 */
async function enterChat(conversationId: string | null, commandId?: string) {
  closeConfirm();
  closeActionsCard();
  clearDetail(); // resets the detail pane; the chat parts rebuild on the next render
  chatConversationId = conversationId;
  chatGenerating = false;
  view.update((v) => ({
    ...v,
    mode: "chat",
    items: [],
    focus: 0,
    detailFull: false,
    sourceCommandId: commandId ?? v.sourceCommandId,
  }));
  if (commandId) void refreshExtensionMeta(commandId);
  void loadChatMessages();
  void loadChatConversations();
  q.focus();
}

/** Enter on the page (ADR-0036): send the draft into the conversation and clear the input. */
async function sendChatMessage() {
  const message = q.value.trim();
  const commandId = view.get().sourceCommandId;
  if (!message || chatGenerating || !commandId) return;
  const parts = ensureChatParts();
  setChatError(null);
  q.value = "";
  const row = parts.view.append({ role: "user", content: message });
  chatTitle = chatTitle ?? conversationTitle([{ role: "user", content: message }]);
  parts.view.beginAnswer();
  try {
    chatConversationId = await invoke<string>("panel_send", {
      commandId,
      conversationId: chatConversationId ?? "",
      message,
    });
    chatGenerating = true;
    parts.view.updateStreaming("", true);
    parts.view.scrollToEnd();
    refreshChatChrome();
    // The conversation appears in the history list (and in side_conversations) right away
    void loadChatConversations();
  } catch (err) {
    // Rollback: the message was not persisted; restore the draft so the user does not retype it.
    row.remove();
    q.value = message;
    setChatError(String(err));
    chatGenerating = false;
    refreshChatChrome();
  }
}

/** Stop the stream (Enter while generating, or the dialog): the generated part is kept (IIE4AD-365). */
async function stopChatGeneration() {
  try {
    await invoke<number>("stop_generation");
  } catch (err) {
    setChatError(String(err));
  }
  chatGenerating = false;
  ensureChatParts().view.settleStreaming();
  refreshChatChrome();
}

/**
 * ⌃[ / ⌃] on the page: step through chat history, like the Side View — `⌃[` is Back (the previous,
 * older conversation), `⌃]` is Forward (the newer one). The list is newest-first, so backward is +1
 * in the array; a conversation outside it (a blank new chat) sits at "now": backward lands on the
 * newest, forward goes nowhere. Both ends clamp.
 */
async function stepChat(backward: boolean) {
  if (chatConversations.length === 0) await loadChatConversations();
  if (chatConversations.length === 0) return;
  const index = chatConversations.findIndex((c) => c.id === chatConversationId);
  const delta = backward ? 1 : -1;
  const next =
    index === -1
      ? backward
        ? 0
        : -1
      : Math.min(Math.max(index + delta, 0), chatConversations.length - 1);
  const target = chatConversations[next];
  if (!target || target.id === chatConversationId) return;
  await enterChat(target.id);
}

/** Remove Chat (⌃X on the page / the actions card): the same deletion as the chat history page (ADR-0022). */
async function removeCurrentChat() {
  const id = chatConversationId;
  if (!id) {
    toast("This is already a new chat", "alert");
    return;
  }
  try {
    const count = await invoke<number>("delete_item", {
      commandId: "ai.search-history",
      item: chatItem(id),
    });
    if (!count) {
      toast("No chat to remove", "alert");
      return;
    }
    toast("Removed chat");
    await enterChat(null);
  } catch (err) {
    setChatError(String(err));
  }
}

/** ⌃⇧X on the page: remove every conversation (the platform's DeleteAll slot, ADR-0022). */
async function deleteAllChats() {
  try {
    const count = await invoke<number>("delete_all", { commandId: "ai.search-history" });
    if (!count) {
      toast("No chats to remove", "alert");
      return;
    }
    toast(`Removed ${count} chats`);
    await enterChat(null);
  } catch (err) {
    setChatError(String(err));
  }
}

/** ⌘J / the actions card: open the shown conversation in the Side View. */
async function materializeChat() {
  const id = chatConversationId;
  const commandId = view.get().sourceCommandId;
  if (!id || !commandId) return;
  try {
    const res = await invoke<ActionResult>("run_item_action", {
      commandId,
      item: chatItem(id),
      action: { id: "materialize", title: "Open in Side View", kind: "secondary" },
    });
    applyResult(res, commandId, false, view.get().sourceIcon, view.get().sourceTitle);
  } catch (err) {
    setChatError(String(err));
  }
}

/** Run one answer action (⌥⏎ copy / write back) on the last answer, through the Extension's hook. */
async function runChatAnswerAction(actionId: "copy" | "write-back", title: string) {
  const commandId = view.get().sourceCommandId;
  const text = lastAnswerText();
  if (!text || !commandId) return;
  const item = {
    id: "ai.answer",
    title: "AI Answer",
    actions: [],
    payload: { conversationId: chatConversationId },
    detail: text,
    pending: false,
  };
  try {
    const res = await invoke<ActionResult>("run_item_action", {
      commandId,
      item,
      action: { id: actionId, title, kind: actionId === "copy" ? "secondary" : "primary" },
    });
    if (actionId === "copy") toast("Copied");
    applyResult(res, commandId, false, view.get().sourceIcon, view.get().sourceTitle);
  } catch (err) {
    setChatError(String(err));
  }
}

/** ⌥⏎: copy the last answer (clipboard only, never the host app — ADR-0002 amendment). */
async function copyLastAnswer() {
  await runChatAnswerAction("copy", "Copy");
}

/** Write the last answer back into the host app (the actions card row; the page's Enter sends instead). */
async function writeBackLastAnswer() {
  await runChatAnswerAction("write-back", "Write Back");
}

/**
 * The stop-confirmation dialog (ADR-0036/0038): one generic component for every streaming surface —
 * Back only asks, the action it carries is the surface's own stop (Quick Ask page or result card).
 */
function openConfirm(options: { text: string; ok: string; cancel: string; action: () => void }) {
  if (confirmOpen) return;
  confirmOpen = true;
  confirmAction = options.action;
  confirmTextEl.textContent = options.text;
  confirmOkEl.replaceChildren(
    document.createTextNode(options.ok),
    kbdEl("⏎", { firstOnly: true }),
  );
  confirmCancelEl.replaceChildren(
    document.createTextNode(options.cancel),
    kbdEl("Esc", { firstOnly: true }),
  );
  confirmCardEl.classList.remove("hidden");
}

/** The "Back while generating" ask shared by the Quick Ask page and the result pages (ADR-0038). */
function openStopConfirm(action: () => void) {
  openConfirm({
    text: "Stop generating? The generated part is kept.",
    ok: "Stop Generation",
    cancel: "Keep Generating",
    action,
  });
}

function closeConfirm() {
  if (!confirmOpen) return;
  confirmOpen = false;
  confirmAction = null;
  confirmCardEl.classList.add("hidden");
  q.focus();
}

confirmOkEl.addEventListener("click", () => {
  const action = confirmAction;
  closeConfirm();
  action?.();
});
confirmCancelEl.addEventListener("click", () => closeConfirm());
// The dialog buttons must not steal the input focus (the same rule as the action pill)
for (const button of [confirmOkEl, confirmCancelEl]) {
  button.addEventListener("mousedown", (e) => e.preventDefault());
}

// ---- Semantic actions ----

function primaryOf(item: Item) {
  return item.actions.find((a) => a.kind === "primary") ?? item.actions[0];
}
function secondaryOf(item: Item) {
  return item.actions.find((a) => a.kind === "secondary");
}

function applyResult(
  res: ActionResult,
  commandId: string,
  live = false,
  icon?: string,
  title?: string,
) {
  if (typeof res === "string") {
    // silent: no UI action needed (the backend already opened the side view window)
    return;
  }
  if ("writeBack" in res) {
    // Write-back is done by the backend (dismiss panel → AX / clipboard fallback delivery); the panel is already hidden by now
    return;
  }
  if ("openSideView" in res) {
    // The side view window is shown by the backend and receives the payload; the panel is already hidden
    return;
  }
  if ("conversation" in res) {
    // The panel's conversation page (Quick Ask, ADR-0036). The text that found this command is never
    // a message: the page owns the Input Bar from here (the capturing row's text already went out).
    q.value = "";
    void enterChat(res.conversation.conversationId ?? null, commandId);
    return;
  }
  if ("list" in res) {
    const items = res.list.items;
    closeActionsCard();
    clearDetail();
    view.update((v) => ({
      ...v,
      mode: "items",
      items,
      focus: 0,
      sourceCommandId: commandId,
      sourceLive: live,
      sourceIcon: icon,
      sourceTitle: title,
      detailFull: res.list.detailFull === true,
    }));
    // The avatar chip follows the extension now entered (ADR-0026)
    void refreshExtensionMeta(commandId);
  }
}

/**
 * Generic actions (ADR-0014): Browse (⌘P) opens the current Extension's record list,
 * New (⌘N) creates a new record. The platform only sets bindings and routing; entries
 * are declared by the Extension; when none is declared, show an inline hint instead of failing silently.
 */
async function openEntry(kind: "browse" | "new") {
  const v = view.get();
  // Current Extension: the focused command on the command layer, the page's source command once inside
  const from = v.mode === "commands" ? v.commands[v.focus]?.id : v.sourceCommandId;
  if (!from) return;
  let command: CommandMeta | null;
  try {
    command = await invoke<CommandMeta | null>("entry_command", {
      commandId: from,
      kind,
    });
  } catch (err) {
    showMessage(`Failed to run: ${String(err)}`);
    return;
  }
  if (!command) {
    toast(kind === "browse" ? "This extension has no record list" : "This extension has no New entry");
    return;
  }
  try {
    const res = await invoke<ActionResult>("invoke_command", {
      commandId: command.id,
      query: null,
    });
    // An entry never consumes the search text: entering its records page clears the bar (ADR-0038),
    // same as applyFocused — typing there searches commands again.
    q.value = "";
    applyResult(res, command.id, command.live, command.icon, command.title);
  } catch (err) {
    showMessage(`Failed to run: ${String(err)}`);
  }
}

/**
 * Apply one command from the command layer (Enter and the Open Command row share this path,
 * including frecency recording and live-command input takeover).
 */
async function applyCommand(cmd: CommandMeta) {
  // The Input Bar text is a command's parameter only when the row declares Query (ADR-0036): the
  // capturing row (the no-match fallback) consumes it; a Live row takes the input over; every other
  // row is applied without it — text used to search is never content, and the page it opens clears
  // the bar (ADR-0038): from there the input is a search box again, not the page's state.
  const query = cmd.live || cmd.input !== "query" ? null : q.value;
  try {
    const res = await invoke<ActionResult>("invoke_command", {
      commandId: cmd.id,
      query,
    });
    if (cmd.live || cmd.input !== "query") q.value = "";
    applyResult(res, cmd.id, cmd.live, cmd.icon, cmd.title);
  } catch (err) {
    showMessage(`Failed to run: ${String(err)}`);
  }
}

async function applyFocused(alt: boolean) {
  const v = view.get();
  if (v.mode === "commands") {
    const cmd = v.commands[v.focus];
    if (!cmd) return;
    await applyCommand(cmd);
    return;
  }
  if (v.mode === "items") {
    const item = v.items[v.focus];
    const action = alt
      ? (secondaryOf(item) ?? primaryOf(item))
      : primaryOf(item);
    if (!item || !action || !v.sourceCommandId) return;
    try {
      const res = await invoke<ActionResult>("run_item_action", {
        commandId: v.sourceCommandId,
        item,
        action,
      });
      if (action.id === "copy") toast("Copied");
      applyResult(res, v.sourceCommandId, v.sourceLive, v.sourceIcon, v.sourceTitle);
    } catch (err) {
      showMessage(`Failed to run: ${String(err)}`);
    }
    return;
  }
}

function move(delta: number) {
  view.update((v) => {
    const len = v.mode === "commands" ? v.commands.length : v.items.length;
    return { ...v, focus: len === 0 ? 0 : (v.focus + delta + len) % len };
  });
}

// ---- Actions card (ADR-0037): registered sources, loaded per page ----

/** The page the actions card is opening for: what every source's rows key on. */
function panelPageContext(): PageContext {
  const v = view.get();
  return {
    surface: "panel",
    mode: v.mode,
    item: v.mode === "items" ? v.items[v.focus] : undefined,
    commandId: currentCommandId(),
    suggestion: isSuggestionFocus(v),
  };
}

/**
 * "Command" (ADR-0029): the page command's own rows — one menu for the root and for inside a command.
 * Open Command leads on the root (the same Apply path as Enter); inside a command the row is omitted
 * ("opening" the view you are already in would reset it). Then the favorite toggle, Remove from
 * Suggestions on a Suggestions row (⌃X, ADR-0025), and the Configure Extension placeholder.
 */
async function commandCardRows(ctx: PageContext): Promise<CardRow[]> {
  if (!ctx.commandId) return [];
  const commandId = ctx.commandId;
  let favorited = false;
  try {
    favorited = await invoke<boolean>("is_favorite", { commandId });
  } catch {
    // Ignore: the toggle still works, the label just may be stale
  }
  const rows: CardRow[] = [];
  if (ctx.mode === "commands") {
    rows.push({
      title: "Open Command",
      icon: "corner-down-left",
      keys: keyDisplay.get("apply") ?? "⏎",
      run: () => {
        const cmd = view.get().commands.find((c) => c.id === commandId);
        if (cmd) void applyCommand(cmd);
      },
    });
  }
  rows.push({
    title: favorited ? "Remove from Favorites" : "Add to Favorites",
    icon: "star",
    keys: GENERAL_KEY_LABELS.favorite,
    run: () => void toggleFavorite(commandId),
  });
  if (ctx.suggestion) {
    rows.push({
      title: "Remove from Suggestions",
      icon: "trash-2",
      keys: GENERAL_KEY_LABELS.delete,
      run: () => void deleteSuggestion(commandId),
    });
  }
  rows.push({ title: "Configure Extension", icon: "settings-2", disabled: true, run: () => {} });
  return rows;
}

/**
 * "Actions": the object's own actions. A result page's object is its focused item — the shape
 * (list / split / detail) never changes it — while a chat page's object is the conversation.
 */
function objectCardRows(ctx: PageContext): CardRow[] {
  if (ctx.mode === "chat") {
    const hasAnswer = lastAnswerText() !== null;
    return [
      {
        title: "Open in Side View",
        icon: "panel-right",
        keys: keyDisplay.get("materialize") ?? "⌘J",
        disabled: !chatConversationId,
        run: () => void materializeChat(),
      },
      {
        title: "Copy Last Answer",
        icon: "copy",
        keys: keyDisplay.get("secondaryCopy") ?? "⌥⏎",
        disabled: !hasAnswer,
        run: () => void copyLastAnswer(),
      },
      {
        title: "Write Back Last Answer",
        icon: "corner-down-left",
        disabled: !hasAnswer,
        run: () => void writeBackLastAnswer(),
      },
    ];
  }
  const item = ctx.item;
  if (!item) return [];
  // The item's own declaration (ADR-0006): the first action carries the Apply semantic (⏎).
  return item.actions.map((action) => ({
    title: action.title,
    icon: action.kind === "primary" ? "corner-down-left" : "copy",
    keys: action.keybinding ?? (action.kind === "primary" ? "⏎" : null),
    run: () => void runItemAction(item, action),
  }));
}

/**
 * "General" (ADR-0014/0022): the platform general slots as they are live on this page — Browse ⌘P /
 * New ⌘N resolved from the Extension's declared entries, plus the delete slots where the page owns
 * records (a result page, or the chat page as Remove Chat / Remove All Chats, ADR-0036).
 */
async function pageCardRows(ctx: PageContext): Promise<CardRow[]> {
  const rows: CardRow[] = [];
  if (ctx.commandId) {
    const kinds: EntryAction[] = ["browse", "new"];
    const found = await Promise.all(
      kinds.map((kind) =>
        invoke<CommandMeta | null>("entry_command", {
          commandId: ctx.commandId,
          kind,
        }).catch(() => null),
      ),
    );
    const titles = ctx.mode === "chat"
      ? { browse: "Browse Chats", new: "New Chat" }
      : { browse: "Browse Records", new: "New Record" };
    kinds.forEach((kind, index) => {
      if (!found[index]) return;
      rows.push({
        title: titles[kind],
        icon: kind === "browse" ? "history" : "plus",
        keys: GENERAL_KEY_LABELS[kind],
        run: () => void openEntry(kind),
      });
    });
  }
  if (ctx.mode === "items") {
    rows.push({
      title: "Delete Record",
      icon: "trash-2",
      keys: GENERAL_KEY_LABELS.delete,
      run: () => void deleteFocused(false),
    });
    rows.push({
      title: "Delete All Records",
      icon: "trash-2",
      keys: GENERAL_KEY_LABELS.deleteAll,
      run: () => void deleteFocused(true),
    });
  } else if (ctx.mode === "chat") {
    rows.push({
      title: "Remove Chat",
      icon: "trash-2",
      keys: GENERAL_KEY_LABELS.delete,
      disabled: !chatConversationId,
      run: () => void removeCurrentChat(),
    });
    rows.push({
      title: "Remove All Chats",
      icon: "trash-2",
      keys: GENERAL_KEY_LABELS.deleteAll,
      run: () => void deleteAllChats(),
    });
  }
  return rows;
}

/** The panel's card (ADR-0037): Actions → Command → General, loaded for every page kind. */
const panelActionPlan = createActionPlan([
  { title: "Actions", when: (ctx) => ctx.mode !== "commands", rows: objectCardRows },
  { title: "Command", when: (ctx) => ctx.commandId !== undefined, rows: commandCardRows },
  { title: "General", when: () => true, rows: pageCardRows },
]);

/**
 * Open the actions card (Raycast-style floating card, Show All Actions ⌘K): the registered sources
 * for the current page, one section each — the call site no longer merges rows by page kind
 * (ADR-0037). The body (command list / results) stays in place behind the card.
 */
async function openActionsCard() {
  closeAboutCard(); // the two cards are peers: opening one closes the other (like openAboutCard)
  actionsCard.open(await panelActionPlan.sections(panelPageContext()));
}

/** Run one of the object's declared actions through the Extension's hook (result pages). */
async function runItemAction(item: Item, action: Action) {
  const v = view.get();
  if (!v.sourceCommandId) return;
  try {
    const res = await invoke<ActionResult>("run_item_action", {
      commandId: v.sourceCommandId,
      item,
      action,
    });
    if (action.id === "copy") toast("Copied");
    applyResult(res, v.sourceCommandId, v.sourceLive, v.sourceIcon, v.sourceTitle);
  } catch (err) {
    showMessage(`Failed to run: ${String(err)}`);
  }
}

/**
 * The command-level rows (ADR-0029): Open Command (root only, = Apply), the favorite toggle,
 * and the Configure Extension placeholder.
 */
/** The command a command-level action acts on: the focused one at the root, the source command inside a command. */
function currentCommandId(): string | undefined {
  const v = view.get();
  if (v.mode === "commands") return v.commands[v.focus]?.id;
  return v.sourceCommandId;
}

/**
 * Toggle a command's favorite state (ADR-0029): shared by the card row and the ⌘⇧F binding.
 * On the root layer the list re-runs so the Favorites section updates immediately.
 */
async function toggleFavorite(commandId: string | undefined) {
  if (!commandId) return;
  const v = view.get();
  closeActionsCard(); // its favorite label is stale now; reopening re-reads it
  try {
    const now = await invoke<boolean>("toggle_favorite", { commandId });
    toast(now ? "Added to Favorites" : "Removed from Favorites");
    if (v.mode === "commands") await refresh(q.value);
  } catch (err) {
    toast(`Failed: ${String(err)}`, "alert");
  }
}

/** Close the actions card; returning focus to the Input Bar is the card's onClose hook. */
function closeActionsCard() {
  actionsCard.close();
}

async function materialize() {
  const v = view.get();
  if (v.mode === "chat") {
    await materializeChat();
    return;
  }
  const item = v.mode === "items" ? v.items[v.focus] : undefined;
  const action = item?.actions.find((a) => a.id === "materialize" || a.keybinding === "⌘J");
  if (item && action && v.sourceCommandId) {
    const res = await invoke<ActionResult>("run_item_action", {
      commandId: v.sourceCommandId,
      item,
      action,
    });
    applyResult(res, v.sourceCommandId, v.sourceLive, v.sourceIcon, v.sourceTitle);
  }
}

// ---- About card (ADR-0026): the avatar chip's menu, same UX as the actions card (sections, input at the bottom) ----

/** Feedback lands in the repository's issue tracker. */
const FEEDBACK_URL = "https://github.com/limoiie/moe/issues";

interface AboutRow {
  id: string;
  title: string;
  icon: string;
  /** Kbd display string (e.g. "⌘,"); rows without a platform binding show none. */
  keys?: string;
  /** The row's state is on (the theme rows): a trailing check marks it. */
  active?: boolean;
  run: () => void;
}

/** One group inside the About card (ADR-0026 amendment): the app's meta actions under a header. */
interface AboutSection {
  title: string;
  rows: AboutRow[];
}

let aboutFocus = 0;
let aboutCardOpen = false;

/** Appearance rows (ADR-0035 amendment): System follows the OS; the choice persists to config.toml. */
const THEME_ROWS: { value: ThemePreference; title: string; icon: string }[] = [
  { value: "system", title: "System", icon: "monitor" },
  { value: "light", title: "Light", icon: "sun" },
  { value: "dark", title: "Dark", icon: "moon" },
];

/** The About menu: the app's own meta actions (ADR-0026), grouped into App / Theme / Support sections (ADR-0026/0035 amendments). */
function aboutSections(): AboutSection[] {
  return [
    {
      title: "App",
      rows: [
        {
          id: "about.config",
          title: "Open Config File",
          icon: "settings-2",
          keys: GENERAL_KEY_LABELS.openConfig,
          run: () => void runAboutConfig(),
        },
        {
          id: "about.key",
          title: "Save AI Key",
          icon: "key-round",
          run: () => void runAboutKey(),
        },
      ],
    },
    {
      title: "Theme",
      rows: THEME_ROWS.map((option) => ({
        id: `about.theme.${option.value}`,
        title: option.title,
        icon: option.icon,
        active: themePreference() === option.value,
        run: () => void runAboutTheme(option.value),
      })),
    },
    {
      title: "Support",
      rows: [
        {
          id: "about.feedback",
          title: "Send Feedback",
          icon: "megaphone",
          run: () => void runAboutFeedback(),
        },
      ],
    },
  ];
}

async function runAboutConfig() {
  closeAboutCard();
  await openConfigFile();
}

/** Open the config file (the `moe.open-config` command); shared by the About row and the ⌘, binding. */
async function openConfigFile() {
  try {
    await invoke("invoke_command", {
      commandId: "moe.open-config",
      query: null,
      record: false,
    });
    toast("Opened config file");
  } catch (err) {
    showMessage(`Failed to run: ${String(err)}`);
  }
}

/** Save AI Key runs the same command as the list: its guidance card lands in the results layer. */
async function runAboutKey() {
  closeAboutCard();
  try {
    const res = await invoke<ActionResult>("invoke_command", {
      commandId: "moe.set-ai-key",
      query: null,
      record: false,
    });
    applyResult(res, "moe.set-ai-key", false, "key-round", "Moe: Save AI Key");
  } catch (err) {
    showMessage(`Failed to run: ${String(err)}`);
  }
}

async function runAboutFeedback() {
  closeAboutCard();
  try {
    await invoke("open_external", { url: FEEDBACK_URL });
    toast("Opened the feedback page");
  } catch (err) {
    toast(`Failed to open: ${String(err)}`, "alert");
  }
}

/**
 * Switch the appearance (ADR-0035 amendment): Rust persists it to config.toml and broadcasts
 * (both windows re-theme); the card stays open — a theme is worth trying twice — and its check
 * follows the new preference.
 */
async function runAboutTheme(theme: ThemePreference) {
  const label = THEME_ROWS.find((option) => option.value === theme)?.title ?? theme;
  try {
    await invoke("set_theme", { theme });
    toast(`Theme: ${label}`);
  } catch (err) {
    toast(`Failed to set theme: ${String(err)}`, "alert");
  }
  if (aboutCardOpen) renderAboutCard();
}

/** Filter rows by title (case-insensitive); a section whose rows all filtered out disappears. */
function filteredAboutSections(): AboutSection[] {
  const needle = aboutSearchEl.value.trim().toLowerCase();
  if (!needle) return aboutSections();
  return aboutSections()
    .map((section) => ({
      title: section.title,
      rows: section.rows.filter((row) => row.title.toLowerCase().includes(needle)),
    }))
    .filter((section) => section.rows.length > 0);
}

/** The card's rows in navigation order: the flat concatenation of its sections (focus indexes this list). */
function filteredAboutRows(): AboutRow[] {
  return filteredAboutSections().flatMap((section) => section.rows);
}

function aboutRowEl(row: AboutRow, focused: boolean): HTMLLIElement {
  const li = document.createElement("li");
  li.className =
    "moe-row flex cursor-default items-center gap-2.5 rounded-xl px-2.5 py-1.5 text-sm transition-colors duration-100 " +
    (focused
      ? "bg-surface-selected text-fg"
      : "text-fg");
  if (focused) li.dataset.focused = "true";
  li.append(iconEl(row.icon, { size: 15, className: focused ? "text-fg" : "text-fg-subtle" }));
  const title = document.createElement("span");
  title.className = "min-w-0 flex-1 truncate";
  title.textContent = row.title;
  li.append(title);
  if (row.keys) {
    const keys = kbdEl(row.keys, { firstOnly: true });
    keys.classList.add("shrink-0");
    li.append(keys);
  }
  // The state rows (themes): a trailing check marks the active one — shape, not color alone
  if (row.active) {
    li.append(iconEl("check", { size: 14, className: "shrink-0 text-accent" }));
  }
  li.addEventListener("click", () => row.run());
  return li;
}

function renderAboutCard() {
  const sections = filteredAboutSections();
  const rows = sections.flatMap((section) => section.rows);
  aboutFocus = rows.length === 0 ? 0 : Math.min(aboutFocus, rows.length - 1);
  if (rows.length === 0) {
    const empty = document.createElement("li");
    empty.className = "px-2 py-3 text-xs text-fg-subtle";
    empty.textContent = "No matching actions";
    aboutListEl.replaceChildren(empty);
    return;
  }
  // Raycast-style: while filtering, matches render as one flat list without headers; headers return with the cleared input.
  const showHeaders = aboutSearchEl.value.trim() === "";
  const lis: HTMLLIElement[] = [];
  let flat = 0;
  let focusedLi: HTMLLIElement | null = null;
  for (const section of sections) {
    if (showHeaders) lis.push(cardSectionHeaderEl(section.title));
    for (const row of section.rows) {
      const li = aboutRowEl(row, flat === aboutFocus);
      if (flat === aboutFocus) focusedLi = li;
      lis.push(li);
      flat += 1;
    }
  }
  aboutListEl.replaceChildren(...lis);
  focusedLi?.scrollIntoView({ block: "nearest" });
}

function moveAboutFocus(delta: number) {
  const rows = filteredAboutRows();
  if (rows.length === 0) return;
  aboutFocus = (aboutFocus + delta + rows.length) % rows.length;
  renderAboutCard();
}

function openAboutCard() {
  closeActionsCard();
  aboutCardOpen = true;
  aboutSearchEl.value = "";
  aboutFocus = 0;
  aboutCardEl.classList.remove("hidden");
  renderAboutCard();
  aboutSearchEl.focus();
}

function closeAboutCard() {
  if (!aboutCardOpen) return;
  aboutCardOpen = false;
  aboutCardEl.classList.add("hidden");
  q.focus();
}

function toggleAboutCard() {
  if (aboutCardOpen) closeAboutCard();
  else openAboutCard();
}

chipEl.addEventListener("mousedown", (e) => e.preventDefault());
chipEl.addEventListener("click", () => toggleAboutCard());
aboutSearchEl.addEventListener("input", () => {
  aboutFocus = 0;
  renderAboutCard();
});
// The about card owns its keys while its input is focused (same UX as the actions card)
aboutSearchEl.addEventListener("keydown", (e) => {
  const up =
    e.key === "ArrowUp" ||
    (e.ctrlKey && !e.metaKey && !e.shiftKey && e.key.toLowerCase() === "p");
  const down =
    e.key === "ArrowDown" ||
    (e.ctrlKey && !e.metaKey && !e.shiftKey && e.key.toLowerCase() === "n");
  if (up || down) {
    e.preventDefault();
    e.stopPropagation();
    moveAboutFocus(down ? 1 : -1);
  } else if (e.key === "Enter" && !e.isComposing) {
    e.preventDefault();
    e.stopPropagation();
    filteredAboutRows()[aboutFocus]?.run();
  } else if (e.key === "Escape" || e.key === "Backspace") {
    // The front-most card owns these keys: the window-level "empty Backspace = Back" must
    // never see them (otherwise deleting a filter char would close the card)
    e.stopPropagation();
    if (e.key === "Escape" || (aboutSearchEl.value === "" && !e.repeat)) {
      e.preventDefault();
      closeAboutCard();
    }
  }
});

/** Live commands: re-run the list with the new query as input changes (e.g. "AI: Search Chat History"). */
async function rerunLive(query: string) {
  const v = view.get();
  if (!v.sourceCommandId) return;
  try {
    const res = await invoke<ActionResult>("invoke_command", {
      commandId: v.sourceCommandId,
      query: query || null,
      record: false, // a re-run is not a launch (frecency semantics)
    });
    applyResult(res, v.sourceCommandId, true, v.sourceIcon, v.sourceTitle);
  } catch (err) {
    showMessage(`Failed to run: ${String(err)}`);
  }
}

/** Delete slot (ADR-0022): ⌃X deletes the focused record, ⌃⇧X deletes all; re-runs the current list on success. */
async function deleteFocused(all: boolean) {
  const v = view.get();
  if (!v.sourceCommandId) return;
  const item = v.items[v.focus];
  if (!all && !item) return;
  try {
    const count = all
      ? await invoke<number>("delete_all", { commandId: v.sourceCommandId })
      : await invoke<number>("delete_item", {
          commandId: v.sourceCommandId,
          item,
        });
    if (!count) {
      toast("No records to delete", "alert");
      return;
    }
    toast(all ? `Deleted ${count} records` : "Deleted 1 record");
    if (v.sourceLive) {
      await rerunLive(q.value);
    } else {
      const res = await invoke<ActionResult>("invoke_command", {
        commandId: v.sourceCommandId,
        query: null,
        record: false,
      });
      applyResult(res, v.sourceCommandId, v.sourceLive, v.sourceIcon, v.sourceTitle);
    }
  } catch (err) {
    toast(`Delete failed: ${String(err)}`, "alert");
  }
}

/** Whether the focused root row sits in the Suggestions section (ADR-0025): the delete slot's scope. */
function isSuggestionFocus(v: View): boolean {
  return (
    v.mode === "commands" &&
    v.suggestionsStart >= 0 &&
    v.focus >= v.suggestionsStart &&
    v.focus < v.suggestionsStart + v.suggestionsCount
  );
}

/** Forget one recently used command (⌃X on a suggestion, ADR-0025). */
async function deleteSuggestion(commandId: string | undefined) {
  if (!commandId) return;
  try {
    await invoke<boolean>("delete_suggestion", { commandId });
    toast("Removed from suggestions");
    await refresh(q.value);
  } catch (err) {
    toast(`Delete failed: ${String(err)}`, "alert");
  }
}

/** Clear all recently used commands (⌃⇧X on suggestions, ADR-0025). */
async function clearSuggestions() {
  try {
    const count = await invoke<number>("clear_suggestions");
    toast(count ? `Forgot ${count} recent commands` : "No suggestions to clear", count ? "check" : "alert");
    await refresh(q.value);
  } catch (err) {
    toast(`Delete failed: ${String(err)}`, "alert");
  }
}

// Esc / empty-input Backspace layered back: confirm dialog → stop generation → actions card → result page → clear input → (may close panel)
// Empty Backspace passes quit:false: the root layer stays in place and does not close the panel (closing the panel belongs to Esc alone)
async function back(options: { quit?: boolean } = {}) {
  const { quit = true } = options;
  const v = view.get();
  // The stop-confirmation dialog is the front-most layer (ADR-0036/0038; its own keys are handled above)
  if (confirmOpen) {
    closeConfirm();
    return;
  }
  // The Quick Ask page (ADR-0036): back while generating asks first (Enter stops directly); a draft
  // clears first (the platform-wide input layering); otherwise back to the command layer.
  if (v.mode === "chat") {
    if (chatGenerating) {
      openStopConfirm(() => void stopChatGeneration());
      return;
    }
    if (q.value) {
      q.value = "";
      renderActionBar();
      return;
    }
    clearDetail(); // the chat parts detach with the pane (they rebuild on the next entry)
    view.update((s) => ({ ...s, mode: "commands", items: [], focus: 0 }));
    // The page cleared the input when it took over: the root page (empty query) matches it now.
    void refresh("");
    return;
  }
  // A streaming result page asks first too (ADR-0038): an accidental Back must not silently kill a
  // generation — Enter and the pill's Stop row stop immediately
  if (v.mode === "items" && v.items[v.focus]?.pending) {
    openStopConfirm(() => void stopGeneration());
    return;
  }
  // The actions card is an overlay: close it first, then back out further (Raycast-style)
  if (actionsCard.isOpen()) {
    closeActionsCard();
    return;
  }
  // Same for the about card (ADR-0026)
  if (aboutCardOpen) {
    closeAboutCard();
    return;
  }
  // Result pages back out in one step (ADR-0038): the full-screen detail is the page itself, the row
  // list behind it is not a stop on the way, and the page cleared the Input Bar when it opened — so
  // the parent is the fresh root (empty query), never the stale search from the way in.
  if (v.mode !== "commands") {
    await refresh("");
    return;
  }
  if (q.value) {
    q.value = "";
    await refresh("");
    return;
  }
  if (!quit) return; // Root layer: empty Backspace stops here
  await invoke("hide_panel");
}

async function refresh(query: string) {
  clearDetail();
  const sections = await invoke<CommandSection[]>("search_commands", { query });
  // The Suggestions section's real place in the flat list (ADR-0025): it sits under Favorites
  // when those exist, so the delete slot and the actions card need its range, not just a count.
  const suggestionsIndex = sections.findIndex((s) => s.title === "Suggestions");
  view.update((v) => ({
    ...v,
    mode: "commands",
    sections,
    // Focus/navigation is based on the flat list; sections only render headers.
    commands: sections.flatMap((s) => s.items),
    suggestionsStart:
      suggestionsIndex === -1
        ? -1
        : sections.slice(0, suggestionsIndex).reduce((flat, s) => flat + s.items.length, 0),
    suggestionsCount: suggestionsIndex === -1 ? 0 : sections[suggestionsIndex].items.length,
    focus: 0,
  }));
}

/** Stop in-progress generation (platform-wide: not per extension/conversation, IIE4AD-365). */
async function stopGeneration() {
  try {
    const stopped = await invoke<number>("stop_generation");
    toast(stopped > 0 ? "Generation stopped" : "No generation in progress");
  } catch (err) {
    showMessage(`Failed to stop: ${String(err)}`);
  }
}

// ---- Attachment input (⌘⇧A): reuses the same Input Bar input path; Enter inserts an `@"path"` mention (ADR-0010) ----

const attachBarEl = document.querySelector<HTMLDivElement>("#attach")!;
const attachTextEl = document.querySelector<HTMLSpanElement>("#attach-text")!;
const QUERY_PLACEHOLDER = "Search commands…";
const ATTACH_BAR_BASE =
  "items-center justify-between gap-3 border-b px-4 py-2 text-xs";

let attaching = false;
let savedQuery = "";
let attachBarToken = 0;

/** Input Bar leading icon: search normally, paperclip in attachment mode (ADR-0012). */
/** Last rendered leading-icon kind: mode changes rebuild it, keystrokes don't. */
let inputIconKind: "attach" | "back" | "logo" | null = null;

/**
 * The Input Bar's leading slot: the square-m mark on the root page, a back arrow on nested pages
 * (click = Back; the keyboard paths stay Esc / empty ⌫), a paperclip while attaching. The glyph
 * is one column with the item rows' icons — 16px wide, centered — while its size and stroke come
 * from the --moe-input-icon-* tokens, so it reads bigger and bolder without breaking the grid.
 */
function renderInputIcon() {
  const kind: "attach" | "back" | "logo" = attaching
    ? "attach"
    : view.get().mode === "commands"
      ? "logo"
      : "back";
  if (kind === inputIconKind) return;
  inputIconKind = kind;
  inputIconEl.classList.toggle("moe-input-icon-back", kind === "back");
  // The root page carries the brand mark (inline SVG, currentColor); nested pages the back arrow,
  // attachment mode the paperclip (ADR-0034 amendment).
  inputIconEl.replaceChildren(
    kind === "attach"
      ? iconEl("paperclip", { className: "moe-input-glyph text-fg-muted" })
      : kind === "back"
        ? iconEl("arrow-left", { className: "moe-input-glyph text-fg-muted" })
        : logoEl("moe-input-glyph text-fg"),
  );
}

// The leading slot doubles as Back on nested pages (click; the keyboard paths are Esc / empty ⌫)
inputIconEl.addEventListener("click", () => {
  if (!attaching && view.get().mode !== "commands") void back({ quit: false });
});

function setAttachBar(kind: "hint" | "error", text: string) {
  attachTextEl.textContent = text;
  const palette =
    kind === "error"
      ? "border-danger-line bg-danger-soft text-danger"
      : "border-accent-line bg-accent-soft text-accent";
  attachBarEl.className = `${ATTACH_BAR_BASE} flex ${palette}`;
}

function hideAttachBar() {
  attachBarEl.className = `${ATTACH_BAR_BASE} hidden`;
}

function startAttach() {
  if (attaching) return;
  attaching = true;
  renderInputIcon();
  savedQuery = q.value;
  q.value = "";
  q.placeholder = "Paste a file path, Enter to attach";
  setAttachBar(
    "hint",
    "Attachment mode: paste a file path, Enter to add (Esc cancels; ~ and paths with spaces are supported)",
  );
  q.focus();
}

function cancelAttach() {
  if (!attaching) return;
  attaching = false;
  renderInputIcon();
  q.value = savedQuery;
  savedQuery = "";
  updatePlaceholder();
  hideAttachBar();
}

async function submitAttach() {
  const raw = q.value.trim();
  if (!raw) {
    cancelAttach();
    return;
  }
  try {
    const info = await validatePath(raw);
    attaching = false;
    renderInputIcon();
    q.value = appendMention(savedQuery, info.path);
    savedQuery = "";
    updatePlaceholder();
    const kind = info.kind === "image" ? "Image" : "Text";
    setAttachBar(
      "hint",
      `Attachment added: ${info.name} (${kind}, ${humanBytes(info.bytes)})`,
    );
    const token = ++attachBarToken;
    window.setTimeout(() => {
      if (!attaching && token === attachBarToken) hideAttachBar();
    }, 3000);
    // The list follows the new query (an attachment-only query surfaces the "Ask with Attachment" entry, avoiding an accidental run of the first command);
    // on the Quick Ask page the draft is a message, so only the pill follows the mention.
    if (view.get().mode === "chat") renderActionBar();
    else void refresh(q.value);
    q.focus();
  } catch (err) {
    setAttachBar("error", `Failed to add attachment: ${String(err)}`);
  }
}

// ---- Global keyboard events (keyboard-first: every capability is reachable, the mouse is only redundant) ----

window.addEventListener("keydown", (e) => {
  // Back while an answer streams asks first (ADR-0036/0038): the dialog is the front-most layer and
  // owns every key — including the attachment toggle below (a modal must not open another mode).
  if (confirmOpen) {
    e.preventDefault();
    if (e.key === "Enter") {
      const action = confirmAction;
      closeConfirm();
      action?.();
    } else if (e.key === "Escape" || e.key === "Backspace") {
      closeConfirm();
    }
    return;
  }
  if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key.toLowerCase() === "a") {
    e.preventDefault();
    if (attaching) cancelAttach();
    else startAttach();
    return;
  }
  if (attaching) {
    // In attachment mode the input bar serves paths only: no panel semantics are triggered
    if (e.key === "Enter") {
      e.preventDefault();
      void submitAttach();
    } else if (e.key === "Escape" || (e.key === "Backspace" && q.value === "" && !e.repeat)) {
      e.preventDefault();
      cancelAttach();
    }
    return;
  }
  // The about card is an overlay owned by the chip (ADR-0026); its input handles its own keys,
  // but Esc must still close it when the focus sits elsewhere (e.g. right after click-running a row).
  if (aboutCardOpen && e.key === "Escape") {
    e.preventDefault();
    closeAboutCard();
    return;
  }
  // Generic actions (ADR-0014/0022/0029): Browse ⌘P / Actions ⌘K / New ⌘N /
  // Delete ⌃X / DeleteAll ⌃⇧X / Favorite ⌘⇧F. Semantics are recognized by the shared keymap module; the landing spot is decided by this surface.
  const general = generalActionOf(e);
  if (general) {
    if (general === "favorite") {
      // ⌘⇧F toggles the current command's favorite state (ADR-0029), wherever it is on screen
      e.preventDefault();
      void toggleFavorite(currentCommandId());
      return;
    }
    if (general === "openConfig") {
      // ⌘, opens the config file (the macOS Preferences convention, ADR-0027 amendment)
      e.preventDefault();
      closeAboutCard();
      void openConfigFile();
      return;
    }
    if (general === "delete" || general === "deleteAll") {
      const v = view.get();
      // On the Quick Ask page the slot acts on the conversation (ADR-0036, the chat history's deletion)
      if (v.mode === "chat") {
        e.preventDefault();
        if (general === "deleteAll") void deleteAllChats();
        else void removeCurrentChat();
        return;
      }
      // The delete slot works on the results layer (ADR-0022)…
      if (v.mode === "items" && v.sourceCommandId) {
        e.preventDefault();
        void deleteFocused(general === "deleteAll");
        return;
      }
      // …and on the Suggestions section (ADR-0025): ⌃X forgets the focused command,
      // ⌃⇧X clears all recent usage. Elsewhere the input's native cut stays.
      if (v.mode === "commands" && v.suggestionsCount > 0) {
        if (general === "deleteAll" || isSuggestionFocus(v)) {
          e.preventDefault();
          if (general === "deleteAll") void clearSuggestions();
          else void deleteSuggestion(v.commands[v.focus]?.id);
          return;
        }
      }
      return;
    }
    e.preventDefault();
    if (general === "actions") toggleActionsCard();
    else void openEntry(general);
    return;
  }
  // The front-most list owns navigation and Backspace (ADR-0031): while a card is open, the card's
  // own input stops propagation for the keys it consumes; this guard covers focus sitting elsewhere.
  // Backspace with empty input = Back (layered back, ADR-0017):
  // leave it alone when the input is non-empty (normal delete); when empty, back out layer by layer, but the root layer does not close the panel (quit:false).
  const cardOpen = actionsCard.isOpen() || aboutCardOpen;
  if (!cardOpen && e.key === "Backspace" && q.value === "" && !e.isComposing && !e.repeat) {
    e.preventDefault();
    void back({ quit: false });
    return;
  }
  // The Quick Ask page steps through chat history with ⌃[ / ⌃] (ADR-0036, the Side View's binding):
  // ⌃[ goes backward to the previous (older) conversation, ⌃] forward to the newer one
  if (
    !cardOpen &&
    view.get().mode === "chat" &&
    e.ctrlKey &&
    !e.metaKey &&
    !e.altKey &&
    (e.key === "[" || e.key === "]")
  ) {
    e.preventDefault();
    void stepChat(e.key === "[");
    return;
  }
  // On the Quick Ask page ↑↓/⌃N/⌃P keep their native input meaning (nothing to navigate)
  const navigating = !cardOpen && view.get().mode !== "chat";
  if (navigating && (e.key === "ArrowDown" || (e.ctrlKey && !e.metaKey && e.key === "n"))) {
    e.preventDefault();
    move(1);
  } else if (navigating && (e.key === "ArrowUp" || (e.ctrlKey && !e.metaKey && e.key === "p"))) {
    e.preventDefault();
    move(-1);
  } else if (e.key === "Enter") {
    const v = view.get();
    if (v.mode === "chat" && e.isComposing) return; // an IME commit is not a send
    e.preventDefault();
    // On the Quick Ask page Enter sends the draft (⌥⏎ copies the last answer) — or stops the stream (ADR-0036)
    if (v.mode === "chat") {
      if (e.altKey) void copyLastAnswer();
      else if (chatGenerating) void stopChatGeneration();
      else void sendChatMessage();
      return;
    }
    // A pending result (e.g. AI Commands streaming a transform) yields its primary to Stop: Enter stops
    // the generation instead of writing back the partial text — the pill already reads Stop Generation
    // (ADR-0036 amendment). ⌥⏎ keeps the secondary action (copy the partial text); after the stream
    // ends, Enter is the item's primary again (write back).
    if (v.mode === "items" && !e.altKey && v.items[v.focus]?.pending) {
      void stopGeneration();
      return;
    }
    void applyFocused(e.altKey);
  } else if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key.toLowerCase() === "k") {
    // About (⌘⇧K, ADR-0027): the chip's card, with a platform binding
    e.preventDefault();
    toggleAboutCard();
  } else if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "j") {
    // Materialize (⌘J, ADR-0014 amendment). ⌘K is routed through generalActionOf above; ⌃K stays
    // the macOS kill-line editing key (ADR-0031)
    e.preventDefault();
    void materialize();
  } else if (e.key === "Escape") {
    e.preventDefault();
    void back();
  }
});

/** Actions card toggle (⌘K / ⌘⇧P / the "Actions" button on the pill). */
function toggleActionsCard() {
  if (actionsCard.isOpen()) closeActionsCard();
  else void openActionsCard();
}

// Click outside the card/pill to close (the pill is excluded, otherwise it would cancel the "Actions" button's click)
document.addEventListener("mousedown", (e) => {
  const target = e.target as Node;
  // Clicking away from the stop-confirmation dialog keeps generating (a click is a cancel, like Esc)
  if (confirmOpen && !confirmCardEl.contains(target)) closeConfirm();
  if (actionsCard.isOpen() && !actionsCardEl.contains(target) && !actionBarEl.contains(target)) {
    closeActionsCard();
  }
  // The about card closes when clicking outside it or the chip it belongs to (ADR-0026)
  if (aboutCardOpen && !aboutCardEl.contains(target) && !chipEl.contains(target)) {
    closeAboutCard();
  }
});

let debounce: ReturnType<typeof setTimeout> | undefined;
q.addEventListener("input", () => {
  // On the Quick Ask page the input is a message draft, not a search: only the pill follows it.
  if (view.get().mode === "chat") {
    renderActionBar();
    return;
  }
  clearTimeout(debounce);
  debounce = setTimeout(() => {
    if (attaching) return;
    const v = view.get();
    // While a result streams the page owns the input (ADR-0038): re-running the search would replace
    // the results layer under the stream and orphan it (the frames keep arriving for a page nobody sees)
    if (v.mode === "items" && v.items.some((i) => i.pending)) return;
    if (v.mode === "items" && v.sourceLive && v.sourceCommandId) {
      void rerunLive(q.value);
    } else {
      void refresh(q.value);
    }
  }, 60);
});

// ---- Summon permission guidance (ADR-0008): always visible until granted, disappears once authorized ----

interface SummonStatus {
  status: "ready" | "needsPermission" | "unsupported";
  key: string;
  doubleTapMs: number;
  accessibility: boolean;
}

const KEY_LABELS: Record<string, string> = {
  "double-cmd": "double-tap ⌘",
  "double-option": "double-tap ⌥",
  "double-ctrl": "double-tap ⌃",
  "double-shift": "double-tap ⇧",
};

const bannerEl = document.querySelector<HTMLDivElement>("#banner")!;
const bannerTextEl = document.querySelector<HTMLSpanElement>("#banner-text")!;
const bannerActionEl = document.querySelector<HTMLButtonElement>("#banner-action")!;

function hideBanner() {
  bannerEl.classList.add("hidden");
  bannerEl.classList.remove("flex");
}

async function refreshBanner() {
  const status = await invoke<SummonStatus>("summon_status");
  if (status.status === "needsPermission") {
    const key = KEY_LABELS[status.key] ?? status.key;
    bannerTextEl.textContent = `${key} to summon needs Input Monitoring permission (System Settings → Privacy & Security → Input Monitoring); it takes effect automatically once granted, though some macOS versions require restarting Moe once.`;
    bannerActionEl.textContent = "Open Input Monitoring Settings";
    bannerActionEl.dataset.action = "input-monitoring";
    showBanner();
  } else if (status.status === "unsupported") {
    // Sessions that cannot globally intercept the keyboard (Linux/Wayland etc., IIE4AD-350): offer the alternative paths
    const key = KEY_LABELS[status.key] ?? status.key;
    bannerTextEl.textContent = `${key} to summon is unavailable in this session (Wayland and similar cannot globally intercept the keyboard): set [summon] key in config.toml to a combo, or bind \`moe --toggle\` in your WM.`;
    bannerActionEl.textContent = "Open Config File";
    bannerActionEl.dataset.action = "config";
    showBanner();
  } else if (!status.accessibility) {
    bannerTextEl.textContent =
      "Reading the selection and writing back needs Accessibility permission (System Settings → Privacy & Security → Accessibility; write-back also auto-prompts); no restart needed once granted.";
    bannerActionEl.textContent = "Open Accessibility Settings";
    bannerActionEl.dataset.action = "accessibility";
    showBanner();
  } else {
    hideBanner();
  }
}

function showBanner() {
  bannerIconEl.replaceChildren(iconEl("alert", { size: 14 }));
  bannerEl.classList.remove("hidden");
  bannerEl.classList.add("flex");
}

bannerActionEl.addEventListener("click", () => {
  const action = bannerActionEl.dataset.action;
  if (action === "config") {
    void invoke("invoke_command", {
      commandId: "moe.open-config",
      query: null,
      record: false,
    });
    return;
  }
  void invoke(
    action === "accessibility"
      ? "open_accessibility_settings"
      : "open_input_monitoring_settings",
  );
});

// Streaming command events: the items layer updates in place by item id; the Quick Ask page adopts
// frames of its own conversation (the capture's ask and the page's sends both carry the id). Frames
// arrive per model delta, so both surfaces run them through the coalescer (ADR-0038): the DOM
// re-renders at most every ~50 ms, and the newest frame per key is what lands.
const chatFrames = streamCoalescer<string, { text: string; pending: boolean }>((frames) => {
  const frame = frames.get(chatConversationId ?? "");
  if (!frame || view.get().mode !== "chat") return;
  const parts = ensureChatParts();
  parts.view.updateStreaming(frame.text, frame.pending);
  parts.view.scrollToEnd();
  chatGenerating = frame.pending;
  refreshChatChrome();
  // The final frame (pending=false) marks the end of generation (IIE4AD-365)
  if (!frame.pending) void loadChatConversations();
});

const itemFrames = streamCoalescer<string, { commandId: string; item: Item }>((frames) => {
  const v = view.get();
  if (v.mode !== "items") return;
  const items = v.items.slice();
  let changed = false;
  for (const [id, frame] of frames) {
    if (frame.commandId !== v.sourceCommandId) continue;
    const idx = items.findIndex((i) => i.id === id);
    if (idx === -1) continue;
    items[idx] = frame.item;
    changed = true;
  }
  // view updates trigger render() → the preview re-renders in place on streaming events (staying pinned to the bottom)
  if (changed) view.update((s) => ({ ...s, items }));
});

void listen<CommandEventPayload>("command-event", (event) => {
  const payload = event.payload?.itemUpdated;
  if (!payload) return;
  const v = view.get();
  if (v.mode === "chat") {
    const conversationId = (payload.item.payload as { conversationId?: string } | null)
      ?.conversationId;
    if (!conversationId || conversationId !== chatConversationId) return;
    chatFrames.push(conversationId, {
      text: payload.item.detail ?? "",
      pending: payload.item.pending === true,
    });
    return;
  }
  if (v.mode !== "items" || v.sourceCommandId !== payload.commandId) return;
  if (!v.items.some((i) => i.id === payload.item.id)) return;
  itemFrames.push(payload.item.id, { commandId: payload.commandId, item: payload.item });
});
void listen("summon-authorized", () => hideBanner());

// Keep the previous input and results across summons (the user may add input after hiding);
// only refresh the permission guidance (the user may have just granted it in System Settings); unfinished attachment input is not kept across summons.
window.addEventListener("focus", () => {
  cancelAttach();
  void refreshBanner();
});

await initActionBar();
// One source for the row height (ADR-0026): the bottom bar, chip and pill follow ROW_HEIGHT.
document.documentElement.style.setProperty("--moe-row-h", `${ROW_HEIGHT}px`);
renderInputIcon();
await refresh("");
await refreshBanner();
render();
// Size the panel to exactly VISIBLE_ROWS rows (ADR-0026): measured from the real input bar,
// so the bottom bar lines up with the last row band regardless of font metrics.
void invoke("resize_panel", { width: PANEL_WIDTH, height: measuredPanelHeight() }).catch(() => {});
