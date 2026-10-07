// UI-side mirror of platform generic actions (ADR-0014/0022/0029). Bindings are set by `moe-core::keymap`:
//   Browse ⌘P (record list) · Actions ⌘⇧P (action list) · New ⌘N (new record)
//   Delete ⌃X (delete current record) · DeleteAll ⌃⇧X (delete all records)
//   Favorite ⌘⇧F (add/remove the current command from Favorites)
// Each surface (command panel / Side View / future sub-apps) only does two things:
//   1. Recognize keyboard events into semantics with `generalActionOf(event)`;
//   2. Decide where that semantic lands on this surface (panel: the entry/delete hooks an Extension declares; Side View: local views).

export type GeneralAction = "browse" | "actions" | "new" | "delete" | "deleteAll" | "favorite";

/** The generic actions that require an Extension-declared entry (one-to-one with Rust `EntryKind`). */
export type EntryAction = Exclude<GeneralAction, "actions" | "delete" | "deleteAll" | "favorite">;

/** Display strings (for tooltips/hints), matching the Rust keymap table. */
export const GENERAL_KEY_LABELS: Record<GeneralAction, string> = {
  browse: "⌘P",
  actions: "⌘⇧P",
  new: "⌘N",
  delete: "⌃X",
  deleteAll: "⌃⇧X",
  favorite: "⌘⇧F",
};

/**
 * Recognize a generic action; returns null for any other keys.
 * The delete slot only accepts ⌃ (not ⌘); Browse/New/Favorite only accept ⌘ (⌃P/⌃N are Navigation).
 */
export function generalActionOf(e: KeyboardEvent): GeneralAction | null {
  const key = e.key.toLowerCase();
  // Delete / DeleteAll (ADR-0022): ⌃X / ⌃⇧X, never mixed with ⌘
  if (e.ctrlKey && !e.metaKey && !e.altKey && key === "x") {
    return e.shiftKey ? "deleteAll" : "delete";
  }
  if (!e.metaKey || e.ctrlKey) return null;
  if (key === "p") return e.shiftKey ? "actions" : "browse";
  if (key === "n" && !e.shiftKey) return "new";
  // Favorite (ADR-0029): ⌘⇧F toggles the current command in/out of Favorites
  if (key === "f" && e.shiftKey) return "favorite";
  return null;
}
