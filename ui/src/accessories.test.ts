import { describe, expect, it } from "vitest";
import { accessoryLabel, MAX_ACCESSORIES, rowAccessories } from "./accessories";
import type { Accessory, Item } from "./types";

/** The smallest Item that satisfies the type; only `accessories` matters here. */
function item(accessories?: Accessory[]): Item {
  return { id: "a", title: "a", actions: [], payload: null, accessories };
}

function tags(n: number): Accessory[] {
  return Array.from({ length: n }, (_, i) => ({ kind: "tag", value: `t${i}` }));
}

describe("rowAccessories", () => {
  it("renders on every shape, including the split page's narrow column", () => {
    // The history list is a split page (its items carry a preview detail); the date accessory has to
    // survive there or the migration this ADR made would be invisible.
    const a = tags(2);
    expect(rowAccessories(item(a))).toEqual(a);
  });

  it("caps at three, dropping the rest in declared order", () => {
    expect(rowAccessories(item(tags(5)))).toHaveLength(MAX_ACCESSORIES);
    expect(rowAccessories(item(tags(5))).map((x) => x.value)).toEqual(["t0", "t1", "t2"]);
  });

  it("is empty when the item carries none", () => {
    expect(rowAccessories(item())).toEqual([]);
  });
});

describe("accessoryLabel", () => {
  it("renders a date relatively", () => {
    expect(accessoryLabel({ kind: "date", value: 100 }, 160)).toBe("1m ago");
  });

  it("renders tag and text verbatim", () => {
    expect(accessoryLabel({ kind: "tag", value: "Agentic" }, 0)).toBe("Agentic");
    expect(accessoryLabel({ kind: "text", value: "1.2M ctx" }, 0)).toBe("1.2M ctx");
  });

  it("renders nothing for an icon", () => {
    expect(accessoryLabel({ kind: "icon", value: "zap" }, 0)).toBe("");
  });
});
