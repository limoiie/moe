// The result list's renderers: the `<li>`s it is made of — a row, a section header, the empty view —
// plus the accessory elements a row can carry (ADR-0018/0020/0030/0041/0043). They live here, with
// their side effects passed in, rather than in `main.ts`: that file queries the DOM at module load, so
// no test can import it, and before this the list's drawing could only be checked by hand. ADR-0007's
// amendment states the rule (a renderer worth testing lives in its own module and takes its side
// effects as arguments); the metadata block was the first to follow it, this is the second.

import { accessoryLabel } from "./accessories";
import { iconEl, inkClass } from "./icons";
import { kbdEl } from "./kbd";
import { SECTION_HEADER_HEIGHT } from "./layout";
import type { Accessory, EmptyView } from "./types";

/** Section header (ADR-0020): source is the group; headers are not focusable, do not participate in
 *  navigation, and do not respond to hover. Fixed height so the ten-row grid can reserve it (ADR-0026). */
export function sectionHeaderEl(title: string): HTMLLIElement {
  const li = document.createElement("li");
  li.className =
    "moe-section-header select-none px-3 text-[11px] font-medium uppercase tracking-wider text-fg-subtle";
  li.style.height = `${SECTION_HEADER_HEIGHT}px`;
  li.textContent = title;
  return li;
}

/**
 * The Empty View (ADR-0043): the platform's placeholder for a list with no rows. Non-focusable by
 * construction — a `<li>` with no click handler, no `data-focused` and no `.moe-row` class, so ↑↓ never
 * lands on it, the row hover rule never matches it, and ⏎ has nothing to Apply. It replaced the
 * Extension-authored notice row, which *was* a real Item.
 *
 * Whether to show it at all is the caller's call, not this function's: the platform suppresses it while
 * a query-free load is in flight, so a first paint cannot flash "no results".
 */
export function emptyViewEl(empty: EmptyView): HTMLLIElement {
  const li = document.createElement("li");
  li.className = "select-none px-2 py-6 text-center";
  li.setAttribute("role", "presentation");
  if (empty.icon) {
    const icon = document.createElement("div");
    icon.className = "mb-2 flex justify-center text-fg-faint";
    icon.append(iconEl(empty.icon, { size: 20 }));
    li.append(icon);
  }
  const title = document.createElement("div");
  title.className = "text-sm text-fg-muted";
  title.textContent = empty.title;
  li.append(title);
  if (empty.description) {
    const desc = document.createElement("div");
    desc.className = "mx-auto mt-1 max-w-[44ch] text-xs leading-5 text-fg-subtle";
    desc.textContent = empty.description;
    li.append(desc);
  }
  return li;
}
export interface Row {
  title: string;
  subtitle?: string;
  key?: string;
  /** Semantic name; undefined = use the source command's icon, or fall back if there is none. */
  icon?: string;
  /** Fallback shape (no icon specified) is displayed dimmed. */
  iconMuted?: boolean;
  /** Trailing type badge on command rows (ADR-0030): "Command" unless the extension declares otherwise. */
  badge?: string;
  /** Trailing accessories on result rows (ADR-0041): tags, text, relative dates, tinted icons. */
  accessories?: Accessory[];
  /** Root page rows (ADR-0031): the declared shortcut reveals only while the row is focused or hovered. */
  revealKeys?: boolean;
}

export interface RowDeps {
  /** One clock reading per render, so every row of a list agrees on "now" — and so a test can pin it. */
  nowUnix: number;
  /** A row was pressed: focus it. The index is the flat one the caller passed in, which for a grouped
   *  list is the item's index and never a header's. */
  onFocus: (index: number) => void;
}

/** One accessory element (ADR-0041): a tinted pill, plain text, a relative date, or a tinted glyph.
 *  `kind` picks the shape; an unknown color name leaves the element in the secondary step. */
function accessoryEl(a: Accessory, nowUnix: number): HTMLElement | SVGElement {
  if (a.kind === "icon") {
    const wrap = document.createElement("span");
    wrap.className = "moe-accessory";
    const ink = inkClass(a.color);
    if (ink) wrap.classList.add(ink);
    wrap.append(iconEl(a.value, { size: 14 }));
    if (a.tooltip) wrap.title = a.tooltip;
    return wrap;
  }
  const el = document.createElement("span");
  el.className = `moe-accessory ${a.kind === "tag" ? "moe-accessory-tag" : `moe-accessory-${a.kind}`}`;
  const ink = inkClass(a.color);
  if (ink) el.classList.add(ink);
  el.textContent = accessoryLabel(a, nowUnix);
  if (a.tooltip) el.title = a.tooltip;
  return el;
}

/**
 * A single row: hover is an interactive hint (never steals focus) and only applies after real pointer
 * movement — the re-render flicker while keyboard-navigating is gated away in styles.css (.moe-row +
 * data-pointer); only the focused row gets the main highlight.
 *
 * The trailing order is the Extension's declaration order, then the shortcut Kbd, then the type badge;
 * accessories sit directly after the title cell, which is what makes the title truncate before any
 * accessory is dropped (the title cell is `min-w-0 flex-1`, accessories are `flex: none`).
 */
export function rowEl(row: Row, index: number, focused: boolean, deps: RowDeps): HTMLLIElement {
  const li = document.createElement("li");
  li.className =
    "moe-row group flex cursor-default items-center gap-2.5 rounded-2xl px-3 py-2.5 text-sm transition-colors duration-100 " +
    (focused ? "bg-surface-selected text-fg" : "text-fg");
  if (focused) li.dataset.focused = "true";
  li.append(
    iconEl(row.icon, {
      className: row.iconMuted
        ? "text-fg-faint"
        : focused
          ? "text-fg"
          : "text-fg-subtle group-hover:text-fg-muted",
    }),
  );
  const left = document.createElement("div");
  left.className = "min-w-0 flex-1 truncate";
  left.textContent = row.title;
  if (row.subtitle) {
    const sub = document.createElement("span");
    sub.className = "ml-2 text-xs text-fg-subtle";
    sub.textContent = row.subtitle;
    left.append(sub);
  }
  li.append(left);
  if (row.accessories) {
    // `rowAccessories` has already capped them and hidden them where they do not belong; a date
    // accessory renders from its timestamp here, not from a backend-formatted string.
    for (const accessory of row.accessories) li.append(accessoryEl(accessory, deps.nowUnix));
  }
  if (row.key) {
    // Key blocks (shadcn Kbd style): the shared kbdEl component, one block per key (ADR-0015/0030)
    const keys = kbdEl(row.key, { firstOnly: true });
    keys.classList.add("shrink-0");
    // Root page (ADR-0031): reveal on focus/hover only (styles.css `.moe-row-keys`)
    if (row.revealKeys) keys.classList.add("moe-row-keys");
    li.append(keys);
  }
  if (row.badge) {
    // Trailing type badge (ADR-0030): what kind of row this is (Command / AI Command / File…);
    // its text size comes from the shared secondary-text token (styles.css, `.moe-kind-badge`)
    const badge = document.createElement("span");
    badge.className = "moe-kind-badge";
    badge.textContent = row.badge;
    li.append(badge);
  }
  li.addEventListener("mousedown", () => deps.onFocus(index));
  return li;
}
