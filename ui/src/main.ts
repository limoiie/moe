import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import DOMPurify from "dompurify";
import { marked } from "marked";
import "./styles.css";
import { appendMention, humanBytes, validatePath } from "./attachment";
import { createActionPlan, type ActionRow, type PageContext } from "./actions";
import { argumentValues, declaredArguments, effectiveOption } from "./arguments";
import { createCard, type CardSection } from "./card";
import { createComposer } from "./composer";
import { rowAccessories } from "./accessories";
import { applyTagFilter, filterChangeAllowed, filterValuesFor, settleFilterValue } from "./filter";
import { generatingEl } from "./generating";
import { iconEl } from "./icons";
import { metadataEl, type MetadataDeps } from "./metadata";
import { emptyViewEl, rowEl, sectionHeaderEl, type Row, type RowDeps } from "./rows";
import {
  chatTarget,
  NO_SELECTION,
  selectionIndicator,
  type SelectionDetail,
  type SelectionState,
} from "./selection";
import { invokeWithTimeout } from "./ipc";
import { kbdEl } from "./kbd";
import { GENERAL_KEY_LABELS, generalActionOf, keyDisplay, loadKeyDisplay, type EntryAction } from "./keymap";
import { createLayers } from "./layers";
import { createLoadingHairline } from "./loading";
import { logoEl } from "./logo";
import { createModelSwitcher, type ActivePair } from "./models";
import {
  CONVERSATION_PANE_CLASS,
  DETAIL_COLUMNS_CLASS,
  DETAIL_PANE_CLASS,
  LIST_FULL_CLASS,
  LIST_NARROW_CLASS,
  METADATA_COLUMN_WIDTH,
  PANEL_WIDTH,
  pageShapeOf,
  panelHeightFor,
  ROW_HEIGHT,
  type PageShape,
} from "./layout";
import {
  conversationTitle,
  createConversationView,
  reasoningEl,
  type ConversationView,
} from "./messages";
import { initPointerIntent } from "./pointer";
import { store } from "./store";
import { streamCoalescer } from "./stream";
import { initTheme, themePreference, type ThemePreference } from "./theme";
import type {
  Action,
  ActionResult,
  CommandArgument,
  CommandEventPayload,
  CommandMeta,
  CommandSection,
  Conversation,
  EmptyView,
  Filter,
  Item,
  Message,
  ToolCall,
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
  /**
   * The **visible** item set. The tag filter narrows it, so focus, navigation, the actions card, the
   * delete slots and `pageShapeOf` keep walking one list — which is what lets the platform filter
   * without touching anything else (ADR-0043).
   */
  items: Item[];
  /** The result set as returned, before the tag filter: what re-widening restores, and what the
   *  Filter Card's values are derived from. */
  allItems: Item[];
  /** The result's declared filter dimension (ADR-0043); absent on pages that have none. */
  filter?: Filter;
  /** The dimension's values, derived from `allItems` before the filter narrows anything. */
  filterValues: string[];
  /** The active value; undefined = All. */
  filterValue?: string;
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
  /** The result's declared zero-row placeholder (ADR-0043): shown instead of a blank panel. */
  empty?: EmptyView;
}

const q = document.querySelector<HTMLInputElement>("#query")!;
const queryRowEl = document.querySelector<HTMLDivElement>("#query-row")!;
const querySubtitleEl = document.querySelector<HTMLSpanElement>("#query-subtitle")!;
const chatComposerHostEl = document.querySelector<HTMLElement>("#chat-composer")!;
const argumentChipsEl = document.querySelector<HTMLSpanElement>("#argument-chips")!;
const argumentCardEl = document.querySelector<HTMLDivElement>("#argument-card")!;
const argumentListEl = document.querySelector<HTMLUListElement>("#argument-list")!;
const argumentSearchEl = document.querySelector<HTMLInputElement>("#argument-search")!;
const chatTargetRowEl = document.querySelector<HTMLDivElement>("#chat-target-row")!;
const chatTargetEl = document.querySelector<HTMLSpanElement>("#chat-target")!;
const chatTargetPeekEl = document.querySelector<HTMLDivElement>("#chat-target-peek")!;
const bottomBarEl = document.querySelector<HTMLDivElement>("#bottom-bar")!;
const rootEl = document.querySelector<HTMLDivElement>("#root")!;
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
const loadingBarEl = document.querySelector<HTMLDivElement>("#loading-bar")!;
const filterControlEl = document.querySelector<HTMLButtonElement>("#filter-control")!;
const selectionStateEl = document.querySelector<HTMLSpanElement>("#selection-state")!;
const filterCardEl = document.querySelector<HTMLDivElement>("#filter-card")!;
const filterListEl = document.querySelector<HTMLUListElement>("#filter-list")!;
const filterSearchEl = document.querySelector<HTMLInputElement>("#filter-search")!;
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
  allItems: [],
  filterValues: [],
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
/**
 * Palette search in flight (ADR-0043): a hairline under the Input Bar, and stale rows stay visible —
 * the list is never blanked mid-typing. `.moe-generating`'s dots are deliberately not reused: search
 * latency and token streaming are different states, and conflating them makes a debounce look like a
 * generation.
 */
let searchLoading = false;

/**
 * The palette's loading hairline (MOE-0055): delayed and minimum-visible, so a fast local search never
 * flashes it. `searchLoading` itself stays immediate — the Empty View's suppression reads it, and that
 * rule is about a load being in flight, not about the hairline being up (ADR-0043's no-flash rule).
 */
const loadingHairline = createLoadingHairline({
  delayMs: 150,
  minVisibleMs: 250,
  onChange: (visible) => loadingBarEl.classList.toggle("hidden", !visible),
});

function setSearchLoading(on: boolean) {
  searchLoading = on;
  loadingHairline.set(on);
}

/**
 * The captured selection's shape as the backend last reported it (MOE-0057). Read once at boot
 * (`selection_state`), then kept current by the `selection-changed` event the show path and the
 * late-capture worker emit — shape only, never the text.
 */
let selectionState: SelectionState = NO_SELECTION;

/**
 * The captured selection itself, for the composer's peek (MOE-0059). Fetched on demand — the chips
 * render from the shape alone, so the text crosses into the panel only when the user hovers to look
 * — and keyed by the state object it was fetched for, so a fresh summon invalidates it by identity.
 */
let selectionDetail: SelectionDetail | null = null;
let selectionDetailFor: SelectionState | null = null;

const actionsCard = createCard({
  cardEl: actionsCardEl,
  listEl: actionListEl,
  inputEl: actionSearchEl,
  emptyText: "This result has no actions",
  onFocusChange: () => renderActionBar(),
  onClose: () => {
    renderActionBar();
    draftEl().focus();
  },
});

/**
 * The Filter Card (ADR-0043): the dimension's values plus a platform-supplied reset, anchored under
 * its trigger (ADR-0028's anchor-follows-trigger rule). The same component and key discipline as
 * every other card (ADR-0044); single-select, so choosing a value closes it.
 */
const filterCard = createCard({
  cardEl: filterCardEl,
  listEl: filterListEl,
  inputEl: filterSearchEl,
  emptyText: "No filter values",
  filteredEmptyText: "No matching values",
  onClose: () => draftEl().focus(),
});

function openFilterCard() {
  const v = view.get();
  const filter = filterableDimension();
  if (!filter) return;
  // Frozen while a result streams (MOE-0046): a picker whose rows cannot take effect would be a
  // control that silently does nothing, so it does not open.
  if (!filterChangeAllowed(v.allItems)) return;
  // The cards are peers: opening one closes the others (like openActionsCard / openAboutCard)
  closeActionsCard();
  closeAboutCard();
  filterCard.open([
    {
      rows: [
        {
          title: `All ${filter.label}`,
          icon: filter.icon,
          checked: v.filterValue === undefined,
          run: () => setFilterValue(undefined),
        },
        ...v.filterValues.map((value) => ({
          title: value,
          icon: filter.icon,
          checked: v.filterValue === value,
          run: () => setFilterValue(value),
        })),
      ],
    },
  ]);
}

function closeFilterCard() {
  filterCard.close();
}

