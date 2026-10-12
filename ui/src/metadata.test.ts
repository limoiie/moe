// The tag-click rule (ADR-0042 / MOE-0048): which of the three things a tag does, and — just as
// important — that the third one is the fallback rather than an error.

import { describe, expect, it } from "vitest";
import { tagIntent } from "./metadata";
import type { Action } from "./types";

const use: Action = { id: "use", title: "Use This Model", kind: "primary" };
const copy: Action = { id: "copy", title: "Copy", kind: "secondary" };

describe("tagIntent", () => {
  it("prefers the filter when the text is a declared value", () => {
    // Even when the tag also carries a resolvable action: the filter rule wins, as ADR-0042 says.
    expect(tagIntent({ text: "Agentic", actionId: "copy" }, [copy], ["Agentic"])).toEqual({
      kind: "filter",
      value: "Agentic",
    });
  });

  it("runs an action the item declares", () => {
    expect(tagIntent({ text: "Copy", actionId: "copy" }, [use, copy], [])).toEqual({
      kind: "action",
      actionId: "copy",
    });
  });

  it("is display-only when the id resolves to nothing", () => {
    // One bad id must not error, and must not stop the row's other tags from working.
    expect(tagIntent({ text: "Nope", actionId: "missing" }, [use], [])).toEqual({ kind: "none" });
  });

  it("is display-only with neither a match nor an id", () => {
    expect(tagIntent({ text: "Agentic" }, [use], [])).toEqual({ kind: "none" });
    expect(tagIntent({ icon: "zap" }, [use], ["Agentic"])).toEqual({ kind: "none" });
  });

  it("does not filter on a page with no declared dimension", () => {
    // `filterValues` is empty exactly when the result declares none (ADR-0043), so the same tag is
    // display-only on a page whose page has no axis to narrow.
    expect(tagIntent({ text: "Agentic" }, [], [])).toEqual({ kind: "none" });
  });

  it("matches the dimension exactly, not by prefix or case", () => {
    expect(tagIntent({ text: "agentic" }, [], ["Agentic"])).toEqual({ kind: "none" });
    expect(tagIntent({ text: "Agentic-" }, [], ["Agentic"])).toEqual({ kind: "none" });
  });
});
