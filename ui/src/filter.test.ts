// The tag filter's rules (ADR-0043). Small and unglamorous, but each one is a way the list can lie to
// the user: showing rows the value does not match, offering a value nothing carries, staying narrowed
// by a value that has vanished, or moving off an item while its answer is still arriving (MOE-0046).

import { describe, expect, it } from "vitest";
import { applyTagFilter, deriveFilterValues, filterChangeAllowed, filterValuesFor, settleFilterValue } from "./filter";
import type { Filter, Item } from "./types";

/** The smallest Item that satisfies the type; only `tags` matters to these rules. */
function item(id: string, tags?: string[]): Item {
  return { id, title: id, actions: [], payload: null, tags };
}

describe("applyTagFilter", () => {
  it("returns the set untouched when no value is active", () => {
    const items = [item("a", ["Agentic"]), item("b")];
    // Identity, not a copy: re-widening must not rebuild the list on every render.
    expect(applyTagFilter(items, undefined)).toBe(items);
    expect(applyTagFilter(items, "")).toBe(items);
  });

  it("keeps only the items carrying the value", () => {
    const items = [item("a", ["Agentic"]), item("b", ["Standard"]), item("c")];
    expect(applyTagFilter(items, "Agentic").map((i) => i.id)).toEqual(["a"]);
  });

  it("excludes items with no tags at all", () => {
    // The whole point of All: an untagged item is reachable only through the reset, never by a value.
    const items = [item("a", ["Agentic"]), item("b")];
    expect(applyTagFilter(items, "Agentic").map((i) => i.id)).toEqual(["a"]);
  });

  it("matches exactly, not by prefix or case", () => {
    const items = [item("a", ["Agentic"]), item("b", ["agentic"]), item("c", ["Agentic-ish"])];
    expect(applyTagFilter(items, "Agentic").map((i) => i.id)).toEqual(["a"]);
  });

  it("keeps an item that carries the value among several tags", () => {
    const items = [item("a", ["Standard", "Agentic"])];
    expect(applyTagFilter(items, "Agentic").map((i) => i.id)).toEqual(["a"]);
  });
});

describe("deriveFilterValues", () => {
  it("is empty when nothing carries a tag", () => {
    expect(deriveFilterValues([item("a"), item("b")])).toEqual([]);
  });

  it("lists each value once, in first-seen order", () => {
    const items = [item("a", ["Agentic"]), item("b", ["Standard"]), item("c", ["Agentic"])];
    expect(deriveFilterValues(items)).toEqual(["Agentic", "Standard"]);
  });

  it("collects every tag of a multi-tagged item", () => {
    expect(deriveFilterValues([item("a", ["Standard", "Agentic"])])).toEqual([
      "Standard",
      "Agentic",
    ]);
  });

  it("ignores untagged items without breaking the order", () => {
    const items = [item("a"), item("b", ["X"]), item("c"), item("d", ["Y"])];
    expect(deriveFilterValues(items)).toEqual(["X", "Y"]);
  });
});

describe("filterValuesFor", () => {
  const filter = (values?: string[]): Filter => ({ id: "tags", label: "Tags", values });

  it("offers nothing when the list declares no dimension", () => {
    expect(filterValuesFor(undefined, [item("a", ["Agentic"])])).toEqual([]);
  });

  it("prefers declared values over the items", () => {
    expect(filterValuesFor(filter(["Standard", "Agentic"]), [item("a", ["Agentic"])])).toEqual([
      "Standard",
      "Agentic",
    ]);
  });

  it("derives from the items when values are empty", () => {
    expect(filterValuesFor(filter([]), [item("a", ["Agentic"]), item("b", ["Standard"])])).toEqual(
      ["Agentic", "Standard"],
    );
  });

  it("derives when values are absent — the Rust side omits an empty vector", () => {
    // The regression: `filter.values.length` on an omitted field threw at runtime.
    expect(filterValuesFor(filter(undefined), [item("a", ["Agentic"])])).toEqual(["Agentic"]);
  });
});

describe("filterChangeAllowed", () => {
  it("allows a change when nothing is streaming", () => {
    expect(filterChangeAllowed([item("a", ["Agentic"])])).toBe(true);
    expect(filterChangeAllowed([])).toBe(true);
  });

  it("refuses while any item is still being produced", () => {
    // ADR-0038's guard, extended to the filter by MOE-0046: narrowing the list mid-stream would move
    // the page off the item the frames are still arriving for.
    expect(filterChangeAllowed([item("a"), { ...item("b"), pending: true }])).toBe(false);
  });

  it("does not refuse for a settled item", () => {
    expect(filterChangeAllowed([{ ...item("a"), pending: false }])).toBe(true);
  });
});

describe("settleFilterValue", () => {
  it("keeps a value the new set still carries", () => {
    expect(settleFilterValue("Agentic", ["Agentic", "Standard"])).toBe("Agentic");
  });

  it("drops a value the new set no longer carries", () => {
    // Otherwise the page stays narrowed by a value that is nowhere to be seen — and its own picker
    // could not show it either.
    expect(settleFilterValue("Agentic", ["Standard"])).toBeUndefined();
    expect(settleFilterValue("Agentic", [])).toBeUndefined();
  });

  it("leaves an unset value unset", () => {
    expect(settleFilterValue(undefined, ["Agentic"])).toBeUndefined();
  });
});