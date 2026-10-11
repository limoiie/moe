// The model switcher (MOE-0029/0030): a floating card listing the ranked models (active provider
// first) of the providers that can actually run — each listed provider carries a resolvable key.
// Rows carry the provider's brand mark and the model's abilities as rounded tiles; the focused
// row's details live in the companion card floating to the left (modality tiles, then a label /
// value fact list, then the provider/model id footer in mono). The detail card is absolutely
// positioned against the list, so its content can never resize the list (no hover flicker loop).
// Enter or click switches the active pair (`[ai] provider` / `[ai] model` in place); both chat
// surfaces open it from the actions card's "Set AI Model" row.

import { invoke } from "@tauri-apps/api/core";
import { createCard, type Card, type CardRow, type CardSection } from "./card";
import { iconEl } from "./icons";
import { providerMark } from "./logos";

export interface ActivePair {
  providerId: string;
  providerName: string;
  modelId: string;
  modelName: string;
}

export interface ModelDetail {
  context: string | null;
  maxOutput: string | null;
  modalitiesIn: string[];
  modalitiesOut: string[];
  capabilities: string[];
  status: string | null;
  priceInput: string | null;
  priceOutput: string | null;
  priceCacheRead: string | null;
  priceCacheWrite: string | null;
  released: string | null;
  knowledge: string | null;
  doc: string | null;
}

export interface ModelOption {
  providerId: string;
  providerName: string;
  modelId: string;
  modelName: string;
  current: boolean;
  detail: ModelDetail;
}

async function fetchOptions(query: string): Promise<ModelOption[]> {
  try {
    return await invoke<ModelOption[]>("ai_model_options", { query });
  } catch {
    return [];
  }
}

async function setPair(providerId: string, modelId: string): Promise<ActivePair | null> {
  return invoke<ActivePair | null>("ai_set_pair", { provider: providerId, model: modelId });
}

export interface ModelSwitcher {
  open(): void;
  close(): void;
  toggle(): void;
  isOpen(): boolean;
}

/** The companion detail card's width (px) + the gap to the list card (the fit check). */
const DETAIL_WIDTH = 288;
const DETAIL_GAP = 8;

/** Rounded-tile hues (the modality/ability language): blue text · green image · amber files. */
type TileTone = "blue" | "green" | "amber" | "neutral";
const TILE_TONES: Record<TileTone, string> = {
  blue: "bg-accent/12 text-accent",
  green: "bg-success/12 text-success",
  amber: "bg-warning/15 text-warning",
  neutral: "bg-surface-hover text-fg-muted",
};

/** Modality → tile (the `in → out` row, MOE-0030 revision 8). */
const MODALITY_TILES: Record<string, { icon: string; tone: TileTone }> = {
  text: { icon: "type", tone: "blue" },
  image: { icon: "image", tone: "green" },
  pdf: { icon: "file-text", tone: "amber" },
  audio: { icon: "megaphone", tone: "neutral" },
  video: { icon: "monitor", tone: "neutral" },
};

/** Ability → tile (the model rows' trailing tiles, strongest first). */
const ABILITY_TILES: Record<string, { icon: string; tone: TileTone }> = {
  "tool calls": { icon: "wrench", tone: "blue" },
  vision: { icon: "image", tone: "green" },
  reasoning: { icon: "lightbulb", tone: "amber" },
  attachments: { icon: "paperclip", tone: "neutral" },
  "structured output": { icon: "list-checks", tone: "neutral" },
  "open weights": { icon: "globe", tone: "neutral" },
};
/** How many ability tiles a list row shows (the rest still live in the detail card's horizon). */
const ROW_ABILITY_LIMIT = 4;
const ABILITY_ORDER = [
  "tool calls",
  "vision",
  "reasoning",
  "attachments",
  "structured output",
  "open weights",
];