/**
 * The Argument Card (ADR-0045): the argument chip's picker — the argument's options, the house card
 * language and key discipline (ADR-0044). Single-select, so choosing closes it; the cards are peers,
 * so opening one closes the others. The input keeps what is typed even when nothing matches
 * (`createCard`'s filtered-empty line), which is what makes a short list searchable without losing
 * the query — and the query the panel's search box carries is never touched by this card.
 */
const argumentCard = createCard({
  cardEl: argumentCardEl,
  listEl: argumentListEl,
  inputEl: argumentSearchEl,
  emptyText: "This argument has no options",
  filteredEmptyText: "No matching options",
  onClose: () => {
    openArgument = null;
    renderArgumentChips();
    draftEl().focus();
  },
});

/** The argument whose picker is open (null = none): Tab cycles through the focused command's. */
let openArgument: { command: CommandMeta; argument: CommandArgument } | null = null;

/**
 * The argument values chosen this session, keyed by command id then argument id — a session cache
 * like the filter's memory, never disk: the declaration is the authority whenever nothing is chosen,
 * so a restart simply shows the declared defaults again.
 */
const argumentMemory = new Map<string, Map<string, string>>();

function rememberedArgument(commandId: string, argumentId: string): string | undefined {
  return argumentMemory.get(commandId)?.get(argumentId);
}

function setArgumentValue(commandId: string, argumentId: string, optionId: string) {
  const remembered = argumentMemory.get(commandId) ?? new Map<string, string>();
  remembered.set(argumentId, optionId);
  argumentMemory.set(commandId, remembered);
  renderArgumentChips();
}

/** The focused command on the command layer: the only row that can be applied with arguments. */
function focusedCommand(): CommandMeta | undefined {
  const v = view.get();
  return v.mode === "commands" ? v.commands[v.focus] : undefined;
}

/** The command a result page belongs to (a re-run's item action carries its arguments, ADR-0045). */
function pageCommand(): CommandMeta | undefined {
  const v = view.get();
  return v.commands.find((c) => c.id === v.sourceCommandId);
}

/** The values one invocation carries for a command (ADR-0045); {} for a command that declares none. */
function argumentValuesFor(command: CommandMeta | undefined): Record<string, string> {
  return argumentValues(command, rememberedArgument);
}

/**
 * The argument chips (ADR-0045): the focused command's declared arguments, in the Input Bar before
 * the text — the values ⏎ will run with. Only the command layer shows them: a result page already
 * ran with its values, and the command layer is where they can still change.
 */
function renderArgumentChips() {
  const command = focusedCommand();
  const declared = declaredArguments(command);
  argumentChipsEl.classList.toggle("hidden", declared.length === 0);
  // The picker belongs to the row that opened it: another row (or no row at all) closes it, so a
  // menu can never outlive the command whose argument it edits.
  if (argumentCard.isOpen() && openArgument?.command.id !== command?.id) {
    closeArgumentCard();
    return;
  }
  if (!command || declared.length === 0) return;
  argumentChipsEl.replaceChildren(
    ...declared.map((argument) => argumentChipEl(command, argument)),
  );
}

/** One chip: the argument's effective value plus the picker's chevron. */
function argumentChipEl(command: CommandMeta, argument: CommandArgument): HTMLButtonElement {
  const chip = document.createElement("button");
  chip.type = "button";
  chip.className = "moe-argument-chip";
  const open = argumentCard.isOpen() && openArgument?.argument.id === argument.id;
  chip.dataset.open = open ? "true" : "false";
  chip.title = `${argument.title} — press Tab to choose`;
  const label = document.createElement("span");
  label.className = "moe-argument-label";
  label.textContent =
    effectiveOption(argument, rememberedArgument(command.id, argument.id))?.title ?? argument.title;
  chip.append(label, iconEl("chevron-down", { size: 12, className: "shrink-0" }));
  // A click must not take focus from the Input Bar: the card's own input takes it instead.
  chip.addEventListener("mousedown", (e) => e.preventDefault());
  chip.addEventListener("click", () => {
    if (open) closeArgumentCard();
    else openArgumentCard(command, argument);
  });
  return chip;
}

/** Open the picker for one argument (the cards are peers: the others close). */
function openArgumentCard(command: CommandMeta, argument: CommandArgument) {
  closeActionsCard();
  closeAboutCard();
  closeFilterCard();
  openArgument = { command, argument };
  const effective = effectiveOption(argument, rememberedArgument(command.id, argument.id))?.id;
  argumentSearchEl.placeholder = `Filter ${argument.title.toLowerCase()}…`;
  argumentCard.open([
    {
      title: argument.title,
      rows: argument.options.map((option) => ({
        title: option.title,
        checked: option.id === effective,
        run: () => {
          setArgumentValue(command.id, argument.id, option.id);
          draftEl().focus();
        },
      })),
    },
  ]);
  renderArgumentChips();
}

function closeArgumentCard() {
  // `onClose` clears `openArgument` and re-renders the chips, so the accent ring goes with the card.
  argumentCard.close();
}

/**
 * Tab (ADR-0045): focuses the focused command's first argument, then around — the key Raycast uses
 * for arguments, and the reason the chips are the one Input Bar element the pointer never has to
 * reach for. False = nothing to focus, so the key keeps whatever meaning it had.
 */
function focusArgument(): boolean {
  const command = focusedCommand();
  const declared = declaredArguments(command);
  if (!command || declared.length === 0) return false;
  const index = openArgument
    ? declared.findIndex((argument) => argument.id === openArgument?.argument.id)
    : -1;
  const next = declared[(index + 1) % declared.length];
  // One declared argument means Tab lands on the picker that is already open: keep it (and what the
  // user typed into it) instead of reopening — Tab must never clear the filter.
  if (index !== -1 && next.id === openArgument?.argument.id) return true;
  openArgumentCard(command, next);
  return true;
}

// The Filter Control is the card's trigger, so a click on it toggles rather than counting as "outside"
filterControlEl.addEventListener("mousedown", (e) => e.preventDefault());
filterControlEl.addEventListener("click", () => {
  if (filterCard.isOpen()) closeFilterCard();
  else openFilterCard();
});

// ---- Quick Ask page (the panel's conversation, ADR-0036) ----

/** The conversation the page shows; null = blank new chat (the first send creates one). */
let chatConversationId: string | null = null;
/** An answer is streaming: Enter stops it instead of sending (ADR-0036). */
let chatGenerating = false;
/** A send is in flight: Enter must not double-send while the stop check awaits (MOE-0030 r9). */
let chatSending = false;
/** The conversation title (first user message): the Input Bar's placeholder. */
let chatTitle: string | null = null;
/** The active provider · model shown under the page title (MOE-0033); null until fetched / unset. */
let chatPair: ActivePair | null = null;
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

/**
 * The Quick Ask page's composer row and model switcher (MOE-0030): the page wears the root
 * page's bottom-bar language — round glass add button, one capsule-like one-row input, round
 * glass actions button — while the Input Bar above reads the conversation title. "Set AI Model"
 * lives as a row inside the actions card the round button opens.
 */
const chatComposer = createComposer({
  placeholder: "Ask anything",
  onSend: () => void sendChatMessage(),
  onStop: () => void stopChatGeneration(),
  onActions: () => toggleActionsCard(),
});
chatComposerHostEl.append(chatComposer.root);

/**
 * The composer's target chip (MOE-0059): the selection the next message will carry, shown only while
 * the page is a blank conversation with no send in flight — once the first message leaves, the
 * selection is already inside it (ADR-0019/0021) and the chip has nothing left to promise. The rules
 * (which page, which states, what each says) live in ui/src/selection.ts.
 */
function renderSelectionTarget() {
  const v = view.get();
  const indicator = chatTarget(
    { mode: v.mode, armed: chatConversationId === null && !chatSending },
    selectionState,
  );
  chatTargetRowEl.classList.toggle("hidden", indicator === undefined);
  if (!indicator) {
    hideTargetPeek();
    return;
  }
  targetIndicator = indicator;
  chatTargetEl.dataset.state = indicator.state;
  const label = document.createElement("span");
  label.className = "moe-chat-target-label";
  label.textContent = indicator.label;
  const parts = [iconEl(indicator.icon, { size: 12, className: "shrink-0" }), label];
  if (indicator.detail) {
    const detail = document.createElement("span");
    detail.className = "moe-chat-target-detail";
    detail.textContent = indicator.detail;
    parts.push(detail);
  }
  chatTargetEl.replaceChildren(...parts);
}

