// The Selection Indicator's rules (MOE-0057). Each test is a way the chip could lie: shown for a
// command that does not take a selection, claiming "no selection" while the capture is still
// reading, or dropping the Finder files that came along with the text.

import { describe, expect, it } from "vitest";
import {
  chatTarget,
  NO_SELECTION,
  selectionIndicator,
  type SelectionState,
} from "./selection";
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

  it("wears the paperclip when only Finder files were captured", () => {
    expect(selectionIndicator(layer("selection"), state({ fileCount: 2 }))?.icon).toBe("paperclip");
    expect(selectionIndicator(layer("selection"), state({ textChars: 2 }))?.icon).toBe(
      "text-cursor-input",
    );
  });
});

describe("chatTarget", () => {
  const chat = (armed: boolean) => ({ mode: "chat" as const, armed });

  it("shows nothing off the Quick Ask page", () => {
    expect(chatTarget({ mode: "commands", armed: true }, state({ textChars: 12 }))).toBeUndefined();
    expect(chatTarget({ mode: "items", armed: true }, state({ textChars: 12 }))).toBeUndefined();
  });

  it("shows nothing once the conversation has started, or while the first send is in flight", () => {
    // The selection belongs to the conversation's first message only (ADR-0019/0021): once that
    // message exists — or is on its way — the chip has nothing left to promise.
    expect(chatTarget(chat(false), state({ textChars: 12 }))).toBeUndefined();
  });

  it("shows nothing when nothing was captured", () => {
    // Unlike the Input Bar's indicator there is no "No selection" state: no target is not news.
    expect(chatTarget(chat(true), NO_SELECTION)).toBeUndefined();
  });

  it("reads as pending while the clipboard fallback is still reading", () => {
    const indicator = chatTarget(chat(true), state({ pending: true }));
    expect(indicator?.state).toBe("pending");
    expect(indicator?.label).toBe("Reading selection…");
  });

  it("names the text target and its size", () => {
    const indicator = chatTarget(chat(true), state({ textChars: 1234 }));
    expect(indicator?.state).toBe("captured");
    expect(indicator?.label).toBe("Selection");
    expect(indicator?.detail).toBe(`${(1234).toLocaleString()} characters`);
    expect(indicator?.icon).toBe("text-cursor-input");
  });

  it("counts the Finder files alongside the text", () => {
    const indicator = chatTarget(chat(true), state({ textChars: 12, fileCount: 2 }));
    expect(indicator?.label).toBe("Selection");
    expect(indicator?.detail).toBe("12 characters · 2 files");
  });

  it("singularizes a one-character target", () => {
    expect(chatTarget(chat(true), state({ textChars: 1 }))?.detail).toBe("1 character");
  });

  it("names a files-only target as the files it is", () => {
    const one = chatTarget(chat(true), state({ fileCount: 1 }));
    expect(one?.label).toBe("1 file");
    expect(one?.icon).toBe("paperclip");
    expect(one?.detail).toBeUndefined();
    expect(chatTarget(chat(true), state({ fileCount: 3 }))?.label).toBe("3 files");
  });
});