/** One rounded tile: an icon on a tinted squircle, the ability/modality language. */
function tile(icon: string, tone: TileTone, size: number, title: string): HTMLElement {
  const el = document.createElement("span");
  el.className = `inline-flex shrink-0 items-center justify-center rounded-[6px] ${TILE_TONES[tone]}`;
  el.style.width = `${size}px`;
  el.style.height = `${size}px`;
  el.append(iconEl(icon, { size: Math.round(size * 0.58) }));
  el.title = title;
  el.setAttribute("aria-label", title);
  return el;
}

/** The model rows' trailing ability tiles (at most ROW_ABILITY_LIMIT, strongest first). */
function abilityTiles(capabilities: string[]): HTMLElement | null {
  const ordered = ABILITY_ORDER.filter((ability) => capabilities.includes(ability));
  if (ordered.length === 0) return null;
  const wrap = document.createElement("span");
  wrap.className = "flex shrink-0 items-center gap-1";
  for (const ability of ordered.slice(0, ROW_ABILITY_LIMIT)) {
    const tileDef = ABILITY_TILES[ability];
    wrap.append(tile(tileDef.icon, tileDef.tone, 18, ability));
  }
  return wrap;
}

/** The `in → out` modality tile row (the detail card's hero, MOE-0030 revision 8). */
function modalityRow(detail: ModelDetail): HTMLElement | null {
  if (detail.modalitiesIn.length === 0 && detail.modalitiesOut.length === 0) return null;
  const row = document.createElement("div");
  row.className = "flex flex-wrap items-center gap-1.5";
  const group = (modalities: string[]) => {
    const wrap = document.createElement("span");
    wrap.className = "flex flex-wrap items-center gap-1.5";
    for (const modality of modalities) {
      const tileDef = MODALITY_TILES[modality] ?? { icon: "list", tone: "neutral" as TileTone };
      wrap.append(tile(tileDef.icon, tileDef.tone, 26, modality));
    }
    return wrap;
  };
  row.append(group(detail.modalitiesIn));
  if (detail.modalitiesIn.length > 0 && detail.modalitiesOut.length > 0) {
    row.append(iconEl("arrow-right", { size: 14, className: "shrink-0 text-fg-subtle" }));
  }
  row.append(group(detail.modalitiesOut));
  return row;
}

