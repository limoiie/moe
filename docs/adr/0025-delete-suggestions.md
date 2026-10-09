# Deleting suggestions: forget recently used commands

ADR-0023 put a "Suggestions" section at the top of the empty root page. Once commands pile up
there, the user needs to curate it — Raycast's equivalent is removing a suggestion (and clearing
recents). This ADR wires the existing delete slots (⌃X / ⌃⇧X, ADR-0022) onto that section.

## Decision

- **⌃X on a focused suggestion forgets that command's usage**; **⌃⇧X clears all recent usage**
  (every frecency entry). The commands themselves are code — nothing is "deleted" from the
  registry: a forgotten command simply leaves the Suggestions section and stays listed in its
  source section below.
- Backend: `Frecency::remove(id)` / `clear()` in moe-core (frecency already persisted
  `last_used_unix` per entry, so forgetting is pure deletion, no migration). Two IPC endpoints:
  `delete_suggestion(command_id) -> bool` and `clear_suggestions() -> usize`, which save the
  frecency file immediately after mutating.
- **Delete slots are platform-level**, so the same keys land on different storage per context:
  on a results layer they route to the owning Extension's `delete_item`/`delete_all` hooks
  (ADR-0022); on the Suggestions section they act on platform frecency (this ADR). The panel
  decides by context: results layer first; otherwise, in commands mode, only when the focused
  row is inside the Suggestions section (tracked via the section shape returned by search).
  Everywhere else the native cut passes through.
- After deleting, the panel re-runs the current query so the Suggestions section updates
  immediately (focus resets to the top, as after any list refresh).

## Cost

- Clearing suggestions also resets the frecency **ordering weights** (score = the exponentially
  decayed use counter, ADR-0023 amendment) — the section and the tie-breaking share one data
  structure, so "forget all" means search ordering falls back to plain matching order until usage
  accumulates again. This is consistent, not a bug.
- UI relies on the first section being titled "Suggestions" to count its rows
  (`ui/src/main.ts` `suggestionsCount`); if ADR-0023's section ordering ever changes, that
  detection must follow.