/** The chip's current words: the peek card's caption is the indicator's own explanation. */
let targetIndicator: ReturnType<typeof chatTarget> = undefined;
/** The card is up (or its text is being fetched): the pointer is on the chip or the card. */
let targetPeekOpen = false;
let targetPeekCloseTimer: ReturnType<typeof setTimeout> | undefined;

/**
 * Read the captured selection itself (MOE-0059), once per reported state. On demand only: the chip
 * renders from the shape, so the text crosses into the panel exactly when the user asks to look.
 */
async function ensureSelectionDetail() {
  const requested = selectionState;
  if (selectionDetailFor === requested) return;
  selectionDetailFor = requested;
  try {
    selectionDetail = await invoke<SelectionDetail | null>("selection_detail");
  } catch {
    selectionDetail = null;
    selectionDetailFor = null; // a failed read is not cached; the next hover tries again
  }
  if (targetPeekOpen && selectionState === requested) paintTargetPeek();
}

/**
 * The peek card: the captured selection in the house card language — a caption saying what the chip
 * means (the chip carries no native tooltip; it would fight this card), then the text and the Finder
 * paths, in a column that scrolls rather than growing.
 */
function paintTargetPeek() {
  const detail = selectionDetail;
  if (!detail) return;
  const caption = document.createElement("div");
  caption.className = "moe-peek-caption";
  caption.textContent = targetIndicator?.title ?? "";
  const children: HTMLElement[] = [caption];
  const text = detail.text && detail.text.trim() !== "" ? detail.text : null;
  if (text !== null || detail.files.length > 0) {
    const body = document.createElement("div");
    body.className = "moe-peek-scroll";
    if (text !== null) {
      const pre = document.createElement("pre");
      pre.className = "moe-peek-text";
      pre.textContent = text; // user content, never markup
      body.append(pre);
    }
    if (detail.files.length > 0) {
      const list = document.createElement("ul");
      list.className = "moe-peek-files";
      for (const path of detail.files) {
        const item = document.createElement("li");
        item.textContent = path;
        item.title = path;
        list.append(item);
      }
      body.append(list);
    }
    children.push(body);
  }
  chatTargetPeekEl.replaceChildren(...children);
  chatTargetPeekEl.classList.remove("hidden");
}

/** Show the peek (and fetch the text behind it if this state's detail is not in hand yet). */
function showTargetPeek() {
  clearTimeout(targetPeekCloseTimer);
  targetPeekOpen = true;
  if (selectionDetailFor !== selectionState) {
    void ensureSelectionDetail();
    return;
  }
  if (chatTargetPeekEl.classList.contains("hidden")) paintTargetPeek();
}

function hideTargetPeek() {
  clearTimeout(targetPeekCloseTimer);
  targetPeekOpen = false;
  chatTargetPeekEl.classList.add("hidden");
}

/** Leaving either the chip or the card is not final: the pointer crosses a gap to reach the card. */
function scheduleHideTargetPeek() {
  clearTimeout(targetPeekCloseTimer);
  targetPeekCloseTimer = setTimeout(hideTargetPeek, 160);
}

chatTargetEl.addEventListener("mouseenter", showTargetPeek);
chatTargetEl.addEventListener("mouseleave", scheduleHideTargetPeek);
chatTargetPeekEl.addEventListener("mouseenter", () => clearTimeout(targetPeekCloseTimer));
chatTargetPeekEl.addEventListener("mouseleave", scheduleHideTargetPeek);

const modelSwitcher = createModelSwitcher({
  host: rootEl,
  // Same anchor as the panel's actions card: bottom-right, above the composer row that opens it.
  // The companion detail card floats to the left of the list (the panel is wide enough for the pair).
  anchorClass: "bottom-14 right-2",
  cardWidth: "w-80",
  onApplied: (pair: ActivePair) => {
    chatPair = pair; // the page title's subtitle follows the switch (MOE-0033)
    syncChatTopbar();
    toast(`${pair.providerName} · ${pair.modelName} is now the active model`);
  },
  onError: (message: string) => toast(`Failed to switch model: ${message}`, "alert"),
  onClose: () => draftEl().focus(),
});

// On the chat page the Input Bar is a read-only title, not an input: clicks land in the composer.
q.addEventListener("focus", () => {
  if (view.get().mode === "chat") chatComposer.focus();
});

/** The input that owns the draft: the Input Bar, or the chat page's composer capsule. */
function draftEl(): HTMLInputElement {
  return view.get().mode === "chat" ? chatComposer.input : q;
}

/** One generating flag for the page (the actions card's Stop row follows it). */
function setChatGenerating(on: boolean) {
  chatGenerating = on;
}

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

async function initActionBar() {
  await loadKeyDisplay();
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
      disabled: draftEl().value.trim() === "",
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
      accessories: rowAccessories(i),
    };
  });
}



/**
 * The dimension this page can actually filter by (ADR-0043): nothing on the command layer, nothing on
 * a full-detail page (a single result has nothing to narrow), and nothing when the result declared no
 * dimension. One predicate, so the Filter Control, the Filter Card and the ⌘K row can never disagree
 * about whether the page has one.
 */
function filterableDimension(): Filter | undefined {
  const v = view.get();
  if (v.mode !== "items") return undefined;
  const shape = pageShapeOf(v.items[v.focus], v.detailFull === true, v.items.length);
  return shape === "detail" ? undefined : v.filter;
}

/**
 * The Filter Control (ADR-0043): shown only on a result page that declares a dimension, because a
 * filter exists on some pages and not others. Label at rest ("Tags"), label + value when set
 * ("Tags: Agentic"), so the bar says at a glance whether the list is narrowed.
 */
function renderFilterControl() {
  const declared = filterableDimension();
  filterControlEl.classList.toggle("hidden", declared === undefined);
  if (!declared) return;
  const v = view.get();
  filterControlEl.dataset.active = v.filterValue !== undefined ? "true" : "false";
  // While the page streams, the control says so rather than silently swallowing a change (MOE-0046).
  const frozen = !filterChangeAllowed(v.allItems);
  filterControlEl.disabled = frozen;
  filterControlEl.title = frozen
    ? `Filter by ${declared.label} — unavailable while generating`
    : `Filter by ${declared.label} (${GENERAL_KEY_LABELS.browse})`;
  const label = document.createElement("span");
  label.className = "moe-filter-label";
  label.textContent = v.filterValue ? `${declared.label}: ${v.filterValue}` : declared.label;
  filterControlEl.replaceChildren(
    label,
    iconEl("chevron-down", { size: 12, className: "shrink-0" }),
  );
}

/**
 * The Selection Indicator (MOE-0057): shown while the focused command takes a selection, saying
 * whether the summon actually captured one — the answer a selection command's guidance card would
 * otherwise give only after the fact. Same glass and height rule as the Filter Control; the rules
 * (which command it speaks for, which state it shows) live in ui/src/selection.ts.
 */
function renderSelectionState() {
  const indicator = selectionIndicator(view.get(), selectionState);
  selectionStateEl.classList.toggle("hidden", indicator === undefined);
  if (!indicator) return;
  selectionStateEl.dataset.state = indicator.state;
  selectionStateEl.title = indicator.title;
  const label = document.createElement("span");
  label.className = "moe-selection-label";
  label.textContent = indicator.label;
  selectionStateEl.replaceChildren(
    iconEl(indicator.icon, { size: 12, className: "shrink-0" }),
    label,
  );
}

/**
 * Read the captured selection's shape (MOE-0057): once at boot. From there the backend's
 * `selection-changed` events keep it current — the show path emits on every summon, and the
 * clipboard-fallback worker emits again when a late capture settles (MOE-0013).
 */
async function refreshSelectionState() {
  try {
    selectionState = await invoke<SelectionState>("selection_state");
  } catch {
    // A failed read leaves the chip saying "no selection" rather than a stale claim.
    selectionState = NO_SELECTION;
  }
  renderSelectionState();
}

