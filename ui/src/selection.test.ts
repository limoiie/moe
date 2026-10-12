// The Selection Indicator's rules (MOE-0057). Each test is a way the chip could lie: shown for a
// command that does not take a selection, claiming "no selection" while the capture is still
// reading, or dropping the Finder files that came along with the text.

import { describe, expect, it } from "vitest";
import { NO_SELECTION, selectionIndicator, type SelectionState } from "./selection";
import type { CommandMeta } from "./types";

/** The smallest CommandMeta that satisfies the type; only `input` matters to these rules. */
function command(input: CommandMeta["input"]): CommandMeta {
  return { id: "cmd", extensionId: "ext", title: "Cmd", input, live: false };
}

/** The command layer with `command` focused. */
function layer(input: CommandMeta["input"]) {
  return { mode: "commands" as const, commands: [command(input)], focus: 0 };
}

function state(overrides: Partial<SelectionState>): SelectionState {
  return { ...NO_SELECTION, ...overrides };
}

describe("selectionIndicator", () => {
  it("shows nothing for a command that takes no selection", () => {
    expect(selectionIndicator(layer("none"), state({ textChars: 12 }))).toBeUndefined();
    expect(selectionIndicator(layer("query"), state({ textChars: 12 }))).toBeUndefined();
  });

  it("shows nothing when the command layer has no focused command", () => {
    expect(
      selectionIndicator({ mode: "commands", commands: [], focus: 0 }, state({ textChars: 12 })),
    ).toBeUndefined();
  });

  it("shows nothing on a result page or the chat page, even for a selection command", () => {
    // The command already ran there; the question the chip answers ("will ⏎ use my selection?") is
    // only live while the command is still the focused row.
    const commands = [command("selection")];
    expect(selectionIndicator({ mode: "items", commands, focus: 0 }, state({ textChars: 12 }))).toBeUndefined();
    expect(selectionIndicator({ mode: "chat", commands, focus: 0 }, state({ textChars: 12 }))).toBeUndefined();
  });

  it("reads as pending while the clipboard fallback is still reading", () => {
    // Not "No selection": the capture has not answered yet, and the flip would be visible.
    const indicator = selectionIndicator(layer("selection"), state({ pending: true }));
    expect(indicator?.state).toBe("pending");
    expect(indicator?.label).toBe("Reading selection…");
  });

  it("reports captured text with the character count", () => {
    const indicator = selectionIndicator(layer("selection"), state({ textChars: 12 }));
    expect(indicator?.state).toBe("captured");
    expect(indicator?.label).toBe("Selection");
    expect(indicator?.title).toBe("12 characters selected.");
  });

  it("singularizes a one-character selection", () => {
    expect(selectionIndicator(layer("selection"), state({ textChars: 1 }))?.title).toBe(
      "1 character selected.",
    );
  });

  it("groups a long count the way the reader's locale does", () => {
    // `toLocaleString` in the expectation too: the grouping is the runtime's, not ours to pin.
    expect(selectionIndicator(layer("selection"), state({ textChars: 1234 }))?.title).toBe(
      `${(1234).toLocaleString()} characters selected.`,
    );
  });

  it("names the Finder files that came along with the text", () => {
    const indicator = selectionIndicator(layer("selection"), state({ textChars: 12, fileCount: 2 }));
    expect(indicator?.state).toBe("captured");
    expect(indicator?.label).toBe("Selection + 2 files");
    expect(indicator?.title).toBe("12 characters selected and 2 files.");
  });

  it("reports files alone as captured, singular or plural", () => {
    const two = selectionIndicator(layer("selection"), state({ fileCount: 2 }));
    expect(two?.state).toBe("captured");
    expect(two?.label).toBe("2 files");
    expect(two?.title).toBe("2 files selected in Finder.");
    expect(selectionIndicator(layer("selection"), state({ fileCount: 1 }))?.label).toBe("1 file");
  });

  it("says no selection only when the capture has settled on nothing", () => {
    const indicator = selectionIndicator(layer("selection"), NO_SELECTION);
    expect(indicator?.state).toBe("none");
    expect(indicator?.label).toBe("No selection");
  });
});
