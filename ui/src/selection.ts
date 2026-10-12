// The Input Bar's Selection Indicator (MOE-0057): the pure rules, lifted out of `main.ts` so a test
// can reach them — `main.ts` queries the DOM at module load and cannot be imported at all.

import type { CommandMeta } from "./types";

/**
 * The captured selection's shape, mirroring the backend's `SelectionStatePayload` (ADR-0006). Shape
 * only: the panel never reads the selected text, which stays between the platform and the Extension.
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

/** What the chip shows; `state` drives its material through `data-state`. */
export interface SelectionIndicator {
  state: "pending" | "captured" | "none";
  label: string;
  title: string;
}

/** "2 files" / "1 file" — the label's and the tooltip's shared phrase. */
function filePhrase(count: number): string {
  return `${count} ${count === 1 ? "file" : "files"}`;
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
      label: "Reading selection…",
      title: "Moe is still reading the selection from the app it was summoned over.",
    };
  }
  if (state.textChars > 0) {
    const files = state.fileCount > 0 ? ` and ${filePhrase(state.fileCount)}` : "";
    return {
      state: "captured",
      label: state.fileCount > 0 ? `Selection + ${filePhrase(state.fileCount)}` : "Selection",
      title: `${state.textChars.toLocaleString()} character${state.textChars === 1 ? "" : "s"} selected${files}.`,
    };
  }
  if (state.fileCount > 0) {
    return {
      state: "captured",
      label: filePhrase(state.fileCount),
      title: `${filePhrase(state.fileCount)} selected in Finder.`,
    };
  }
  return {
    state: "none",
    label: "No selection",
    title: "Nothing was selected when Moe was summoned.",
  };
}
