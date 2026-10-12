# M9t: metadata TagList, clickable tags, and tag→filter

- **ID**: MOE-0048
- **State**: todo
- **Labels**: ready-for-agent, feature, ux
- **Created**: 2026-10-12
- **Updated**: 2026-10-12
- **Blocked by**: MOE-0043, MOE-0044
- **Blocks**: MOE-0045

The interactive half of the metadata block. ADR-0042's `TagList` renders in ADR-0041's palette, a tag
may carry an action, and a tag that matches a declared filter dimension sets the Filter Control — the
one place ADR-0042 knowingly goes past Raycast, because Raycast's `TagList.Item.onAction` exists but
Raycast has no filter dimension to match against.

## Decision

- **`tagList` renders with ADR-0041's nine tokens** — one palette across accessories, card rows and
  the detail pane, never a second vocabulary.
- **A tag may be clickable, by reference.** Raycast's `TagList.Item` has `onAction`; a closure cannot
  cross the IPC boundary, so a Moe tag carries an optional `actionId` that must resolve to one of the
  item's declared `Action`s and dispatches through `run_item_action` (`run_item_action`,
  `crates/moe-core/src/contract.rs`). No `actionId` means display-only; an *unresolvable* id also
  renders display-only rather than erroring, so the row's other tags keep working.
- **A tag whose text matches a declared filter dimension sets the Filter Control** instead of running
  an action, closing the loop between `Item.tags` and the pane's `TagList`: see a tag, click it, the
  list narrows. If filters are ever removed, this rule goes with them — ADR-0042's Cost names the
  coupling.

## Delivery

- `ui/src/main.ts` — the `tagList` renderer in both placements, the action dispatch path, and the
  filter-matching rule.
- `ui/src/types.ts` — the mirror's `actionId` field, if the contract needs it added.
- `ui/src/styles.css` — tag rendering in the pane.

## Acceptance

- [ ] `tagList` renders in both placements, from one shared renderer, in ADR-0041's tokens.
- [ ] A tag with a valid `actionId` runs that action; an invalid one renders display-only, and the
      same list's other tags still work.
- [ ] Clicking a tag whose text matches a declared filter dimension narrows the list and updates the
      Filter Control's label.
- [ ] A tag with no `actionId` and no matching dimension is display-only and does not look clickable.
- [ ] `just lint`, `just test`, `just ui-build` green.

## Comments

- 2026-10-12 (agent): split out of MOE-0044 during the granularity pass. The tag→filter rule needed a
  declared filter dimension, which was an invisible coupling while it lived inside the metadata
  ticket; making it an explicit `Blocked by: MOE-0043` is the point of the split. Blocks MOE-0045,
  which decides how these tags are reached from the keyboard.