function render() {
  const v = view.get();
  // One clock reading and one focus sink for every row this render (ADR-0041/MOE-0053): handed to the
  // renderer rather than reached for by it, which is what lets a test drive `rowEl`.
  const rowDeps: RowDeps = {
    nowUnix: Math.floor(Date.now() / 1000),
    onFocus: (index) => view.update((s) => ({ ...s, focus: index })),
  };
  // The Input Bar's leading slot follows the page layer (Moe logo → back arrow, ADR-0034)
  renderInputIcon();
  // The Filter Control follows the page's declared dimension (ADR-0043)
  renderFilterControl();
  // The Selection Indicator follows the focused command's input kind (MOE-0057)
  renderSelectionState();
  // The argument chips follow the focused command's declared arguments (ADR-0045)
  renderArgumentChips();
  // The composer's target chip follows the page and its conversation (MOE-0059)
  renderSelectionTarget();
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
          rowDeps,
        );
        if (flat === v.focus) focusedLi = li;
        flat += 1;
        lis.push(li);
      }
    }
    // Zero rows on the command layer (ADR-0043): the platform supplies the copy, because no
    // Extension owns the palette itself. The old behaviour was a blank panel.
    if (flat === 0 && !(searchLoading && q.value.trim() === "")) {
      lis.push(emptyViewEl({ title: "No commands match", description: "Try fewer words." }));
    }
    listEl.replaceChildren(...lis);
  } else if (v.mode === "items") {
    // Grouped list page (ADR-0018 amendment): `Item.group` labels render as non-focusable headers
    // and the flat item index (focus, navigation, actions, delete) never counts them — the same
    // rule as the command layer's source sections (ADR-0020).
    const entries = currentEntries();
    const lis: HTMLLIElement[] = [];
    let lastGroup: string | undefined;
    v.items.forEach((item, i) => {
      if (item.group && item.group !== lastGroup) lis.push(sectionHeaderEl(item.group));
      lastGroup = item.group;
      const li = rowEl(entries[i], i, i === v.focus, rowDeps);
      if (i === v.focus) focusedLi = li;
      lis.push(li);
    });
    // Zero rows (ADR-0043): never a blank panel. Suppressed while a query-free load is in flight, so
    // the first paint cannot flash "no results" (Raycast's rule).
    if (v.items.length === 0) {
      if (!(searchLoading && q.value.trim() === "")) {
        // When the *tag* filter is what emptied the list, the copy has to be the platform's: the
        // Extension's Empty View ("No matching chat history") is wrong when the query matched and
        // the tag did not.
        const narrowedAway = v.filterValue !== undefined && v.allItems.length > 0;
        lis.push(
          emptyViewEl(
            narrowedAway
              ? {
                  title: `No matches for “${v.filterValue}”`,
                  description: "Clear the filter to see everything.",
                }
              : (v.empty ?? { title: "No results" }),
          ),
        );
      }
    }
    listEl.replaceChildren(...lis);
  } else {
    // Quick Ask page (ADR-0036): the detail pane is the conversation, the list is away
    listEl.classList.add("hidden");
    detailEl.className = CONVERSATION_PANE_CLASS;
    syncChatHint();
  }
  // Chat page chrome (MOE-0030): the composer row replaces the command layer's bottom bar, and
  // the Input Bar becomes the page's read-only title; leaving the page restores both.
  const chatPage = v.mode === "chat";
  chatComposerHostEl.classList.toggle("hidden", !chatPage);
  bottomBarEl.classList.toggle("hidden", chatPage);
  syncChatTopbar();
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
  } else if (v.mode !== "chat") {
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





/** What one paint of the detail pane needs. The item is present in both shapes — it carries the
 *  metadata — while `header` is the split shape's header row only. */
interface DetailPaint {
  markdown: string;
  itemId: string | null;
  pending: boolean;
  item?: Item;
  header?: boolean;
  reasoning?: string;
}

function paintDetail(paint: DetailPaint) {
  const { markdown, itemId, pending, item, header, reasoning } = paint;
  const nearBottom =
    detailEl.scrollHeight - detailEl.scrollTop - detailEl.clientHeight < 40;
  // The card re-renders per stream frame: carry the user's expand toggle and the thinking timer
  // across the rebuild (MOE-0012; the row never auto-opens).
  const reasoningWasOpen =
    detailEl.querySelector<HTMLDetailsElement>("details.moe-thinking")?.open ?? false;
  if (itemId !== detailItemId) detailThinkingStartedAt = null;
  // A fresh run of the same item (a rerun) starts with a reasoning-less placeholder: re-arm the timer
  if (pending && !reasoning?.trim()) detailThinkingStartedAt = null;
  const parts = ensureDetailParts();
  const body = parts.body;
  body.replaceChildren();
  if (header && item) body.append(detailHeaderEl(item));
  // The model's thinking (reasoning blocks, MOE-0008) renders as the muted ChatGPT-style row above
  // the answer; the body only ever holds the answer text
  if (reasoning?.trim()) {
    detailThinkingStartedAt ??= Date.now();
    const seconds = pending ? 0 : Math.round((Date.now() - detailThinkingStartedAt) / 1000);
    const block = reasoningEl(reasoning, pending, seconds);
    block.open = reasoningWasOpen;
    body.append(block);
  }
  const prose = document.createElement("div");
  prose.className = "md";
  prose.innerHTML = DOMPurify.sanitize(marked.parse(markdown, { async: false }));
  // ADR-0042's two placements from one renderer (MOE-0047): the `split` shape puts the block under the
  // Markdown at the pane's bottom; the `detail` shape puts it in a right-hand column beside the
  // Markdown, the two halves scrolling on their own — which is why `renderDetail` switches the pane
  // itself to `DETAIL_COLUMNS_CLASS` when a column is coming.
  const metadata = item?.metadata ?? [];
  // The block's side effects, passed in rather than imported by the renderer (MOE-0052): the block
  // itself lives in `metadata.ts`, where a jsdom test can reach it.
  const metadataDeps: MetadataDeps = {
    dimensionValues: view.get().filterValues,
    onFilter: setFilterValue,
    onAction: (target, action) => void runItemAction(target, action),
    onOpenLink: (target) => {
      // Only http(s) reaches the OS (`open::open_url`), so a bad target fails without spawning anything.
      void invoke("open_external", { url: target }).catch(() =>
        toast("Could not open that link"),
      );
    },
  };
  if (!header && item && metadata.length > 0) {
    body.className = "flex min-h-0 flex-1 gap-4";
    const proseColumn = document.createElement("div");
    proseColumn.className = "min-h-0 min-w-0 flex-1 overflow-y-auto";
    proseColumn.append(prose);
    const column = document.createElement("div");
    column.className = "moe-metadata-column";
    column.append(metadataEl(metadata, item, metadataDeps));
    body.append(proseColumn, column);
  } else {
    body.className = "";
    body.append(prose);
    if (metadata.length > 0 && item) body.append(metadataEl(metadata, item, metadataDeps));
  }
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

/** When the shown detail's reasoning first appeared (the "Thought for N seconds" timer, MOE-0012). */
let detailThinkingStartedAt: number | null = null;

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
  detailThinkingStartedAt = null;
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
  paintDetail({ markdown, itemId: null, pending: false });
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
  // The `detail` shape switches to the two-column pane only when there is a column to fill; with no
  // metadata it is byte-identical to before (ADR-0042's additive claim).
  detailEl.className =
    full && (item.metadata?.length ?? 0) > 0 ? DETAIL_COLUMNS_CLASS : DETAIL_MESSAGE_CLASS;
  const reasoning = (item.payload as { reasoning?: string } | null)?.reasoning;
  paintDetail({
    markdown: item.detail ?? "",
    itemId: item.id,
    pending: item.pending === true,
    item,
    header: !full,
    reasoning,
  });
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

/** Chat state changed outside render(): hint + top bar title + action pill. */
function refreshChatChrome() {
  syncChatHint();
  syncChatTopbar();
  updatePlaceholder();
  renderActionBar();
}

/**
 * The chat page's top bar is the Input Bar itself (MOE-0030 revision, the command page's own
 * design): the leading slot keeps the back arrow (ADR-0034) and the field reads the conversation
 * title, read-only — a title, not a draft.
 */
function syncChatTopbar() {
  if (view.get().mode === "chat") {
    q.readOnly = true;
    q.value = chatTitle ?? "";
    q.placeholder = "New Chat";
    // The active pair under the title (MOE-0033): smaller and muted, so the title leads.
    const pair = chatPair;
    querySubtitleEl.textContent = pair ? `${pair.providerName} · ${pair.modelName}` : "";
    querySubtitleEl.classList.toggle("hidden", !pair);
  } else if (q.readOnly) {
    q.readOnly = false;
    q.value = "";
    querySubtitleEl.textContent = "";
    querySubtitleEl.classList.add("hidden");
  }
}

/** The title's subtitle (MOE-0033): the active provider · model, refreshed on page entry and on
 *  a switch through the switcher; a missing pair leaves the line hidden. */
async function refreshChatPair() {
  try {
    chatPair = await invoke<ActivePair | null>("ai_active_pair");
  } catch {
    chatPair = null;
  }
  syncChatTopbar();
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
    // An unanswered question means a reply is on its way (the ask that opened the page) — or it
    // is not: a request that failed after persisting the question (or an app restart) would leave
    // this page "generating" forever, swallowing every Enter into a silent Stop. The hint opens
    // the page in waiting mode; `verifyPendingAnswer` checks the backend's live-stream registry
    // shortly after, so a phantom state is repaired instead of trapping the chat (MOE-0030 r9).
    const lastIsQuestion = messages[messages.length - 1]?.role === "user";
    setChatGenerating(lastIsQuestion);
    if (chatGenerating) parts.view.updateStreaming("", true);
    parts.view.scrollToEnd();
    if (lastIsQuestion) void verifyPendingAnswer(id);
  } catch (err) {
    setChatError(`Failed to load the chat: ${String(err)}`);
  }
  refreshChatChrome();
}

/**
 * The page loaded with an unanswered question: after a grace period (the worker registers its
 * stream on spawn, which can trail this load by a beat), ask whether a generation is really live.
 * If it is not, the waiting state is a phantom: settle it and say so instead of eating the next
 * Enter (MOE-0030 revision 9).
 */
async function verifyPendingAnswer(id: string) {
  await new Promise((resolve) => setTimeout(resolve, 1500));
  if (chatConversationId !== id || !chatGenerating) return; // stepped away, or frames settled it
  let live = true;
  try {
    live = await invoke<boolean>("conversation_streaming", { conversationId: id });
  } catch {
    // An older backend without the IPC: assume live (never invent a phantom warning).
    return;
  }
  if (live || chatConversationId !== id || !chatGenerating) return;
  setChatGenerating(false);
  const parts = ensureChatParts();
  parts.view.settleStreaming();
  parts.view.discardAnswer(); // the waiting bubble was never a real answer
  setChatError(
    "The previous question never received an answer — the request may have failed (the app log has details), or Moe restarted. Ask again, or start a new chat (⌘N).",
  );
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
  modelSwitcher.close();
  chatComposer.closeAttach();
  clearDetail(); // resets the detail pane; the chat parts rebuild on the next render
  chatConversationId = conversationId;
  setChatGenerating(false);
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
  void refreshChatPair();
  draftEl().focus();
}

/** Enter on the page (ADR-0036): send the draft into the conversation and clear the input. */
async function sendChatMessage() {
  if (chatSending) return;
  const message = chatComposer.input.value.trim();
  if (!message) return;
  const parts = ensureChatParts();
  chatSending = true;
  // The target is consumed the moment the first message leaves: the chip goes with it, and comes back
  // only if the send is rolled back below (the selection never made it into a message).
  renderSelectionTarget();
  try {
    if (chatGenerating) {
      // Enter-as-stop (ADR-0036) — unless nothing is actually generating: the backend returns 0 for
      // a stale flag (a request that failed after persisting the question, an app restart), and a
      // Stop that stops nothing must not swallow the question (MOE-0030 revision 9).
      const stopped = await stopChatGeneration();
      if (stopped > 0) return;
      // Nothing was actually streaming: the waiting bubble was never a real answer, drop it.
      parts.view.discardAnswer();
    }
    // The page's command is always known: an entry that somehow lost it still sends as Quick Ask.
    const commandId = view.get().sourceCommandId ?? "ai.quick-ask";
    setChatError(null);
    chatComposer.input.value = "";
    const row = parts.view.append({ role: "user", content: message });
    chatTitle = chatTitle ?? conversationTitle([{ role: "user", content: message }]);
    // The generating state goes up before the invoke resolves (the worker starts answering as soon
    // as it spawns): the indicator and the stop affordance must never lag behind the send.
    parts.view.beginAnswer();
    setChatGenerating(true);
    parts.view.updateStreaming("", true);
    parts.view.scrollToEnd();
    refreshChatChrome();
    try {
      chatConversationId = await invokeWithTimeout<string>(
        "panel_send",
        {
          commandId,
          conversationId: chatConversationId ?? "",
          message,
        },
        "The send",
      );
      // The conversation appears in the history list (and in side_conversations) right away
      void loadChatConversations();
    } catch (err) {
      // Rollback: the message was not persisted; restore the draft so the user does not retype it,
      // and the empty answer bubble must not linger.
      row.remove();
      parts.view.discardAnswer();
      chatComposer.input.value = message;
      setChatError(String(err));
      toast(String(err), "alert");
      setChatGenerating(false);
      refreshChatChrome();
    }
  } finally {
    chatSending = false;
    renderSelectionTarget();
  }
}

/** Stop the stream (Enter while generating, or the dialog): the generated part is kept (IIE4AD-365).
 *  Returns how many streams were actually aborted — 0 means nothing was generating (a stale flag),
 *  which lets Enter send instead of eating the question (MOE-0030 revision 9). */
async function stopChatGeneration(): Promise<number> {
  let stopped = 0;
  try {
    stopped = await invoke<number>("stop_generation");
  } catch (err) {
    setChatError(String(err));
  }
  setChatGenerating(false);
  ensureChatParts().view.settleStreaming();
  refreshChatChrome();
  return stopped;
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
  draftEl().focus();
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

/**
 * Set the active filter value and re-widen `items` from `allItems`. Focus returns to the first row:
 * the previous index pointed into a different list.
 */
function setFilterValue(value: string | undefined) {
  const current = view.get();
  // Frozen while a result streams (MOE-0046): the change would narrow the list out from under the
  // frames still arriving for the focused item — the filter's version of orphaning a generation.
  // Checked against `allItems`: the stream runs whether or not the active filter happens to show it.
  if (!filterChangeAllowed(current.allItems)) return;
  if (current.sourceCommandId) {
    if (value === undefined) filterMemory.delete(current.sourceCommandId);
    else filterMemory.set(current.sourceCommandId, value);
  }
  view.update((v) => ({
    ...v,
    filterValue: value,
    items: applyTagFilter(v.allItems, value),
    focus: 0,
  }));
}

/**
 * Remembered filter values, session-scoped and keyed by command (ADR-0043 / MOE-0046). The command id
 * is `{extension}.{command}` by convention, so it carries both halves of the key. Deliberately not a
 * preference and not persisted: Raycast's `storeValue` is a cache ("restored next time the dropdown is
 * rendered"), so a fresh process remembers nothing. A fresh entry through ⌘P / ⌘N clears the key
 * (`openEntry`); any other re-entry within the session keeps it.
 */
const filterMemory = new Map<string, string>();

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
  if ("rerun" in res) {
    // The refine loop is intercepted platform-side: it re-invokes the command and returns *that*
    // call's result (the new streaming card). A raw value here would mean the intercept was skipped.
    return;
  }
  if ("openSideView" in res) {
    // The side view window is shown by the backend and receives the payload; the panel is already hidden
    return;
  }
  if ("openSettings" in res) {
    // The settings window is shown by the backend (MOE-0029); the panel is already hidden
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
    const allItems = res.list.items;
    const filter = res.list.filter;
    closeActionsCard();
    closeFilterCard();
    clearDetail();
    view.update((v) => {
      // The dimension's values come from the returned set *before* the tag filter narrows anything,
      // so the picker cannot lose values as the list narrows (ADR-0043).
      const filterValues = filterValuesFor(filter, allItems);
      // The remembered value is keyed by command and lives for the session only (MOE-0046): a re-entry
      // keeps the filter, a fresh process remembers nothing. A value the new set no longer carries
      // falls back to All **and is forgotten**, not merely ignored — the derived list is the authority
      // (see filter.ts).
      const remembered = commandId ? filterMemory.get(commandId) : undefined;
      const filterValue = settleFilterValue(remembered, filterValues);
      if (commandId) {
        if (filterValue === undefined) filterMemory.delete(commandId);
        else filterMemory.set(commandId, filterValue);
      }
      return {
        ...v,
        mode: "items",
        allItems,
        items: applyTagFilter(allItems, filterValue),
        filter,
        filterValues,
        filterValue,
        focus: 0,
        sourceCommandId: commandId,
        sourceLive: live,
        sourceIcon: icon,
        sourceTitle: title,
        detailFull: res.list.detailFull === true,
        empty: res.list.empty,
      };
    });
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
  // A fresh entry through ⌘P / ⌘N resets the filter (MOE-0046): the generic entry points start clean,
  // while any other re-entry within the session keeps the remembered value.
  filterMemory.delete(command.id);
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
      // The values the command will run with (ADR-0045): the panel owns them for the session, so
      // even the declared default travels explicitly.
      arguments: argumentValuesFor(cmd),
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
        // A re-run keeps the page's arguments (ADR-0045): "Translate to Chinese Again" must not
        // fall back to auto and translate the Chinese result back into English.
        arguments: argumentValuesFor(pageCommand()),
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
    filter: filterableDimension(),
    filterValue: v.filterValue,
  };
}

/**
 * "Command" (ADR-0029): the page command's own rows — one menu for the root and for inside a command.
 * Open Command leads on the root (the same Apply path as Enter); inside a command the row is omitted
 * ("opening" the view you are already in would reset it). Then the favorite toggle, Remove from
 * Suggestions on a Suggestions row (⌃X, ADR-0025), and the Configure Extension placeholder.
 */
async function commandCardRows(ctx: PageContext): Promise<ActionRow[]> {
  if (!ctx.commandId) return [];
  const commandId = ctx.commandId;
  let favorited = false;
  try {
    favorited = await invoke<boolean>("is_favorite", { commandId });
  } catch {
    // Ignore: the toggle still works, the label just may be stale
  }
  const rows: ActionRow[] = [];
  if (ctx.mode === "commands") {
    rows.push({
      title: "Open Command",
      icon: "corner-down-left",
      keys: keyDisplay.get("apply") ?? "⏎",
      rank: "primary",
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
      rank: "destructive",
      run: () => void deleteSuggestion(commandId),
    });
  }
  rows.push({
    title: "Configure Extension",
    icon: "settings-2",
    rank: "settings",
    disabled: true,
    run: () => {},
  });
  return rows;
}

/**
 * "Actions": the object's own actions. A result page's object is its focused item — the shape
 * (list / split / detail) never changes it — while a chat page's object is the conversation.
 */
function objectCardRows(ctx: PageContext): ActionRow[] {
  if (ctx.mode === "chat") {
    const hasAnswer = lastAnswerText() !== null;
    return [
      // Switching the model is an action of the conversation (MOE-0030 revision): the round
      // actions button's card carries it, not a header control; ⌥M lands the same action (MOE-0033).
      {
        title: "Change Model",
        icon: "sparkles",
        keys: keyDisplay.get("changeModel") ?? "⌥M",
        run: () => modelSwitcher.open(),
      },
      ...(chatGenerating
        ? [
            {
              title: "Stop Generation",
              icon: "stop",
              keys: keyDisplay.get("apply") ?? "⏎",
              // While a stream runs, ⏎ stops it: the card's primary slot is Stop (ADR-0036).
              rank: "primary" as const,
              run: () => void stopChatGeneration(),
            },
          ]
        : []),
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
        rank: "secondary",
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
  // The item's own declaration (ADR-0006): the first action carries the Apply semantic (⏎); the
  // declared kind is also the row's canonical slot — primary first, secondaries next.
  return item.actions.map((action) => ({
    title: action.title,
    icon:
      action.id === "rerun"
        ? "wand-2"
        : action.kind === "primary"
          ? "corner-down-left"
          : "copy",
    keys: action.keybinding ?? (action.kind === "primary" ? "⏎" : null),
    rank: action.kind === "primary" ? ("primary" as const) : ("secondary" as const),
    run: () => void runItemAction(item, action),
  }));
}

/**
 * "General" (ADR-0014/0022): the platform general slots as they are live on this page — Browse ⌘P /
 * New ⌘N resolved from the Extension's declared entries, plus the delete slots where the page owns
 * records (a result page, or the chat page as Remove Chat / Remove All Chats, ADR-0036).
 */
async function pageCardRows(ctx: PageContext): Promise<ActionRow[]> {
  const rows: ActionRow[] = [];
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
        // ⌘P belongs to the Filter Control on a page that declares a dimension (ADR-0043's fourth
        // amendment). Raycast's model is one owner per chord, so the row drops its Kbd there rather
        // than advertising a key that would open the filter — the action stays clickable.
        keys: kind === "browse" && filterableDimension() ? undefined : GENERAL_KEY_LABELS[kind],
        run: () => void openEntry(kind),
      });
    });
  }
  if (ctx.mode === "items") {
    rows.push({
      title: "Delete Record",
      icon: "trash-2",
      keys: GENERAL_KEY_LABELS.delete,
      rank: "destructive",
      run: () => void deleteFocused(false),
    });
    rows.push({
      title: "Delete All Records",
      icon: "trash-2",
      keys: GENERAL_KEY_LABELS.deleteAll,
      rank: "destructive",
      run: () => void deleteFocused(true),
    });
  } else if (ctx.mode === "chat") {
    rows.push({
      title: "Remove Chat",
      icon: "trash-2",
      keys: GENERAL_KEY_LABELS.delete,
      rank: "destructive",
      disabled: !chatConversationId,
      run: () => void removeCurrentChat(),
    });
    rows.push({
      title: "Remove All Chats",
      icon: "trash-2",
      keys: GENERAL_KEY_LABELS.deleteAll,
      rank: "destructive",
      run: () => void deleteAllChats(),
    });
  }
  // The settings window (MOE-0029) is reachable from every page's card, like the tray item.
  rows.push({
    title: "Open Settings",
    icon: "settings-2",
    rank: "global",
    run: () => void invoke("open_settings_window"),
  });
  return rows;
}

