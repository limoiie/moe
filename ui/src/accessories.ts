// The result row's trailing accessories (ADR-0041), lifted out of `main.ts` so the rules that can
// drift are testable: how many render, when they all hide, and what a date accessory says.

import { relativeTime } from "./dates";
import type { PageShape } from "./layout";
import type { Accessory, Item } from "./types";

/** At most three render: Raycast documents no cap, but Moe's rows are 40px in a fixed ten-row grid. */
export const MAX_ACCESSORIES = 3;

/**
 * The accessories a row shows: none while the detail pane is showing (`split`), where the same
 * information belongs in the pane (Raycast's own recommendation), otherwise the first three in
 * declared order. The title truncates before any is dropped — the row's left cell is `min-w-0 flex-1`
 * and accessories are `flex: none`, so that ordering is the layout's job, not this function's.
 */
export function rowAccessories(item: Item, shape: PageShape): Accessory[] {
  if (shape === "split") return [];
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
