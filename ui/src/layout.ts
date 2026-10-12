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

// ---- The row grid (ADR-0026): the panel is sized to show exactly VISIBLE_ROWS rows, and the
// bottom bar (avatar chip + action pill) overlays the last row's band exactly — same height as
// one row, the chip's avatar centered on the item icons' x. ----

/** One command row's height (px): `py-2.5` + `text-sm` line height = 10 + 20 + 10. */
export const ROW_HEIGHT = 40;

/** Rows visible by default: the panel shows exactly this many (Raycast-style ten-slot list). */
export const VISIBLE_ROWS = 10;

/** One section header's height (px): the source-group label above the rows (ADR-0020). */
export const SECTION_HEADER_HEIGHT = 30;

/**
 * Headers accounted for in the grid: every list has at least one (the first group's label —
 * e.g. Suggestions on the root page), so it is reserved on top of the ten rows. Pages showing
 * more than one header at once fit fewer rows, which is inherent to grouping.
 */
export const HEADER_ROWS = 1;

/** Padding below the list (px): the bottom bar band is the list's own last row + this padding. */
export const BOTTOM_PADDING = 8;

/** `#root` is inset by `m-3` (12px) on every side for the window shadow (ADR-0016). */
export const WINDOW_MARGIN = 12;

/** `#root`'s edge is the glass rim (an inset ring in the shadow token), not a CSS border, so the
 *  grid budgets no border pixels (ADR-0035 amendment: Siri-window grade glass). */
export const CARD_BORDER = 0;

/**
 * Panel height for a measured input-bar height: exactly VISIBLE_ROWS rows fit under the input
 * bar (plus one section header), plus the bottom padding and the window margins. Measured at
 * runtime so the grid is exact regardless of font metrics; the constant below is only the
 * first-frame fallback and must match `tauri.conf.json` (ADR-0026).
 */
export function panelHeightFor(inputBarHeight: number): number {
  return (
    Math.round(inputBarHeight) +
    ROW_HEIGHT * VISIBLE_ROWS +
    SECTION_HEADER_HEIGHT * HEADER_ROWS +
    BOTTOM_PADDING +
    CARD_BORDER * 2 +
    WINDOW_MARGIN * 2
  );
}

/** First-frame fallback height (window logical px): 48 + 10×40 + 1×30 + 8 + 24 margins. */
export const PANEL_HEIGHT = 510;

// Layout classes for the three shapes: applied only in render; nowhere else may compose widths.

/** Single-column list (list): takes the full width. */
export const LIST_FULL_CLASS =
  "min-h-0 flex-1 overflow-y-auto px-2";

/** Left list of the split page: **narrow column** (enough for a conversation title + time); detail takes the remaining width. */
export const LIST_NARROW_CLASS =
  "min-h-0 w-[280px] shrink-0 overflow-y-auto border-r border-line px-2";

/** Right detail of the split page (also the container for full-detail): takes the remaining width. */
export const DETAIL_PANE_CLASS =
  "md min-h-0 flex-1 overflow-y-auto px-4 pb-12 pt-3 text-sm text-fg";

/**
 * The Quick Ask page's conversation pane (ADR-0036): a flex column — the message log inside is the
 * scroll container (`createConversationView`'s `scrollToEnd` scrolls the log, like the Side View's
 * `#messages`), so this class carries no overflow and no prose styles (each answer bubble carries
 * its own `.md`).
 */
export const CONVERSATION_PANE_CLASS = "flex min-h-0 flex-1 flex-col px-4 pb-12 pt-3";

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
