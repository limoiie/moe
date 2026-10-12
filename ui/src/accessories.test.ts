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
  it("hides every accessory while the detail pane shows", () => {
    // Raycast's own recommendation: the information belongs in the pane, not the narrow left column.
    expect(rowAccessories(item(tags(2)), "split")).toEqual([]);
  });

  it("shows them on both list shapes", () => {
    const a = tags(2);
    expect(rowAccessories(item(a), "list")).toEqual(a);
    expect(rowAccessories(item(a), "detail")).toEqual(a);
  });

  it("caps at three, dropping the rest in declared order", () => {
    expect(rowAccessories(item(tags(5)), "list")).toHaveLength(MAX_ACCESSORIES);
    expect(rowAccessories(item(tags(5)), "list").map((x) => x.value)).toEqual(["t0", "t1", "t2"]);
  });

  it("is empty when the item carries none", () => {
    expect(rowAccessories(item(), "list")).toEqual([]);
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