export function createModelSwitcher(options: {
  /** The surface root the cards anchor to (position: relative). */
  host: HTMLElement;
  /** Placement classes for the card pair inside the host (anchor only — no width). */
  anchorClass: string;
  /** The list card's width class (the surfaces differ; the detail card stays DETAIL_WIDTH wide). */
  cardWidth?: string;
  /** The pair changed through the card: the surface refreshes its chrome and toasts. */
  onApplied?: (pair: ActivePair) => void;
  /** The card closed: return focus to the surface's input. */
  onClose?: () => void;
  /** An apply failed (config write): the surface says so — a silent no-op looks like a lost click. */
  onError?: (message: string) => void;
}): ModelSwitcher {
  let card: Card | null = null;
  let wrapperEl: HTMLDivElement | null = null;
  let cardEl: HTMLDivElement | null = null;
  let detailEl: HTMLDivElement | null = null;
  /** Which option each rendered row stands for (the detail card follows the focused row). */
  const rowOptions = new Map<CardRow, ModelOption>();

  /** The ranked options as provider-major sections: a brand mark per group, ability tiles per row. */
  function sectionsOf(optionsList: ModelOption[]): CardSection[] {
    rowOptions.clear();
    const sections: CardSection[] = [];
    let lastProviderId: string | null = null;
    for (const option of optionsList) {
      let section = sections[sections.length - 1];
      if (!section || lastProviderId !== option.providerId) {
        section = {
          title: option.providerName,
          leading: providerMark(option.providerId, option.providerName, 16),
          rows: [],
        };
        sections.push(section);
        lastProviderId = option.providerId;
      }
      const row: CardRow = {
        title: option.modelName,
        trailing: abilityTiles(option.detail.capabilities) ?? undefined,
        checked: option.current,
        run: () => void apply(option),
      };
      rowOptions.set(row, option);
      section.rows.push(row);
    }
    return sections;
  }

  async function apply(option: ModelOption) {
    try {
      const next = await setPair(option.providerId, option.modelId);
      if (next) options.onApplied?.(next);
    } catch (err) {
      options.onError?.(String(err));
    }
  }

  /** The lifecycle chip: short, quiet, warning-toned only when the model is on its way out. */
  function statusChip(text: string): HTMLElement {
    const el = document.createElement("span");
    const retired = text === "deprecated" || text === "retired";
    el.className = `inline-flex shrink-0 items-center rounded-full border px-2 py-0.5 text-[11px] leading-tight ${
      retired ? "border-warning-line bg-warning-soft text-warning" : "border-line text-fg-subtle"
    }`;
    el.textContent = text;
    return el;
  }

  /** One fact row: muted label left, value right, hairline-separated (the LobeHub model card). */
  function factRow(label: string, value: string | null): HTMLElement | null {
    if (!value) return null;
    const el = document.createElement("div");
    el.className = "flex items-baseline justify-between gap-3 py-1";
    const key = document.createElement("span");
    key.className = "shrink-0 text-[13px] text-fg-muted";
    key.textContent = label;
    const val = document.createElement("span");
    val.className = "min-w-0 text-right text-[13px] font-medium text-fg tabular-nums";
    val.textContent = value;
    el.append(key, val);
    return el;
  }

  /** The companion card's content: brand mark + name header, the modality tiles, the fact rows,
   *  and the mono `provider/model-id` footer with the docs link. */
  function detailContent(option: ModelOption): HTMLElement[] {
    const detail = option.detail;

    const head = document.createElement("div");
    head.className = "flex shrink-0 items-center gap-2.5 px-3.5 pt-3 pb-2.5";
    head.append(providerMark(option.providerId, option.providerName, 28));
    const names = document.createElement("div");
    names.className = "flex min-w-0 flex-1 items-center gap-2";
    const name = document.createElement("span");
    name.className = "min-w-0 truncate text-[15px] font-medium text-fg";
    name.textContent = option.modelName;
    names.append(name);
    if (detail.status) names.append(statusChip(detail.status));
    head.append(names);

    const body = document.createElement("div");
    body.className =
      "moe-scroll-fade-bottom min-h-0 flex-1 space-y-2 overflow-y-auto border-t border-line px-3.5 py-2.5";

    const modalities = modalityRow(detail);
    if (modalities) body.append(modalities);

    const rows = [
      factRow("Context", detail.context),
      factRow("Max output", detail.maxOutput),
      factRow("Input", detail.priceInput),
      factRow("Output", detail.priceOutput),
      factRow("Cache read", detail.priceCacheRead),
      factRow("Cache write", detail.priceCacheWrite),
      factRow("Released", detail.released),
      factRow("Knowledge", detail.knowledge),
    ].filter((row): row is HTMLElement => row !== null);
    if (rows.length > 0) {
      const list = document.createElement("div");
      list.className = "divide-y divide-line";
      list.append(...rows);
      body.append(list);
    } else if (body.childElementCount === 0) {
      const empty = document.createElement("div");
      empty.className = "text-xs text-fg-subtle";
      empty.textContent = "The catalog carries no metadata for this model.";
      body.append(empty);
    }

    const footer = document.createElement("div");
    footer.className = "flex shrink-0 items-center justify-between gap-2 border-t border-line px-3.5 py-2";
    const id = document.createElement("span");
    id.className = "min-w-0 truncate font-mono text-[11px] text-fg-subtle";
    id.textContent = `${option.providerId}/${option.modelId}`;
    footer.append(id);
    if (detail.doc) {
      const link = document.createElement("a");
      link.href = detail.doc;
      link.target = "_blank";
      link.rel = "noreferrer";
      link.className = "inline-flex shrink-0 items-center gap-1 text-[11px] text-accent hover:underline";
      link.append(
        iconEl("external-link", { size: 11, className: "shrink-0" }),
        document.createTextNode("Docs"),
      );
      footer.append(link);
    }
    return [head, body, footer];
  }

  /** The companion card follows the focused row — hidden when nothing is focused or the room
      is too tight (a narrow Side View shows the list alone rather than an overlapping pair). */
  function showDetail(option: ModelOption | undefined) {
    if (!detailEl || !cardEl) return;
    const fits = cardEl.offsetWidth + DETAIL_WIDTH + DETAIL_GAP <= options.host.clientWidth;
    if (!option || !fits) {
      detailEl.classList.add("hidden");
      detailEl.classList.remove("flex");
      return;
    }
    detailEl.replaceChildren(...detailContent(option));
    detailEl.classList.remove("hidden");
    detailEl.classList.add("flex");
  }

  function ensureCard(): Card {
    if (card) return card;
    wrapperEl = document.createElement("div");
    // The wrapper holds the list card in flow; the detail card is absolutely positioned against
    // it, so the list's height (and therefore every hover target) never moves with the detail.
    wrapperEl.className = `absolute z-30 hidden ${options.anchorClass}`;
    detailEl = document.createElement("div");
    // Fits its content (height auto, capped at the list's height): the card hugs what it shows,
    // keeps the shared bottom edge, and — bottom-anchored, absolutely positioned — can never
    // resize the list, so no hover flicker is possible either way (MOE-0030 revision 9).
    detailEl.className =
      "moe-actions-card absolute bottom-0 right-full mr-2 hidden max-h-full w-72 flex-col overflow-hidden rounded-2xl bg-surface-overlay";
    cardEl = document.createElement("div");
    cardEl.className = `moe-actions-card hidden flex-col overflow-hidden rounded-2xl bg-surface-overlay ${options.cardWidth ?? "w-80"}`;
    const list = document.createElement("ul");
    // A fixed height: the card reads as a panel, not a strip that grows and shrinks with the hit
    // count (and the detail card, anchored inset-y-0, gets the same stable height).
    list.className = "moe-scroll-fade h-80 overflow-y-auto p-1";
    const input = document.createElement("input");
    input.className =
      "mt-auto w-full border-t border-line bg-transparent px-3 py-2.5 text-sm text-fg caret-accent placeholder-fg-subtle outline-none";
    input.placeholder = "Search models…";
    input.spellcheck = false;
    // The filter sits at the bottom like every other menu card (ADR-0028 amendment); the rows
    // come from the backend per query, so search reaches the whole catalog, not just the head.
    cardEl.append(list, input);
    wrapperEl.append(detailEl, cardEl);
    options.host.append(wrapperEl);
    card = createCard({
      cardEl,
      listEl: list,
      inputEl: input,
      emptyText: "No models — connect a provider in Settings",
      onQuery: (query) => fetchOptions(query).then(sectionsOf),
      focusOnHover: true,
      onFocusChange: (row) => showDetail(row ? rowOptions.get(row) : undefined),
      onClose: () => {
        wrapperEl?.classList.add("hidden");
        wrapperEl?.classList.remove("flex");
        cardEl?.classList.remove("flex");
        detailEl?.classList.add("hidden");
        detailEl?.classList.remove("flex");
        options.onClose?.();
      },
    });
    return card;
  }

  async function open() {
    const modelCard = ensureCard();
    const optionsList = await fetchOptions("");
    wrapperEl?.classList.remove("hidden");
    wrapperEl?.classList.add("flex");
    cardEl?.classList.add("flex");
    // The render inside open() focuses the first row and raises the detail card (the wrapper is
    // visible by then, so the fit check measures real widths).
    modelCard.open(sectionsOf(optionsList));
  }

  function close() {
    card?.close();
  }

  function isOpen() {
    return card?.isOpen() === true;
  }

  function toggle() {
    if (isOpen()) close();
    else void open();
  }

  // Click outside the pair closes it (the surfaces' own cards do the same).
  document.addEventListener("mousedown", (e) => {
    const target = e.target as Node;
    if (isOpen() && cardEl && !cardEl.contains(target) && !detailEl?.contains(target)) {
      close();
    }
  });

  return { open, close, toggle, isOpen };
}