// UI-side mirror of platform generic actions (ADR-0014/0022/0029). Bindings are set by `moe-core::keymap`:
//   Browse ⌘P (record list) · Actions ⌘K (action list, every surface) · New ⌘N (new record)
//   Delete ⌃X (delete current record) · DeleteAll ⌃⇧X (delete all records)
//   Favorite ⌘⇧F (add/remove the current command from Favorites) · OpenConfig ⌘, (open the config file)
// Each surface (command panel / Side View / future sub-apps) only does two things:
//   1. Recognize keyboard events into semantics with `generalActionOf(event)`;
//   2. Decide where that semantic lands on this surface (panel: the entry/delete hooks an Extension declares; Side View: local views).

export type GeneralAction =
  | "browse"
  | "actions"
  | "new"
  | "delete"
  | "deleteAll"
  | "favorite"
  | "openConfig";

/** The generic actions that require an Extension-declared entry (one-to-one with Rust `EntryKind`). */
export type EntryAction = Exclude<
  GeneralAction,
  "actions" | "delete" | "deleteAll" | "favorite" | "openConfig"
>;

/** Display strings (for tooltips/hints), matching the Rust keymap table. */
export const GENERAL_KEY_LABELS: Record<GeneralAction, string> = {
  browse: "⌘P",
  actions: "⌘K",
  new: "⌘N",
  delete: "⌃X",
  deleteAll: "⌃⇧X",
  favorite: "⌘⇧F",
  openConfig: "⌘,",
};

/**
 * Recognize a generic action; returns null for any other keys.
 * The delete slot only accepts ⌃ (not ⌘); Browse/New/Favorite/OpenConfig only accept ⌘ (⌃P/⌃N are Navigation).
 */
export function generalActionOf(e: KeyboardEvent): GeneralAction | null {
  const key = e.key.toLowerCase();
  // Delete / DeleteAll (ADR-0022): ⌃X / ⌃⇧X, never mixed with ⌘
  if (e.ctrlKey && !e.metaKey && !e.altKey && key === "x") {
    return e.shiftKey ? "deleteAll" : "delete";
  }
  if (!e.metaKey || e.ctrlKey) return null;
  // Actions is ⌘K on every surface (ADR-0014 amendment); Browse stays ⌘P
  if (key === "k" && !e.shiftKey) return "actions";
  if (key === "p" && !e.shiftKey) return "browse";
  if (key === "n" && !e.shiftKey) return "new";
  // Favorite (ADR-0029): ⌘⇧F toggles the current command in/out of Favorites
  if (key === "f" && e.shiftKey) return "favorite";
  // OpenConfig (ADR-0027 amendment): ⌘, is the macOS Preferences convention
  if (e.key === "," && !e.shiftKey && !e.altKey) return "openConfig";
  return null;
}
