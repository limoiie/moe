// The result row's trailing accessories (ADR-0041), lifted out of `main.ts` so the rules that can
// drift are testable: how many render, and what a date accessory says.

import { relativeTime } from "./dates";
import type { Accessory, Item } from "./types";

/** At most three render: Raycast documents no cap, but Moe's rows are 40px in a fixed ten-row grid. */
export const MAX_ACCESSORIES = 3;

/**
 * The accessories a row shows: the first three in declared order. The title truncates before any is
 * dropped — the row's left cell is `min-w-0 flex-1` and accessories are `flex: none`, so that
 * ordering is the layout's job, not this function's.
 *
 * They render in **every** shape. ADR-0041 first hid them while the detail pane showed, reading
 * Raycast's "it is recommended not to show any accessories on the `List.Item`" as a host rule. It is
 * a recommendation to extension authors, and Moe is both host and author — while Moe's own pane
 * carries a Markdown preview, not the row's age. Hiding therefore cost the history list the very
 * date accessory this ADR exists to deliver (ADR-0041's second amendment).
 */
export function rowAccessories(item: Item): Accessory[] {
  return (item.accessories ?? []).slice(0, MAX_ACCESSORIES);
}

/**
 * The text a `tag` / `text` / `date` accessory renders; an `icon` has none (its `value` is the icon
 * name). Kept here, not in the renderer, so the date rule — the one that used to live in Rust — has
 * a test.
 */
export function accessoryLabel(accessory: Accessory, nowSeconds: number): string {
  if (accessory.kind === "icon") return "";
  if (accessory.kind === "date") return relativeTime(accessory.value, nowSeconds);
  return accessory.value;
}
