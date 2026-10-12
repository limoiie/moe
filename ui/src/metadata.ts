// The metadata block: its click rule (ADR-0042, landed by MOE-0048) and its renderer. Both live here
// rather than in `main.ts`, which queries the DOM at module load and therefore cannot be imported by a
// test at all — so before this, every claim about how the block *renders* was unverifiable and had to
// be confirmed by hand.
//
// The renderer takes its side effects as arguments (`MetadataDeps`) instead of reaching into the
// panel's state: what a tag click does, and where a link goes, are the two things a test must be able
// to observe without a Tauri backend.

import { iconEl, inkClass } from "./icons";
import type { Action, Item, Metadata, MetadataTag } from "./types";

/** What clicking a metadata tag does. `filter` outranks `action` — ADR-0042 states the ordering. */
export type TagIntent =
  | { kind: "filter"; value: string }
  | { kind: "action"; actionId: string }
  | { kind: "none" };

/**
 * Resolve a tag's click intent.
 *
 * A tag whose *text* is a value of the page's declared filter dimension sets the Filter Control
 * instead of running an action: see a tag, click it, the list narrows. This is the one place ADR-0042
 * knowingly goes past Raycast, whose `TagList.Item` has `onAction` but no dimension to match against.
 * Otherwise an `actionId` that names one of the item's own actions dispatches through
 * `run_item_action`.
 *
 * Everything else is display-only, deliberately: an unresolvable id must not error, or one bad tag
 * would take the rest of the row with it.
 */
export function tagIntent(
  tag: MetadataTag,
  actions: Action[],
  dimensionValues: string[],
): TagIntent {
  if (tag.text !== undefined && dimensionValues.includes(tag.text)) {
    return { kind: "filter", value: tag.text };
  }
  if (tag.actionId !== undefined && actions.some((action) => action.id === tag.actionId)) {
    return { kind: "action", actionId: tag.actionId };
  }
  return { kind: "none" };
}

/** The panel's side of the block: its own state and dispatch, passed in rather than imported. */
export interface MetadataDeps {
  /** The page's declared dimension values — empty when it declares none, which is what makes the same
   *  tag interactive on one page and display-only on another. */
  dimensionValues: string[];
  /** A tag that matches a dimension: set the Filter Control to that value. */
  onFilter: (value: string) => void;
  /** A tag that names one of the item's actions: run it. */
  onAction: (item: Item, action: Action) => void;
  /** A `link` row: hand the target to the platform (`open::open_url` refuses non-http(s)). */
  onOpenLink: (target: string) => void;
}

/** One metadata row's right-hand value: an optional icon and/or text, in a palette color when one is
 *  named (ADR-0041). The ink class sits on the inner span so it wins by specificity rather than by
 *  rule order against the wrapper's default step. */
function valueEl(icon?: string, text?: string, color?: string): HTMLElement | undefined {
  if (!icon && !text) return undefined;
  const wrap = document.createElement("span");
  wrap.className = "moe-metadata-value";
  const inner = document.createElement("span");
  inner.className = "moe-metadata-inline";
  const ink = inkClass(color);
  if (ink) inner.classList.add(ink);
  if (icon) inner.append(iconEl(icon, { size: 13 }));
  if (text) {
    const label = document.createElement("span");
    label.textContent = text;
    inner.append(label);
  }
  wrap.append(inner);
  return wrap;
}

/** One metadata row (ADR-0042): the Extension's title on the left, its value on the right. */
function rowEl(title: string, value?: HTMLElement): HTMLElement {
  const row = document.createElement("div");
  row.className = "moe-metadata-row";
  const label = document.createElement("span");
  label.className = "moe-metadata-title";
  label.textContent = title;
  row.append(label);
  if (value) row.append(value);
  return row;
}

/**
 * One tag of the pane's tag list (ADR-0042). An interactive tag is a real button so the cursor and the
 * semantics are honest, but `tabindex="-1"` keeps it out of the tab order: ADR-0042 names tags
 * pointer-only, and reaching them from the keyboard is MOE-0045's decision — this is the seam it
 * changes. A display-only tag is a plain span, so it cannot look clickable by accident.
 */
function tagEl(tag: MetadataTag, intent: TagIntent, item: Item, deps: MetadataDeps): HTMLElement {
  const el = document.createElement(intent.kind === "none" ? "span" : "button");
  el.className = "moe-metadata-tag";
  if (intent.kind === "filter") {
    (el as HTMLButtonElement).type = "button";
    el.tabIndex = -1;
    el.classList.add("moe-metadata-tag-live");
    // The tag→filter loop: the list narrows and the Filter Control's label follows (ADR-0043).
    el.addEventListener("click", () => deps.onFilter(intent.value));
  } else if (intent.kind === "action") {
    (el as HTMLButtonElement).type = "button";
    el.tabIndex = -1;
    el.classList.add("moe-metadata-tag-live");
    const action = item.actions.find((a) => a.id === intent.actionId);
    if (action) el.addEventListener("click", () => deps.onAction(item, action));
  }
  const ink = inkClass(tag.color);
  if (ink) el.classList.add(ink);
  if (tag.icon) el.append(iconEl(tag.icon, { size: 12 }));
  if (tag.text) {
    const text = document.createElement("span");
    text.textContent = tag.text;
    el.append(text);
  }
  return el;
}

/** A titled row of tags. */
function tagListEl(title: string, tags: MetadataTag[], item: Item, deps: MetadataDeps): HTMLElement {
  const wrap = document.createElement("span");
  wrap.className = "moe-metadata-value moe-metadata-tags";
  for (const tag of tags) {
    wrap.append(tagEl(tag, tagIntent(tag, item.actions, deps.dimensionValues), item, deps));
  }
  return rowEl(title, wrap);
}

/**
 * The detail pane's metadata block (ADR-0042): Raycast's `List.Item.Detail.Metadata`. One renderer for
 * both placements — the split pane's bottom and the `detail` shape's right-hand column differ only in
 * where the caller puts the result.
 */
export function metadataEl(
  metadata: Metadata[],
  item: Item,
  deps: MetadataDeps,
): HTMLElement {
  const block = document.createElement("div");
  block.className = "moe-metadata not-prose";
  for (const entry of metadata) {
    if (entry.kind === "separator") {
      const separator = document.createElement("div");
      separator.className = "moe-metadata-separator";
      block.append(separator);
    } else if (entry.kind === "label") {
      block.append(rowEl(entry.title, valueEl(entry.icon, entry.text, entry.color)));
    } else if (entry.kind === "link") {
      // A button rather than an `<a>`: the panel is a webview and a raw href would navigate it.
      const link = document.createElement("button");
      link.type = "button";
      link.className = "moe-metadata-link";
      link.textContent = entry.text;
      link.title = entry.target;
      link.addEventListener("click", () => deps.onOpenLink(entry.target));
      block.append(rowEl(entry.title, link));
    } else {
      block.append(tagListEl(entry.title, entry.items, item, deps));
    }
  }
  return block;
}
