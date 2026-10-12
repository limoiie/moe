# M9t: metadata TagList, clickable tags, and tag→filter

- **ID**: MOE-0048
- **State**: done
- **Labels**: ready-for-agent, feature, ux
- **Created**: 2026-10-12
- **Updated**: 2026-10-12
- **Commit**: d2d6742
- **Blocked by**: MOE-0043 (done), MOE-0044 (done)
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

- [x] `tagList` renders in both placements, from one shared renderer, in ADR-0041's tokens — the split
      pane's bottom and, since **MOE-0047**, the `detail` shape's right-hand column. Same `metadataEl`,
      different container.
- [x] A tag with a valid `actionId` runs that action; an invalid one renders display-only, and the
      same list's other tags still work — the rule is `tagIntent`, pinned in `metadata.test.ts`
      including the precedence case (a tag that matches a dimension *and* carries an id filters).
      **Not exercised on screen:** no Extension declares an `actionId` yet.
- [x] Clicking a tag whose text matches a declared dimension narrows the list and updates the Filter
      Control's label — **verified:** history's `Agentic` tag narrows the list and the control reads
      "Tags: Agentic". The shared `setFilterValue` path is what carries it.
- [x] A tag with no `actionId` and no matching dimension is display-only and does not look clickable:
      `tagIntent` returns `none` and the renderer emits a `<span>`, never a button — so it cannot look
      interactive by accident.
- [x] 40 UI tests (was 34), 253 Rust tests, `tsc`, `vite build`, clippy green. **`just lint` is red on
      the pre-existing `cargo fmt` diff in `anthropic.rs`** — unchanged by this work.

## Comments

- 2026-10-12 (agent): split out of MOE-0044 during the granularity pass. The tag→filter rule needed a
  declared filter dimension, which was an invisible coupling while it lived inside the metadata
  ticket; making it an explicit `Blocked by: MOE-0043` is the point of the split. Blocks MOE-0045,
  which decides how these tags are reached from the keyboard.
- 2026-10-12 (agent): **landed** (d2d6742). The rule is a pure `tagIntent` in `ui/src/metadata.ts`
  (filter → action → display-only, with ADR-0042's precedence), and `main.ts` only wires it: tags that
  resolve to something are `<button tabindex="-1">`, the rest are spans, so display-only can never look
  clickable. Interactive tags are kept out of the tab order on purpose — ADR-0042 names them
  pointer-only, and **MOE-0045 now owns that decision**, which is why this is a `tabindex` seam rather
  than a `<span>` with a click handler.

  **The first real producer landed here**, because the ticket is otherwise invisible: chat history's
  `Agentic` tag now also appears as a clickable tag in its detail pane. That page already declares the
  dimension and derives its values from `Item.tags`, so the loop closes with no new declarations — see
  the tag, click it, the list narrows. This is the consumer MOE-0044's amendment said this ticket
  should bring.

  Left for **MOE-0047**: the second placement. `metadataEl` already takes its item and container, so
  that ticket reuses this renderer rather than growing a second one.