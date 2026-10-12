// The metadata block's *rendering* (ADR-0042), which until MOE-0052 had no automated check at all:
// every claim about how it draws was confirmed by hand, because the renderer lived in `main.ts`, which
// queries the DOM at module load and cannot be imported by a test. The renderer now lives in
// `metadata.ts` with its side effects injected, so this file can drive it.
//
// jsdom asserts element kinds, classes, order and dispatch — deliberately not layout (jsdom does not
// lay anything out), so the row-height and column-width questions stay hand-checked.
// @vitest-environment jsdom

import { describe, expect, it, vi } from "vitest";
import { metadataEl, type MetadataDeps } from "./metadata";
import type { Action, Item, Metadata } from "./types";

function deps(overrides: Partial<MetadataDeps> = {}): MetadataDeps {
  return {
    dimensionValues: [],
    onFilter: vi.fn(),
    onAction: vi.fn(),
    onOpenLink: vi.fn(),
    ...overrides,
  };
}

function item(actions: Action[] = []): Item {
  return { id: "a", title: "a", actions, payload: null };
}

describe("metadataEl", () => {
  it("renders rows in the Extension's declared order, with the palette color on the inner span", () => {
    const metadata: Metadata[] = [
      { kind: "label", title: "Provider", text: "Anthropic" },
      { kind: "separator" },
      { kind: "label", title: "Model", text: "claude-sonnet-4-5", color: "blue", icon: "info" },
    ];
    const block = metadataEl(metadata, item(), deps());
    expect(block.className).toContain("moe-metadata");
    expect(
      [...block.querySelectorAll(".moe-metadata-title")].map((el) => el.textContent),
    ).toEqual(["Provider", "Model"]);
    expect(block.querySelectorAll(".moe-metadata-separator")).toHaveLength(1);
    // The ink class rides the inner span so it outranks the wrapper's default text step by
    // specificity rather than by rule order (the reason `valueEl` has two spans at all).
    const value = block.querySelectorAll(".moe-metadata-value")[1];
    expect(value.querySelector(".moe-ink-blue")).not.toBeNull();
    expect(value.querySelector("svg")).not.toBeNull();
    expect(value.textContent).toContain("claude-sonnet-4-5");
  });

  it("renders a link as a button that hands its target to the platform", () => {
    // A button rather than an `<a>`: the panel is a webview and a raw href would navigate it.
    const onOpenLink = vi.fn();
    const block = metadataEl(
      [{ kind: "link", title: "Docs", text: "example.com", target: "https://example.com" }],
      item(),
      deps({ onOpenLink }),
    );
    const link = block.querySelector<HTMLButtonElement>("button.moe-metadata-link");
    expect(link?.title).toBe("https://example.com");
    link?.click();
    expect(onOpenLink).toHaveBeenCalledWith("https://example.com");
  });

  it("is a plain span for a display-only tag and a button for a live one", () => {
    // The rule this exists to protect: a tag that can do nothing must not look like it can.
    const tagList: Metadata[] = [
      { kind: "tagList", title: "Tags", items: [{ text: "Agentic" }] },
    ];
    const idle = metadataEl(tagList, item(), deps({ dimensionValues: [] }));
    expect(idle.querySelector("span.moe-metadata-tag")).not.toBeNull();
    expect(idle.querySelector("button.moe-metadata-tag")).toBeNull();

    const live = metadataEl(tagList, item(), deps({ dimensionValues: ["Agentic"] }));
    const tag = live.querySelector<HTMLButtonElement>("button.moe-metadata-tag-live");
    expect(tag).not.toBeNull();
    // Pointer-only by ADR-0042; this is the seam MOE-0045 would change.
    expect(tag?.tabIndex).toBe(-1);
  });

  it("filters on a matching tag and runs the action a tag names", () => {
    const onFilter = vi.fn();
    const onAction = vi.fn();
    const copy: Action = { id: "copy", title: "Copy", kind: "secondary" };
    const block = metadataEl(
      [
        {
          kind: "tagList",
          title: "Tags",
          items: [{ text: "Agentic", color: "purple" }, { text: "Copy", actionId: "copy" }],
        },
      ],
      item([copy]),
      deps({ onFilter, onAction, dimensionValues: ["Agentic"] }),
    );
    const tags = block.querySelectorAll<HTMLButtonElement>("button.moe-metadata-tag");
    expect(tags).toHaveLength(2);
    expect(tags[0].classList.contains("moe-ink-purple")).toBe(true);
    tags[0].click();
    tags[1].click();
    expect(onFilter).toHaveBeenCalledWith("Agentic");
    expect(onAction).toHaveBeenCalledWith(expect.objectContaining({ id: "a" }), copy);
  });

  it("leaves a tag whose actionId resolves to nothing display-only, and the row's others live", () => {
    // One bad id must not error, and must not take the rest of the row with it.
    const onAction = vi.fn();
    const block = metadataEl(
      [
        {
          kind: "tagList",
          title: "Tags",
          items: [{ text: "Nope", actionId: "missing" }, { text: "Agentic" }],
        },
      ],
      item(),
      deps({ onAction, dimensionValues: ["Agentic"] }),
    );
    expect(block.querySelectorAll("button.moe-metadata-tag")).toHaveLength(1);
    expect(block.querySelectorAll("span.moe-metadata-tag")).toHaveLength(1);
    expect(onAction).not.toHaveBeenCalled();
  });
});
