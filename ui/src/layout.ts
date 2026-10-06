// The **three shapes** of the results page (ADR-0018): extensions provide only content,
// while shape, column ratio, and window size are all decided here — any Extension/sub-app
// that returns an Item (optionally with detail) automatically gets exactly the same layout
// and interactions as every other page, with no view-layer customization.
//
//   list  : single-column results, no detail pane                    → default height
//   split : left list + right detail (focused item's detail)         → taller (both columns readable)
//   detail: the single result is the content itself (AI answers, notifications) → detail fills the panel, default height
//
// These are Raycast's List / List+Detail / Detail: the shape is decided by the data
// (whether there is detail, whether it fills the screen), not by each command's own layout.

import type { Item } from "./types";

export type PageShape = "list" | "split" | "detail";

/** Panel width (logical pixels): identical across shapes (Raycast-style: constant size). */
export const PANEL_WIDTH = 768;

/** Panel height (logical pixels): identical across shapes. */
export const PANEL_HEIGHT = 540;

// Layout classes for the three shapes: applied only in render; nowhere else may compose widths.

/** Single-column list (list): takes the full width. */
export const LIST_FULL_CLASS =
  "min-h-0 flex-1 overflow-y-auto px-2 pb-12";

/** Left list of the split page: **narrow column** (enough for a conversation title + time); detail takes the remaining width. */
export const LIST_NARROW_CLASS =
  "min-h-0 w-[280px] shrink-0 overflow-y-auto border-r border-zinc-800 px-2 pb-12";

/** Right detail of the split page (also the container for full-detail): takes the remaining width. */
export const DETAIL_PANE_CLASS =
  "md min-h-0 flex-1 overflow-y-auto px-4 pb-12 pt-3 text-sm text-zinc-200";

/**
 * Which shape should render right now.
 * @param item      the focused item (undefined in list shape)
 * @param detailFull whether the result declared full-screen detail (ADR-0013, decided by the Extension)
 * @param itemCount  number of results (full-screen only applies to a single result)
 */
export function pageShapeOf(
  item: Item | undefined,
  detailFull: boolean,
  itemCount: number,
): PageShape {
  if (!item) return "list";
  const hasDetail = item.detail != null || item.pending === true;
  if (!hasDetail) return "list";
  return detailFull && itemCount === 1 ? "detail" : "split";
}
