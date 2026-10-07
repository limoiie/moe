import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import DOMPurify from "dompurify";
import { marked } from "marked";
import "./styles.css";
import { appendMention, humanBytes, validatePath } from "./attachment";
import { generatingEl } from "./generating";
import { iconEl } from "./icons";
import { kbdEl } from "./kbd";
import { GENERAL_KEY_LABELS, generalActionOf, type EntryAction } from "./keymap";
import {
  DETAIL_PANE_CLASS,
  LIST_FULL_CLASS,
  LIST_NARROW_CLASS,
  PANEL_WIDTH,
  pageShapeOf,
  panelHeightFor,
  type PageShape,
} from "./layout";
import { store } from "./store";
import type {
  Action,
  ActionResult,
  CommandEventPayload,
  CommandMeta,
  CommandSection,
  Item,
} from "./types";

// ---- View state ----

/** An entry on the actions layer: actions of the focused item, or platform generic actions (Browse/New, ADR-0014). */
interface ActionRow {
  action: Action;
  platform?: EntryAction;
}

type Mode = "commands" | "items";
interface View {
  mode: Mode;
  focus: number;
  commands: CommandMeta[];
  /** Source grouping of the command layer (ADR-0020): used to render section headers; commands is its flattening, focus/navigation is based on the flat list. */
  sections: CommandSection[];
  /** How many leading flat entries belong to the Suggestions section (0 = none, ADR-0025). */
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

const view = store<View>({
  mode: "commands",
  focus: 0,
  commands: [],
  sections: [],
  suggestionsCount: 0,
  items: [],
});

/// The item the current detail card belongs to (streaming events re-render by it)
let detailItemId: string | null = null;
/** Detail card shape: preview (focused preview, the list stays) / message (full-screen card, e.g. errors). */
type DetailMode = "none" | "preview" | "message";
let detailMode: DetailMode = "none";
/** After Esc dismisses the preview, it stays collapsed until focus changes (Raycast-style layering). */
let previewDismissed = false;

/** Actions card (the floating card opened by ⌘K / ⌘⇧P): all actions + filtering, without replacing the body. */
let actionsCardRows: ActionRow[] = [];
let actionsCardFocus = 0;
let actionsCardOpen = false;

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

function appAvatarEl(): HTMLImageElement {
  const img = document.createElement("img");
  img.src = "./moe.png";
  img.alt = "Moe";
  img.className = "moe-avatar-img";
  return img;
}

/** The chip's label: the extension's name while inside a command, otherwise none. */
function chipLabel(text: string): HTMLSpanElement {
  const label = document.createElement("span");
  label.className = "moe-avatar-label";
  label.textContent = text;
  return label;
}

/**
 * Render the avatar chip (ADR-0026): toast > extension (inside a command) > Moe's own avatar.
 * The chip is one row tall and its avatar sits centered on the item icons' x.
 */
function renderChip() {
  chipEl.replaceChildren();
  if (toastState) {
    chipEl.append(
      iconEl(toastState.icon, {
        size: 14,
        className: toastState.icon === "check" ? "text-emerald-400" : "text-amber-400",
      }),
      chipLabel(toastState.text),
    );
    return;
  }
  const v = view.get();
  if (v.mode === "items" && extensionMeta) {
    chipEl.append(
      extensionMeta.icon
        ? iconEl(extensionMeta.icon, { size: 16, className: "text-zinc-300" })
        : appAvatarEl(),
      chipLabel(extensionMeta.title),
    );
    chipEl.title = `About ${extensionMeta.title}`;
    return;
  }
  chipEl.append(appAvatarEl());
  chipEl.title = "About Moe";
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
}

/** Primary action of the focused item (same semantics as Apply): returns the title and where a click lands. */
function primaryActionOf(): { title: string; keys: string; run: () => void; disabled: boolean } {
  const v = view.get();
  const applyKeys = keyDisplay.get("apply") ?? "⏎";
  // When the actions card is open: the pill's primary action follows the highlighted action in the card (Raycast-style)
  if (actionsCardOpen) {
    const row = filteredActionRows()[actionsCardFocus];
    return {
      title: row?.action.title ?? "Apply",
      keys: row?.action.keybinding ?? applyKeys,
      run: () => void runActionRow(row),
      disabled: !row,
    };
  }
  if (v.mode === "items") {
    const item = v.items[v.focus];
    // While generating: the primary action yields to Stop (IIE4AD-365, Esc's first priority)
    if (item?.pending) {
      return {
        title: "Stop Generation",
        keys: keyDisplay.get("back") ?? "Esc",
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
    if (actionsCardOpen) closeActionsCard();
    else void openActionsCard();
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

/** Section header (ADR-0020): source is the group; headers are not focusable, do not participate in navigation, and do not respond to hover. */
function sectionHeaderEl(title: string): HTMLLIElement {
  const li = document.createElement("li");
  li.className =
    "select-none px-3 pt-2.5 pb-1 text-[11px] font-medium uppercase tracking-wider text-zinc-500";
  li.textContent = title;
  return li;
}

/** A single row: hover = interactive hint (does not steal focus); only the focused row gets the main highlight. */
function rowEl(e: Row, i: number, focused: boolean): HTMLLIElement {
  const li = document.createElement("li");
  li.className =
    "group flex cursor-default items-center gap-2.5 rounded-lg px-3 py-1.5 text-sm " +
    (focused
      ? "bg-zinc-700/70 text-zinc-50"
      : "text-zinc-300 hover:bg-zinc-800/30 hover:text-zinc-100 hover:ring-1 hover:ring-zinc-600/60");
  li.append(
    iconEl(e.icon, {
      className: e.iconMuted
        ? "text-zinc-600"
        : focused
          ? "text-zinc-200"
          : "text-zinc-400 group-hover:text-zinc-300",
    }),
  );
  const left = document.createElement("div");
  left.className = "min-w-0 flex-1 truncate";
  left.textContent = e.title;
  if (e.subtitle) {
    const sub = document.createElement("span");
    sub.className = "ml-2 text-xs text-zinc-500";
    sub.textContent = e.subtitle;
    left.append(sub);
  }
  li.append(left);
  if (e.key) {
    // Key blocks (shadcn Kbd style): same inline-key style as the action bar and the ⌘K card
    const keys = kbdEl(e.key, { firstOnly: true });
    keys.classList.add("shrink-0");
    li.append(keys);
  }
  li.addEventListener("mousedown", () => {
    view.update((s) => ({ ...s, focus: i }));
  });
  return li;
}

function render() {
  const v = view.get();
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
    // Command layer: render grouped by source (headers + command rows within each group); the focus index stays on the flat list.
    const lis: HTMLLIElement[] = [];
    let flat = 0;
    for (const section of v.sections) {
      lis.push(sectionHeaderEl(section.title));
      for (const c of section.items) {
        const li = rowEl(
          {
            title: c.title,
            subtitle: c.subtitle,
            icon: c.icon,
            iconMuted: !c.icon,
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
  } else {
    const rows = currentEntries().map((e, i) => rowEl(e, i, i === v.focus));
    focusedLi = rows[v.focus] ?? null;
    listEl.replaceChildren(...rows);
  }
  // Keyboard navigation: the focused row always stays in the viewport (long lists)
  focusedLi?.scrollIntoView({ block: "nearest" });
  renderDetail();
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
    "mb-3 flex items-center gap-2.5 border-b border-zinc-800 pb-2.5 text-sm not-prose";
  header.append(
    iconEl(item.icon, { size: 15, className: "shrink-0 text-zinc-500" }),
  );
  const title = document.createElement("span");
  title.className = "min-w-0 flex-1 truncate font-medium text-zinc-100";
  title.textContent = item.title;
  header.append(title);
  if (item.subtitle) {
    const sub = document.createElement("span");
    sub.className = "shrink-0 text-xs text-zinc-500";
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
  // The split page's detail pane is part of the page (not dismissible): previewDismissed only applies to full-screen detail
  const dismissible = shape === "detail";
  if (
    detailMode === "message" ||
    (dismissible && previewDismissed) ||
    shape === "list" ||
    !item
  ) {
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

/**
 * First layer of Esc/Backspace: consumes a dismissible detail (returns true if consumed).
 * Only "full-screen detail" can be dismissed (after which the result row is visible); the split
 * page's detail pane is part of the page and is not dismissed — Back goes straight to the root (ADR-0018).
 */
function dismissDetail(): boolean {
  if (detailMode === "none") return false;
  const v = view.get();
  const item = v.mode === "items" ? v.items[v.focus] : undefined;
  const shape = pageShapeOf(item, v.detailFull === true, v.items.length);
  if (shape !== "detail") return false;
  previewDismissed = true;
  clearDetail();
  return true;
}

view.subscribe(render);

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
  if ("list" in res) {
    const items = res.list.items;
    closeActionsCard();
    clearDetail();
    previewDismissed = false;
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
  // Current Extension: use the source command once in the results/actions layers; on the command layer, use the focused command
  const from = v.sourceCommandId ?? (v.mode === "commands" ? v.commands[v.focus]?.id : undefined);
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
      // Live commands take over input with their own view: clear the input on entry (same as applyFocused)
      query: null,
    });
    if (command.live) q.value = "";
    applyResult(res, command.id, command.live, command.icon, command.title);
  } catch (err) {
    showMessage(`Failed to run: ${String(err)}`);
  }
}

async function applyFocused(alt: boolean) {
  const v = view.get();
  if (v.mode === "commands") {
    const cmd = v.commands[v.focus];
    if (!cmd) return;
    try {
      const res = await invoke<ActionResult>("invoke_command", {
        commandId: cmd.id,
        // Live commands take over input with their own view: clear the input on entry; subsequent input is that command's query
        query: cmd.live ? null : q.value || null,
      });
      if (cmd.live) {
        q.value = "";
      }
      applyResult(res, cmd.id, cmd.live, cmd.icon, cmd.title);
    } catch (err) {
      showMessage(`Failed to run: ${String(err)}`);
    }
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
  previewDismissed = false;
  view.update((v) => {
    const len = v.mode === "commands" ? v.commands.length : v.items.length;
    return { ...v, focus: len === 0 ? 0 : (v.focus + delta + len) % len };
  });
}

/**
 * Open the actions card (Raycast-style floating card): the focused item's primary/secondary
 * actions, plus the platform generic actions the Extension actually declared (Browse ⌘P / New ⌘N, ADR-0014).
 * The body (command list / results) stays in place; it no longer replaces the whole panel as before.
 */
async function openActionsCard() {
  const v = view.get();
  if (v.mode !== "items") return;
  const item = v.items[v.focus];
  const rows: ActionRow[] = (item?.actions ?? []).map((action) => ({ action }));
  if (v.sourceCommandId) {
    const kinds: EntryAction[] = ["browse", "new"];
    const found = await Promise.all(
      kinds.map((kind) =>
        invoke<CommandMeta | null>("entry_command", {
          commandId: v.sourceCommandId,
          kind,
        }).catch(() => null),
      ),
    );
    kinds.forEach((kind, index) => {
      const entry = found[index];
      if (!entry) return;
      rows.push({
        platform: kind,
        action: {
          id: `moe.platform.${kind}`,
          title: kind === "browse" ? "Browse Records" : "New Record",
          kind: "secondary",
          keybinding: GENERAL_KEY_LABELS[kind],
        },
      });
    });
  }
  actionsCardRows = rows;
  actionsCardFocus = 0;
  actionsCardOpen = true;
  actionSearchEl.value = "";
  actionsCardEl.classList.remove("hidden");
  renderActionsCard();
  actionSearchEl.focus();
  renderActionBar();
}

/** Close the actions card and return focus to the Input Bar. */
function closeActionsCard() {
  if (!actionsCardOpen) return;
  actionsCardOpen = false;
  actionsCardEl.classList.add("hidden");
  renderActionBar();
  q.focus();
}

/** Filter by title (case-insensitive). */
function filteredActionRows(): ActionRow[] {
  const needle = actionSearchEl.value.trim().toLowerCase();
  if (!needle) return actionsCardRows;
  return actionsCardRows.filter((row) => row.action.title.toLowerCase().includes(needle));
}

function iconOfActionRow(row: ActionRow): string {
  if (row.platform) return row.platform === "browse" ? "history" : "plus";
  return row.action.kind === "primary" ? "corner-down-left" : "copy";
}

function renderActionsCard() {
  const rows = filteredActionRows();
  actionsCardFocus = rows.length === 0 ? 0 : Math.min(actionsCardFocus, rows.length - 1);
  if (rows.length === 0) {
    const empty = document.createElement("li");
    empty.className = "px-2 py-3 text-xs text-zinc-600";
    empty.textContent = actionSearchEl.value.trim() ? "No matching actions" : "This result has no actions";
    actionListEl.replaceChildren(empty);
    return;
  }
  actionListEl.replaceChildren(
    ...rows.map((row, index) => {
      const focused = index === actionsCardFocus;
      const li = document.createElement("li");
      li.className =
        "flex cursor-default items-center gap-2.5 rounded-lg px-2.5 py-1.5 text-sm " +
        (focused
          ? "bg-zinc-700/70 text-zinc-50"
          : "text-zinc-300 hover:bg-zinc-800/30 hover:text-zinc-100 hover:ring-1 hover:ring-zinc-600/60");
      li.append(
        iconEl(iconOfActionRow(row), {
          size: 15,
          className: focused ? "text-zinc-200" : "text-zinc-500",
        }),
      );
      const title = document.createElement("span");
      title.className = "min-w-0 flex-1 truncate";
      title.textContent = row.action.title;
      li.append(title);
      const keys = row.action.keybinding ?? (row.action.kind === "primary" ? "⏎" : null);
      if (keys) {
        const kbd = kbdEl(keys, { firstOnly: true });
        kbd.classList.add("shrink-0");
        li.append(kbd);
      }
      li.addEventListener("click", () => void runActionRow(row));
      return li;
    }),
  );
  actionListEl.children[actionsCardFocus]?.scrollIntoView({ block: "nearest" });
}

function moveActionFocus(delta: number) {
  const rows = filteredActionRows();
  if (rows.length === 0) return;
  actionsCardFocus = (actionsCardFocus + delta + rows.length) % rows.length;
  renderActionsCard();
  renderActionBar();
}

/** Run one entry of the actions card: platform generic actions take the same path as ⌘P/⌘N; the rest go to the Extension. */
async function runActionRow(row: ActionRow | undefined) {
  if (!row) return;
  closeActionsCard();
  if (row.platform) {
    await openEntry(row.platform);
    return;
  }
  const v = view.get();
  const item = v.mode === "items" ? v.items[v.focus] : undefined;
  if (!item || !v.sourceCommandId) return;
  try {
    const res = await invoke<ActionResult>("run_item_action", {
      commandId: v.sourceCommandId,
      item,
      action: row.action,
    });
    if (row.action.id === "copy") toast("Copied");
    applyResult(res, v.sourceCommandId, v.sourceLive, v.sourceIcon, v.sourceTitle);
  } catch (err) {
    showMessage(`Failed to run: ${String(err)}`);
  }
}

async function materialize() {
  const v = view.get();
  const item = v.mode === "items" ? v.items[v.focus] : undefined;
  const action = item?.actions.find((a) => a.id === "materialize" || a.keybinding === "⌘M");
  if (item && action && v.sourceCommandId) {
    const res = await invoke<ActionResult>("run_item_action", {
      commandId: v.sourceCommandId,
      item,
      action,
    });
    applyResult(res, v.sourceCommandId, v.sourceLive, v.sourceIcon, v.sourceTitle);
  }
}

// ---- About card (ADR-0026): the avatar chip's menu, same UX as the actions card ----

/** Feedback lands in the repository's issue tracker. */
const FEEDBACK_URL = "https://github.com/limoiie/moe/issues";

interface AboutRow {
  id: string;
  title: string;
  icon: string;
  run: () => void;
}

let aboutFocus = 0;
let aboutCardOpen = false;

/** The About menu: the app's own meta actions (ADR-0026). */
function aboutRows(): AboutRow[] {
  return [
    {
      id: "about.config",
      title: "Open Config File",
      icon: "settings-2",
      run: () => void runAboutConfig(),
    },
    {
      id: "about.key",
      title: "Save AI Key",
      icon: "key-round",
      run: () => void runAboutKey(),
    },
    {
      id: "about.feedback",
      title: "Send Feedback",
      icon: "megaphone",
      run: () => void runAboutFeedback(),
    },
  ];
}

async function runAboutConfig() {
  closeAboutCard();
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

function filteredAboutRows(): AboutRow[] {
  const needle = aboutSearchEl.value.trim().toLowerCase();
  if (!needle) return aboutRows();
  return aboutRows().filter((row) => row.title.toLowerCase().includes(needle));
}

function renderAboutCard() {
  const rows = filteredAboutRows();
  aboutFocus = rows.length === 0 ? 0 : Math.min(aboutFocus, rows.length - 1);
  if (rows.length === 0) {
    const empty = document.createElement("li");
    empty.className = "px-2 py-3 text-xs text-zinc-600";
    empty.textContent = "No matching actions";
    aboutListEl.replaceChildren(empty);
    return;
  }
  aboutListEl.replaceChildren(
    ...rows.map((row, index) => {
      const focused = index === aboutFocus;
      const li = document.createElement("li");
      li.className =
        "flex cursor-default items-center gap-2.5 rounded-lg px-2.5 py-1.5 text-sm " +
        (focused
          ? "bg-zinc-700/70 text-zinc-50"
          : "text-zinc-300 hover:bg-zinc-800/30 hover:text-zinc-100 hover:ring-1 hover:ring-zinc-600/60");
      li.append(
        iconEl(row.icon, { size: 15, className: focused ? "text-zinc-200" : "text-zinc-500" }),
      );
      const title = document.createElement("span");
      title.className = "min-w-0 flex-1 truncate";
      title.textContent = row.title;
      li.append(title);
      li.addEventListener("click", () => row.run());
      return li;
    }),
  );
  aboutListEl.children[aboutFocus]?.scrollIntoView({ block: "nearest" });
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
  if (e.key === "ArrowDown" || e.key === "ArrowUp") {
    e.preventDefault();
    e.stopPropagation();
    moveAboutFocus(e.key === "ArrowDown" ? 1 : -1);
  } else if (e.key === "Enter" && !e.isComposing) {
    e.preventDefault();
    e.stopPropagation();
    filteredAboutRows()[aboutFocus]?.run();
  } else if (
    e.key === "Escape" ||
    (e.key === "Backspace" && aboutSearchEl.value === "" && !e.repeat)
  ) {
    e.preventDefault();
    e.stopPropagation();
    closeAboutCard();
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

// Esc / empty-input Backspace layered back: stop generation → actions card → preview/detail → results layer → clear input → (may close panel)
// Empty Backspace passes quit:false: the root layer stays in place and does not close the panel (closing the panel belongs to Esc alone)
async function back(options: { quit?: boolean } = {}) {
  const { quit = true } = options;
  const v = view.get();
  // While streaming: Esc's first priority is stop (IIE4AD-365)
  if (v.mode === "items" && v.items[v.focus]?.pending) {
    await stopGeneration();
    return;
  }
  // The actions card is an overlay: close it first, then back out further (Raycast-style)
  if (actionsCardOpen) {
    closeActionsCard();
    return;
  }
  // Same for the about card (ADR-0026)
  if (aboutCardOpen) {
    closeAboutCard();
    return;
  }
  if (dismissDetail()) {
    return;
  }
  if (v.mode !== "commands") {
    view.update((s) => ({ ...s, mode: "commands", items: [], focus: 0 }));
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
  previewDismissed = false;
  const sections = await invoke<CommandSection[]>("search_commands", { query });
  view.update((v) => ({
    ...v,
    mode: "commands",
    sections,
    // Focus/navigation is based on the flat list; sections only render headers.
    commands: sections.flatMap((s) => s.items),
    // The Suggestions section only appears on the empty query and always comes first (ADR-0023).
    suggestionsCount:
      sections[0]?.title === "Suggestions" ? sections[0].items.length : 0,
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
function renderInputIcon() {
  // Same column as list rows: aligned size/color (ADR: Input Bar and Result List share one grid)
  inputIconEl.replaceChildren(
    iconEl(attaching ? "paperclip" : "search", { size: 16, className: "text-zinc-400" }),
  );
}

function setAttachBar(kind: "hint" | "error", text: string) {
  attachTextEl.textContent = text;
  const palette =
    kind === "error"
      ? "border-red-500/30 bg-red-500/10 text-red-200"
      : "border-sky-500/30 bg-sky-500/10 text-sky-200";
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
    // The list follows the new query (an attachment-only query surfaces the "AI: Ask with Attachment" entry, avoiding an accidental run of the first command)
    void refresh(q.value);
    q.focus();
  } catch (err) {
    setAttachBar("error", `Failed to add attachment: ${String(err)}`);
  }
}

// ---- Global keyboard events (keyboard-first: every capability is reachable, the mouse is only redundant) ----

window.addEventListener("keydown", (e) => {
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
  // Generic actions (ADR-0014/0022): Browse ⌘P / Actions ⌘⇧P / New ⌘N /
  // Delete ⌃X / DeleteAll ⌃⇧X. Semantics are recognized by the shared keymap module; the landing spot is decided by this surface.
  const general = generalActionOf(e);
  if (general) {
    if (general === "delete" || general === "deleteAll") {
      const v = view.get();
      // The delete slot works on the results layer (ADR-0022)…
      if (v.mode === "items" && v.sourceCommandId) {
        e.preventDefault();
        void deleteFocused(general === "deleteAll");
        return;
      }
      // …and on the Suggestions section (ADR-0025): ⌃X forgets the focused command,
      // ⌃⇧X clears all recent usage. Elsewhere the input's native cut stays.
      if (v.mode === "commands" && v.suggestionsCount > 0) {
        if (general === "deleteAll" || v.focus < v.suggestionsCount) {
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
  // Backspace with empty input = Back (layered back, ADR-0017):
  // leave it alone when the input is non-empty (normal delete); when empty, back out layer by layer, but the root layer does not close the panel (quit:false).
  if (e.key === "Backspace" && q.value === "" && !e.isComposing && !e.repeat) {
    e.preventDefault();
    void back({ quit: false });
    return;
  }
  if (e.key === "ArrowDown" || (e.ctrlKey && !e.metaKey && e.key === "n")) {
    e.preventDefault();
    move(1);
  } else if (e.key === "ArrowUp" || (e.ctrlKey && !e.metaKey && e.key === "p")) {
    e.preventDefault();
    move(-1);
  } else if (e.key === "Enter") {
    e.preventDefault();
    void applyFocused(e.altKey);
  } else if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
    e.preventDefault();
    toggleActionsCard();
  } else if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "m") {
    e.preventDefault();
    void materialize();
  } else if (e.key === "Escape") {
    e.preventDefault();
    void back();
  }
});

/** Actions card toggle (⌘K / ⌘⇧P / the "Actions" button on the pill). */
function toggleActionsCard() {
  if (actionsCardOpen) closeActionsCard();
  else void openActionsCard();
}

// The actions card's own bindings: ↑↓ select, ⏎ run, Esc/empty Backspace closes (Raycast-style)
actionSearchEl.addEventListener("input", () => {
  actionsCardFocus = 0;
  renderActionsCard();
  renderActionBar();
});

// Click outside the card/pill to close (the pill is excluded, otherwise it would cancel the "Actions" button's click)
document.addEventListener("mousedown", (e) => {
  const target = e.target as Node;
  if (actionsCardOpen && !actionsCardEl.contains(target) && !actionBarEl.contains(target)) {
    closeActionsCard();
  }
  // The about card closes when clicking outside it or the chip it belongs to (ADR-0026)
  if (aboutCardOpen && !aboutCardEl.contains(target) && !chipEl.contains(target)) {
    closeAboutCard();
  }
});

actionSearchEl.addEventListener("keydown", (e) => {
  if (e.key === "ArrowDown" || e.key === "ArrowUp") {
    e.preventDefault();
    e.stopPropagation();
    moveActionFocus(e.key === "ArrowDown" ? 1 : -1);
  } else if (e.key === "Enter" && !e.isComposing) {
    e.preventDefault();
    e.stopPropagation();
    void runActionRow(filteredActionRows()[actionsCardFocus]);
  } else if (
    e.key === "Escape" ||
    (e.key === "Backspace" && actionSearchEl.value === "" && !e.repeat)
  ) {
    e.preventDefault();
    e.stopPropagation();
    closeActionsCard();
  }
});

let debounce: ReturnType<typeof setTimeout> | undefined;
q.addEventListener("input", () => {
  clearTimeout(debounce);
  debounce = setTimeout(() => {
    if (attaching) return;
    const v = view.get();
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

// Streaming command events: update in place by item id (e.g. AI answers arriving word by word)
void listen<CommandEventPayload>("command-event", (event) => {
  const payload = event.payload?.itemUpdated;
  if (!payload) return;
  const v = view.get();
  if (v.mode !== "items" || v.sourceCommandId !== payload.commandId) return;
  const idx = v.items.findIndex((i) => i.id === payload.item.id);
  if (idx === -1) return;
  const items = v.items.slice();
  items[idx] = payload.item;
  // view updates trigger render() → the preview re-renders in place on streaming events (staying pinned to the bottom)
  view.update((s) => ({ ...s, items }));
});
void listen("summon-authorized", () => hideBanner());

// Keep the previous input and results across summons (the user may add input after hiding);
// only refresh the permission guidance (the user may have just granted it in System Settings); unfinished attachment input is not kept across summons.
window.addEventListener("focus", () => {
  cancelAttach();
  void refreshBanner();
});

await initActionBar();
renderInputIcon();
await refresh("");
await refreshBanner();
render();
// Size the panel to exactly VISIBLE_ROWS rows (ADR-0026): measured from the real input bar,
// so the bottom bar lines up with the last row band regardless of font metrics.
void invoke("resize_panel", { width: PANEL_WIDTH, height: measuredPanelHeight() }).catch(() => {});
