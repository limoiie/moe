// The metadata block's interactive rule (ADR-0042, landed by MOE-0048), lifted out of `main.ts` so the
// decision that can go wrong is testable: what a tag click does. The DOM wiring stays beside the
// renderer.

import type { Action, MetadataTag } from "./types";

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