/** The panel's card (ADR-0037): Actions / Command / General, each canonically ordered and
 *  ordered among themselves by their topmost row (MOE-0033). */
/**
 * The page's filter rows (ADR-0043): the ⌘K card is where Raycast puts the same affordance
 * (`ActionPanel.Submenu`), and it shows the control's chord — ⌘P, the dropdown's key, which Browse
 * yields on any page that declares a dimension (ADR-0043 amendment).
 */
function filterCardRows(ctx: PageContext): ActionRow[] {
  const filter = ctx.filter;
  if (!filter) return [];
  const rows: ActionRow[] = [
    {
      title: `Filter by ${filter.label}`,
      icon: "filter",
      keys: GENERAL_KEY_LABELS.browse,
      run: () => openFilterCard(),
    },
  ];
  if (ctx.filterValue !== undefined) {
    rows.push({
      title: `Clear ${filter.label} filter`,
      icon: "close",
      run: () => setFilterValue(undefined),
    });
  }
  return rows;
}

const panelActionPlan = createActionPlan([
  { title: "Actions", when: (ctx) => ctx.mode !== "commands", rows: objectCardRows },
  { title: "Command", when: (ctx) => ctx.commandId !== undefined, rows: commandCardRows },
  { title: "Filter", when: (ctx) => ctx.filter !== undefined, rows: filterCardRows },
  { title: "General", when: () => true, rows: pageCardRows },
]);

