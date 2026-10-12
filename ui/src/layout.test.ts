// The shape rule (ADR-0018 / ADR-0042). Only the metadata widening has its own case here; the rest of
// the rule is pinned by the fact that every renderer reads it rather than re-deriving a shape.

import { describe, expect, it } from "vitest";
import { pageShapeOf } from "./layout";
import type { Item } from "./types";

function item(fields: Partial<Item> = {}): Item {
  return { id: "a", title: "a", actions: [], payload: null, ...fields };
}

describe("pageShapeOf", () => {
  it("is a list with nothing to show beside the rows", () => {
    expect(pageShapeOf(item(), false, 3)).toBe("list");
    expect(pageShapeOf(item({ subtitle: "just text" }), false, 3)).toBe("list");
  });

  it("opens a pane for Markdown, a stream, or metadata alone", () => {
    // Metadata counts: an item whose only content is facts still has something to show (ADR-0042).
    expect(pageShapeOf(item({ detail: "x" }), false, 3)).toBe("split");
    expect(pageShapeOf(item({ pending: true }), false, 3)).toBe("split");
    expect(pageShapeOf(item({ metadata: [{ kind: "separator" }] }), false, 3)).toBe("split");
  });

  it("keeps full-screen detail an Extension declaration", () => {
    // `detailFull` decides the shape; a multi-item result with metadata stays a split page.
    expect(pageShapeOf(item({ detail: "x" }), true, 1)).toBe("detail");
    expect(pageShapeOf(item({ detail: "x" }), true, 2)).toBe("split");
    expect(pageShapeOf(item({ metadata: [{ kind: "separator" }] }), true, 1)).toBe("detail");
  });

  it("is a list when nothing is focused", () => {
    expect(pageShapeOf(undefined, true, 1)).toBe("list");
  });
});
