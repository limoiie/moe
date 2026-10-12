// The selection's presentation rules (MOE-0057, MOE-0059): the pure rules, lifted out of `main.ts` so
// a test can reach them — `main.ts` queries the DOM at module load and cannot be imported at all.
//
// Two surfaces ask two different questions of the same capture. The Input Bar's chip answers "will ⏎
// use my selection?" for the focused command; the Quick Ask composer's chip answers "what will this
// message carry?" for the conversation about to start. One module, so the two can never disagree about
// what was captured or what to call it.

import type { CommandMeta } from "./types";

/**
 * The captured selection's shape, mirroring the backend's `SelectionStatePayload` (ADR-0006). Shape
 * only: this is what the chips render from, so the selected text itself never crosses to the panel
 * until the user asks to see it (`SelectionDetail`).
 */
export interface SelectionState {
  /** Characters of selected text captured at summon (0 = none). */
  textChars: number;
  /** Finder-selected files captured alongside the text (0 = none). */
  fileCount: number;
  /** The clipboard fallback (MOE-0013) is still reading: "not yet known" must not read as "nothing". */
  pending: boolean;
}

/** Before any summon has answered (panel boot): nothing captured, nothing pending. */
export const NO_SELECTION: SelectionState = { textChars: 0, fileCount: 0, pending: false };

/**
 * The captured selection itself, as `selection_detail` returns it — the contract's `Selection` shape.
 * Fetched on demand (the peek), never at boot.
 */
export interface SelectionDetail {
  text: string | null;
  /** Absolute paths of the Finder-selected files. */
  files: string[];
}

/** What a chip shows; `state` drives its material through `data-state`. */
export interface SelectionIndicator {
  state: "pending" | "captured" | "none";
  /** Semantic icon name (ADR-0012): the text glyph for a text capture, the paperclip for files. */
  icon: string;
  label: string;
  /** A muted trailing detail (the composer chip's counts). Omitted when the label says it all. */
  detail?: string;
  /**
   * The longer explanation. The Input Bar chip renders it as a native tooltip; the composer chip
   * shows it as the peek card's caption instead, because a native tooltip would fight that card.
   */
  title: string;
}

/** "2 files" / "1 file" — the label's and the tooltip's shared phrase. */
function filePhrase(count: number): string {
  return `${count} ${count === 1 ? "file" : "files"}`;
}

/** "1,234 characters" / "1 character" — the tooltip's and the composer chip's shared phrase. */
function charPhrase(count: number): string {
  return `${count.toLocaleString()} character${count === 1 ? "" : "s"}`;
}

/** The glyph for what was captured: text wins when both kinds are there (the text is the subject). */
function selectionIcon(state: SelectionState): string {
  return state.textChars > 0 ? "text-cursor-input" : "paperclip";
}

/**
 * The Input Bar's Selection Indicator: whether the summon captured anything for the command the
 * focused row would run. `undefined` = no chip.
 *
 * Two gates, both deliberate. The chip speaks for the **focused command on the command layer** only:
 * it answers "will ⏎ use my selection?", which a result page (the command already ran) and the chat
 * page (no input) have no question to ask. And it appears only when that command declares
 * `input: "selection"` — for every other command, whether a selection was captured is not its
 * business, and a chip that showed anyway would be noise on every row.
 */
export function selectionIndicator(
  view: { mode: "commands" | "items" | "chat"; commands: CommandMeta[]; focus: number },
  state: SelectionState,
): SelectionIndicator | undefined {
  if (view.mode !== "commands") return undefined;
  if (view.commands[view.focus]?.input !== "selection") return undefined;
  if (state.pending) {
    // The clipboard fallback is still reading: "No selection" now and "Selection" a moment later is
    // exactly the flip MOE-0055 taught the panel to avoid.
    return {
      state: "pending",
      icon: selectionIcon(state),
      label: "Reading selection…",
      title: "Moe is still reading the selection from the app it was summoned over.",
    };
  }
  if (state.textChars > 0) {
    const files = state.fileCount > 0 ? ` and ${filePhrase(state.fileCount)}` : "";
    return {
      state: "captured",
      icon: selectionIcon(state),
      label: state.fileCount > 0 ? `Selection + ${filePhrase(state.fileCount)}` : "Selection",
      title: `${charPhrase(state.textChars)} selected${files}.`,
    };
  }
  if (state.fileCount > 0) {
    return {
      state: "captured",
      icon: selectionIcon(state),
      label: filePhrase(state.fileCount),
      title: `${filePhrase(state.fileCount)} selected in Finder.`,
    };
  }
  return {
    state: "none",
    icon: selectionIcon(state),
    label: "No selection",
    title: "Nothing was selected when Moe was summoned.",
  };
}

/**
 * The Quick Ask composer's target chip (MOE-0059): the selection the next message will carry.
 *
 * `undefined` = no chip. A started conversation already has the selection inside its first message —
 * ADR-0019/0021 give the captured context to the conversation's **first message only** — so the chip
 * belongs to a blank page, and it goes away the moment the first send leaves (`armed` is false while
 * one is in flight). Nothing captured is not news either: unlike the Input Bar's indicator there is
 * no "No selection" state here, because a chat without a target works fine.
 */
export function chatTarget(
  page: { mode: "commands" | "items" | "chat"; armed: boolean },
  state: SelectionState,
): SelectionIndicator | undefined {
  if (page.mode !== "chat" || !page.armed) return undefined;
  if (state.pending) {
    return {
      state: "pending",
      icon: selectionIcon(state),
      label: "Reading selection…",
      title: "Moe is still reading the selected text; it will be carried once it lands.",
    };
  }
  if (state.textChars > 0) {
    const detail =
      state.fileCount > 0
        ? `${charPhrase(state.textChars)} · ${filePhrase(state.fileCount)}`
        : charPhrase(state.textChars);
    return {
      state: "captured",
      icon: selectionIcon(state),
      label: "Selection",
      detail,
      title: "The next message carries the selected text as context.",
    };
  }
  if (state.fileCount > 0) {
    return {
      state: "captured",
      icon: selectionIcon(state),
      label: filePhrase(state.fileCount),
      title: "The next message attaches the files selected in Finder.",
    };
  }
  return undefined;
}
