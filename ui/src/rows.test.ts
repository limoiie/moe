// The result row's rendering (ADR-0018/0030/0041), which until MOE-0053 was checked by hand only: the
// renderer lived in `main.ts`, which queries the DOM at module load and cannot be imported by a test.
// It now lives in `rows.ts` with its side effects injected (ADR-0007's amendment).
//
// jsdom asserts structure — element kind, class, order and dispatch — and deliberately not layout, so
// the row's 40px height and the title's truncation stay hand-checked.
// @vitest-environment jsdom

import { describe, expect, it, vi } from "vitest";
import { rowEl, type RowDeps } from "./rows";
import type { Accessory } from "./types";

function deps(overrides: Partial<RowDeps> = {}): RowDeps {
  return { nowUnix: 1000, onFocus: vi.fn(), ...overrides };
}

describe("rowEl", () => {
  it("marks only the focused row, and hands the flat index to the focus sink", () => {
    const onFocus = vi.fn();
    const idle = rowEl({ title: "Quiet Ask" }, 3, false, deps({ onFocus }));
    const focused = rowEl({ title: "Quick Ask" }, 7, true, deps({ onFocus }));
    expect(idle.dataset.focused).toBeUndefined();
    expect(idle.className).not.toContain("bg-surface-selected");
    expect(focused.dataset.focused).toBe("true");
    expect(focused.className).toContain("bg-surface-selected");

    // A row press moves the focus; hover is CSS, so nothing else may claim it (ADR-0031).
    focused.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
    expect(onFocus).toHaveBeenCalledWith(7);
  });

  it("dims the icon only when it is a fallback shape", () => {
    const declared = rowEl({ title: "t", icon: "sparkles" }, 0, false, deps());
    const fallback = rowEl({ title: "t", iconMuted: true }, 0, false, deps());
    expect(declared.querySelector("svg")?.getAttribute("class")).toContain("text-fg-subtle");
    expect(fallback.querySelector("svg")?.getAttribute("class")).toContain("text-fg-faint");
  });

  it("puts the subtitle inside the title cell, so the title is what truncates", () => {
    const li = rowEl({ title: "explain closures", subtitle: "3d ago" }, 0, false, deps());
    const cell = li.querySelector(".min-w-0.flex-1.truncate");
    expect(cell?.textContent).toBe("explain closures3d ago");
    expect(cell?.querySelector("span.ml-2")?.textContent).toBe("3d ago");
  });

  it("renders accessories after the title cell and before the shortcut and badge", () => {
    // The trailing order is the contract the panel and the Side View share (ADR-0030/0041).
    const accessories: Accessory[] = [
      { kind: "tag", value: "Agentic", color: "purple" },
      { kind: "date", value: 940 },
      { kind: "icon", value: "zap", color: "yellow" },
    ];
    const li = rowEl(
      { title: "t", accessories, key: "⌘P", badge: "Command" },
      0,
      false,
      deps({ nowUnix: 1000 }),
    );
    const cells = [...li.children].map((el) => el.getAttribute("class") ?? "");
    const titleAt = cells.findIndex((c) => c.includes("min-w-0"));
    const accessoryAt = cells.findIndex((c) => c.includes("moe-accessory"));
    // `kbdEl` returns a `.moe-keys` wrapper and the row styles that wrapper (`.moe-row-keys`), so the
    // key block is that element, not its `.moe-kbd` children.
    const keyAt = cells.findIndex((c) => c.includes("moe-keys"));
    const badgeAt = cells.findIndex((c) => c.includes("moe-kind-badge"));
    expect(titleAt).toBeLessThan(accessoryAt);
    expect(accessoryAt).toBeLessThan(keyAt);
    expect(keyAt).toBeLessThan(badgeAt);
  });

  it("renders each accessory kind as its own shape, with the palette on the ink class", () => {
    const li = rowEl(
      {
        title: "t",
        accessories: [
          { kind: "tag", value: "Agentic", color: "purple" },
          { kind: "text", value: "1.2M ctx" },
          { kind: "date", value: 940, tooltip: "12 minutes ago" },
          { kind: "icon", value: "zap", color: "yellow" },
        ],
      },
      0,
      false,
      deps({ nowUnix: 1600 }),
    );
    // One row clock reading for every accessory: the date says "11m ago" for all of them, not one
    // "now" per element (the reason `nowUnix` is a dependency rather than a `Date.now()` in here).
    expect(li.querySelector(".moe-accessory-tag")?.textContent).toBe("Agentic");
    expect(li.querySelector(".moe-accessory-tag")?.classList.contains("moe-ink-purple")).toBe(true);
    expect(li.querySelector(".moe-accessory-text")?.textContent).toBe("1.2M ctx");
    // No color named: the element keeps the secondary step rather than getting an ink class.
    expect(li.querySelector(".moe-accessory-text")?.className).toBe("moe-accessory moe-accessory-text");
    const date = li.querySelector(".moe-accessory-date");
    expect(date?.textContent).toBe("11m ago");
    expect(date?.getAttribute("title")).toBe("12 minutes ago");
    // An icon accessory is a tinted glyph in a span, so the ink class has a `currentColor` to tint.
    const icon = li.querySelector<HTMLElement>(".moe-accessory svg")?.parentElement;
    expect(icon?.classList.contains("moe-ink-yellow")).toBe(true);
  });

  it("reveals the shortcut only where the page asks for it", () => {
    // ADR-0031: on the root page the Kbd is hover/focus-only, via the class styles.css keys off.
    const hidden = rowEl({ title: "t", key: "⌘," }, 0, false, deps());
    const shown = rowEl({ title: "t", key: "⌘,", revealKeys: true }, 0, false, deps());
    expect(hidden.querySelector(".moe-keys")?.classList.contains("moe-row-keys")).toBe(false);
    expect(shown.querySelector(".moe-keys")?.classList.contains("moe-row-keys")).toBe(true);
  });

  it("renders a row with nothing but a title", () => {
    // The degenerate case the command layer relies on: no icon, no subtitle, no badge.
    const li = rowEl({ title: "Echo" }, 0, false, deps());
    expect(li.querySelectorAll("svg")).toHaveLength(1); // the fallback icon
    expect(li.querySelector(".moe-kind-badge")).toBeNull();
    expect(li.textContent).toBe("Echo");
  });
});
