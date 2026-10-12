// The tag filter's pure rules (ADR-0043), lifted out of `main.ts` so they can be tested.
//
// `main.ts` queries the DOM at module load, so a test cannot import it at all — which is why the
// three rules that decide what the user sees live here instead of beside their caller. They are the
// ones that drift silently: which items a value shows, which values a list offers, and what happens
// to a remembered value that the new result set no longer carries.

import type { Filter, Item } from "./types";

/** Narrow a result set to the active value. Exact match: the values come from the items' own tags. */
export function applyTagFilter(items: Item[], value: string | undefined): Item[] {
  if (!value) return items;
  return items.filter((item) => item.tags?.includes(value) === true);
}

/**
 * The values a list offers, in first-seen order, from the items that carry tags. Derived *before* the
 * filter narrows anything, so the picker cannot lose values as the list shrinks.
 */
export function deriveFilterValues(items: Item[]): string[] {
  const seen: string[] = [];
  for (const item of items) {
    for (const tag of item.tags ?? []) {
      if (!seen.includes(tag)) seen.push(tag);
    }
  }
  return seen;
}

/**
 * The values a list offers: the dimension's declared values when it names any, otherwise the ones
 * derived from the items (ADR-0043). The Rust side omits `values` on the wire when it is empty, so
 * absent must read as empty here too — a required field over an omittable one is a type that lies.
 */
export function filterValuesFor(filter: Filter | undefined, items: Item[]): string[] {
  if (!filter) return [];
  const declared = filter.values ?? [];
  return declared.length > 0 ? declared : deriveFilterValues(items);
}

/**
 * The value a *new* result set can still be filtered by. A remembered value the set no longer carries
 * falls back to All — the derived list is the authority, so a page can never stay narrowed by a value
 * that is nowhere to be seen.
 */
export function settleFilterValue(
  previous: string | undefined,
  values: string[],
): string | undefined {
  return previous !== undefined && values.includes(previous) ? previous : undefined;
}