/**
 * Open the actions card (Raycast-style floating card, Show All Actions ⌘K): the registered sources
 * for the current page, one section each — the call site no longer merges rows by page kind
 * (ADR-0037). The body (command list / results) stays in place behind the card.
 */
async function openActionsCard() {
  closeAboutCard(); // the two cards are peers: opening one closes the other (like openAboutCard)
  closeFilterCard();
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
      arguments: argumentValuesFor(pageCommand()),
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

// ---- About card (ADR-0026): the avatar chip's menu.
// One component since ADR-0044: this is `createCard` — the same card the actions card and the Side
// View's use — so the filter, ↑↓/⏎, Esc / empty-⌫, click-outside and the key discipline are not
// re-implemented here. Only the rows and the anchor are this module's, and neither needs a renderer.

/** Feedback lands in the repository's issue tracker. */
const FEEDBACK_URL = "https://github.com/limoiie/moe/issues";

/** Appearance rows (ADR-0035 amendment): System follows the OS; the choice persists to config.toml. */
const THEME_ROWS: { value: ThemePreference; title: string; icon: string }[] = [
  { value: "system", title: "System", icon: "monitor" },
  { value: "light", title: "Light", icon: "sun" },
  { value: "dark", title: "Dark", icon: "moon" },
];

/** The About menu: the app's own meta actions (ADR-0026), grouped into App / Theme / Support sections (ADR-0026/0035 amendments). */
function aboutSections(): CardSection[] {
  return [
    {
      title: "App",
      rows: [
        {
          title: "Open Config File",
          icon: "settings-2",
          keys: GENERAL_KEY_LABELS.openConfig,
          run: () => void runAboutConfig(),
        },
        {
          title: "Save AI Key",
          icon: "key-round",
          run: () => void runAboutKey(),
        },
      ],
    },
    {
      title: "Theme",
      rows: THEME_ROWS.map((option) => ({
        title: option.title,
        icon: option.icon,
        checked: themePreference() === option.value,
        // The card stays open so a second theme can be tried immediately (ADR-0035 amendment)
        keepOpen: true,
        run: () => void runAboutTheme(option.value),
      })),
    },
    {
      title: "Support",
      rows: [
        {
          title: "Send Feedback",
          icon: "megaphone",
          run: () => void runAboutFeedback(),
        },
      ],
    },
  ];
}

/**
 * The About card, on the shared component (ADR-0044). Its input sits at the bottom edge and the card
 * itself at the bottom-left, both owned by the markup (ADR-0028 amendment); the rows, sections,
 * filtering, ↑↓/⏎, Esc / empty-⌫ and click-outside all come from `createCard`.
 */
const aboutCard = createCard({
  cardEl: aboutCardEl,
  listEl: aboutListEl,
  inputEl: aboutSearchEl,
  emptyText: "No matching actions",
  onClose: () => draftEl().focus(),
});

// ---- Overlay layers (ADR-0044) ----
// Declared after the last of the three cards exists, because registration order IS the stacking
// order: front-most last. In practice only one is open at a time (each opener closes its peers), but
// the order is the tie-break if that ever changes — and it is why `closeTop()` now closes the model
// switcher in the panel, which the old if-chain forgot to do.
const panelLayers = createLayers();
panelLayers.register({ name: "actions card", isOpen: actionsCard.isOpen, close: actionsCard.close });
panelLayers.register({ name: "about card", isOpen: aboutCard.isOpen, close: aboutCard.close });
panelLayers.register({
  name: "filter card",
  isOpen: filterCard.isOpen,
  close: filterCard.close,
});
panelLayers.register({
  name: "model switcher",
  isOpen: modelSwitcher.isOpen,
  close: modelSwitcher.close,
});

async function runAboutConfig() {
  // `createCard` closes the card before it runs a row, so no explicit close is needed here
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

/**
 * ⌘' (ADR-0036 amendment): toggle the Quick Ask page. The global hotkey shows the panel and fires
 * the event below; this is also the in-panel fallback when the chord lost its registration. A panel
 * already on the page is dismissed; otherwise the page is entered — re-showing keeps the
 * conversation that was on screen, entering blank only from another page.
 */
async function toggleQuickAsk(wasVisible: boolean) {
  const onPage = view.get().mode === "chat";
  if (wasVisible && onPage) {
    await invoke("hide_panel");
    return;
  }
  if (!onPage) {
    await openQuickAsk();
    return;
  }
  // The panel was re-shown on the page: keep it and take the input focus back.
  draftEl().focus();
}

/**
 * Enter the Quick Ask page (blank): the same landing as applying the command's row (the search
 * text is never sent as a question).
 */
async function openQuickAsk() {
  closeAboutCard();
  try {
    const res = await invoke<ActionResult>("invoke_command", {
      commandId: "ai.quick-ask",
      query: null,
    });
    q.value = "";
    applyResult(res, "ai.quick-ask", false, "sparkles");
  } catch (err) {
    showMessage(`Failed to run: ${String(err)}`);
  }
}

/**
 * ⌘⇧' (ADR-0036 amendment): toggle the AI chat side window (the `ai.side-chat` command's hotkey).
 * The panel webview cannot read the window's visibility, so this asks the backend for the same
 * toggle the global hotkey runs: visible → hidden, hidden → shown as it was. ⌘J still materializes
 * the panel's current conversation instead.
 */
async function toggleSideChat() {
  try {
    await invoke("toggle_side_chat");
  } catch (err) {
    showMessage(`Failed to run: ${String(err)}`);
  }
}

/** Save AI Key runs the same command as the list: its guidance card lands in the results layer. */
async function runAboutKey() {
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
  try {
    await invoke("open_external", { url: FEEDBACK_URL });
    toast("Opened the feedback page");
  } catch (err) {
    toast(`Failed to open: ${String(err)}`, "alert");
  }
}

/**
 * Switch the appearance (ADR-0035 amendment): Rust persists it to config.toml and broadcasts
 * (both windows re-theme). The row is `keepOpen`, so the card stays up — a theme is worth trying
 * twice — and the refresh keeps the filter text and the focused row where the user left them.
 */
async function runAboutTheme(theme: ThemePreference) {
  const label = THEME_ROWS.find((option) => option.value === theme)?.title ?? theme;
  try {
    await invoke("set_theme", { theme });
    toast(`Theme: ${label}`);
  } catch (err) {
    toast(`Failed to set theme: ${String(err)}`, "alert");
  }
  aboutCard.refresh(aboutSections());
}

/** Open the About card: `createCard` resets its filter, focuses its input and renders the rows. */
function openAboutCard() {
  closeActionsCard();
  closeFilterCard();
  aboutCard.open(aboutSections());
}

function closeAboutCard() {
  aboutCard.close();
}

function toggleAboutCard() {
  if (aboutCard.isOpen()) closeAboutCard();
  else openAboutCard();
}

chipEl.addEventListener("mousedown", (e) => e.preventDefault());
chipEl.addEventListener("click", () => toggleAboutCard());

/** Live commands: re-run the list with the new query as input changes (e.g. "AI: Search Chat History"). */
async function rerunLive(query: string) {
  const v = view.get();
  if (!v.sourceCommandId) return;
  setSearchLoading(true);
  try {
    const res = await invoke<ActionResult>("invoke_command", {
      commandId: v.sourceCommandId,
      query: query || null,
      record: false, // a re-run is not a launch (frecency semantics)
      arguments: argumentValuesFor(pageCommand()),
    });
    applyResult(res, v.sourceCommandId, true, v.sourceIcon, v.sourceTitle);
  } catch (err) {
    showMessage(`Failed to run: ${String(err)}`);
  } finally {
    setSearchLoading(false);
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
        arguments: argumentValuesFor(pageCommand()),
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
  // Overlay layers (ADR-0044): close the front-most card, then back out further only when none is
  // open. The modal states above — the confirm dialog, attachment mode, a streaming item's
  // stop-confirm — keep their own precedence and are deliberately not layers.
  if (panelLayers.closeTop()) return;
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
  setSearchLoading(true);
  try {
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
  } finally {
    setSearchLoading(false);
  }
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
    // On the chat page the composer's own path row serves attachments (MOE-0030); the Input Bar's
    // attachment mode belongs to the command layer.
    if (view.get().mode === "chat") {
      chatComposer.toggleAttach();
      return;
    }
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
  // Tab focuses the focused command's argument chip (ADR-0045) — the panel's own use for the key.
  // It runs before the general actions so a future Tab binding cannot shadow it, and it covers the
  // picker's own input too: the card never stops propagation for Tab, so this listener sees it.
  if (e.key === "Tab" && !e.metaKey && !e.ctrlKey && !e.altKey) {
    if (focusArgument()) {
      e.preventDefault();
      return;
    }
  }
  // The about card is an overlay owned by the chip (ADR-0026). Its input handles its own keys; Esc
  // with the focus elsewhere falls through to `back()`, which pops the layer stack (ADR-0044) — so
  // this no longer needs a card-specific check, and every layer gets the behaviour, not just this one.
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
    if (general === "browse" && filterableDimension()) {
      // Raycast binds the search bar's dropdown to ⌘P (ADR-0043 amendment, reversing divergence #2):
      // a page that declares a dimension gives the chord to its filter, and Browse keeps it everywhere
      // else. Toggles, exactly like a click on the control.
      e.preventDefault();
      if (filterCard.isOpen()) closeFilterCard();
      else openFilterCard();
      return;
    }
    e.preventDefault();
    if (general === "actions") toggleActionsCard();
    else void openEntry(general);
    return;
  }
  // The AI surfaces' launch keys (ADR-0036 amendment): ⌘' toggles the Quick Ask page; ⌘⇧' toggles
  // the AI chat side window. Both are global hotkeys — the OS normally consumes the chord and the
  // events above run — so these branches only matter when the registration lost the chord to
  // another app; shift+' is "\"" on most layouts.
  if ((e.metaKey || e.ctrlKey) && e.key === "'" && !e.shiftKey) {
    e.preventDefault();
    // The panel is visible by definition while handling its own key
    void toggleQuickAsk(true);
    return;
  }
  if ((e.metaKey || e.ctrlKey) && e.shiftKey && (e.key === "'" || e.key === "\"")) {
    e.preventDefault();
    void toggleSideChat();
    return;
  }
  // Change Model (⌥M, MOE-0033): opens the switcher on the chat page — the same landing as the
  // card's "Change Model" row, whose Kbd comes from the platform table. Physical `KeyM`: ⌥M types "µ".
  if (e.altKey && !e.metaKey && !e.ctrlKey && e.code === "KeyM") {
    if (view.get().mode !== "chat") return; // the row (and the pair) live on the chat page
    e.preventDefault();
    closeActionsCard();
    closeAboutCard();
    modelSwitcher.toggle();
    return;
  }
  // The front-most list owns navigation and Backspace (ADR-0031): while a card is open, the card's
  // own input stops propagation for the keys it consumes; this guard covers focus sitting elsewhere.
  // Backspace with empty input = Back (layered back, ADR-0017):
  // leave it alone when the input is non-empty (normal delete); when empty, back out layer by layer, but the root layer does not close the panel (quit:false).
  const cardOpen = panelLayers.anyOpen();
  if (!cardOpen && e.key === "Backspace" && draftEl().value === "" && !e.isComposing && !e.repeat) {
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
      // `sendChatMessage` owns the Enter dialect: it stops a live stream and always sends when
      // the stream is dead, so a stuck generation can never swallow the question (MOE-0030 r9).
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
  if (aboutCard.isOpen() && !aboutCardEl.contains(target) && !chipEl.contains(target)) {
    closeAboutCard();
  }
  // The Filter Card closes on a click outside it or the control that owns it (ADR-0043)
  if (filterCard.isOpen() && !filterCardEl.contains(target) && !filterControlEl.contains(target)) {
    closeFilterCard();
  }
  // The Argument Card closes on a click outside it or the chip that owns it (ADR-0045)
  if (argumentCard.isOpen() && !argumentCardEl.contains(target) && !argumentChipsEl.contains(target)) {
    closeArgumentCard();
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
  /** The key is a combo the global-shortcut plugin owns; the listener's status is not its story. */
  combo: boolean;
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
  const key = KEY_LABELS[status.key] ?? status.key;
  if (status.status === "needsPermission") {
    bannerTextEl.textContent = `${key} to summon needs Input Monitoring permission (System Settings → Privacy & Security → Input Monitoring); it takes effect automatically once granted, though some macOS versions require restarting Moe once.`;
    bannerActionEl.textContent = "Open Input Monitoring Settings";
    bannerActionEl.dataset.action = "input-monitoring";
    showBanner();
  } else if (status.status === "unsupported" && status.combo) {
    // A combo is registered by the global-shortcut plugin, so `unsupported` means only that the
    // registration failed (MOE-0064) — never the double-tap interception story below.
    bannerTextEl.textContent = `${key} could not be registered as a global shortcut: another app owns the chord, or this session does not support global shortcuts. Pick another combo in config.toml, or bind \`moe --toggle\` in your WM.`;
    bannerActionEl.textContent = "Open Config File";
    bannerActionEl.dataset.action = "config";
    showBanner();
  } else if (status.status === "unsupported") {
    // Sessions that cannot globally intercept the keyboard (Linux/Wayland etc., IIE4AD-350): offer the alternative paths
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
const chatFrames = streamCoalescer<
  string,
  { text: string; pending: boolean; reasoning: string; toolCalls?: ToolCall[] }
>((frames) => {
  const frame = frames.get(chatConversationId ?? "");
  if (!frame || view.get().mode !== "chat") return;
  const parts = ensureChatParts();
  parts.view.updateStreaming(frame.text, frame.pending, frame.reasoning, frame.toolCalls);
  parts.view.scrollToEnd();
  setChatGenerating(frame.pending);
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
    const meta = payload.item.payload as {
      conversationId?: string;
      reasoning?: string;
      toolCalls?: ToolCall[];
    } | null;
    const conversationId = meta?.conversationId;
    if (!conversationId || conversationId !== chatConversationId) return;
    chatFrames.push(conversationId, {
      text: payload.item.detail ?? "",
      pending: payload.item.pending === true,
      reasoning: meta.reasoning ?? "",
      toolCalls: meta.toolCalls,
    });
    return;
  }
  if (v.mode !== "items" || v.sourceCommandId !== payload.commandId) return;
  if (!v.items.some((i) => i.id === payload.item.id)) return;
  itemFrames.push(payload.item.id, { commandId: payload.commandId, item: payload.item });
});
void listen("summon-authorized", () => hideBanner());
// The captured selection, as the backend reports it (MOE-0057): the show path emits the fresh shape
// on every summon, and the clipboard-fallback worker (MOE-0013) emits again when a late capture
// settles — the chip follows without the list re-rendering.
void listen<SelectionState>("selection-changed", (event) => {
  selectionState = event.payload;
  renderSelectionState();
  renderSelectionTarget();
  // An open peek holds the previous state's text; refetching replaces it in place.
  if (targetPeekOpen) void ensureSelectionDetail();
});
// Global ⌘' (ADR-0036 amendment): toggle the Quick Ask page. The payload reports whether the panel
// was already visible when the hotkey fired — visible on the page means dismiss it.
void listen<{ visible?: boolean }>("quick-ask-toggle", (event) => {
  void toggleQuickAsk(event.payload?.visible === true);
});
// The palette's entrance (styles.css `.moe-panel-entrance`): the panel is hidden and re-shown — never
// reloaded — so the fade is armed while hidden and played once the window is up. Armed here at boot
// too, because the first summon must not flash the card before fading it in.
rootEl.classList.add("moe-panel-entrance", "moe-panel-dormant");
void listen<{ visible?: boolean }>("panel-visibility", (event) => {
  rootEl.classList.toggle("moe-panel-dormant", event.payload?.visible !== true);
});

// Keep the previous input and results across summons (the user may add input after hiding);
// only refresh the permission guidance (the user may have just granted it in System Settings); unfinished attachment input is not kept across summons.
window.addEventListener("focus", () => {
  cancelAttach();
  void refreshBanner();
});

await initActionBar();
// One source for the row height (ADR-0026): the bottom bar, chip and pill follow ROW_HEIGHT.
document.documentElement.style.setProperty("--moe-row-h", `${ROW_HEIGHT}px`);
// The detail shape's metadata column follows layout.ts too, not a class string (ADR-0042).
document.documentElement.style.setProperty(
  "--moe-metadata-col-w",
  `${METADATA_COLUMN_WIDTH}px`,
);
renderInputIcon();
await refresh("");
await refreshBanner();
await refreshSelectionState();
render();
// Size the panel to exactly VISIBLE_ROWS rows (ADR-0026): measured from the real input bar,
// so the bottom bar lines up with the last row band regardless of font metrics.
void invoke("resize_panel", { width: PANEL_WIDTH, height: measuredPanelHeight() }).catch(() => {});
