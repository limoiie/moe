# M9o: the Filter Control narrows a result list by tags

- **ID**: MOE-0043
- **State**: todo
- **Labels**: ready-for-agent, feature, ux
- **Created**: 2026-10-12
- **Updated**: 2026-10-12
- **Blocked by**: MOE-0041, MOE-0042
- **Blocks**: MOE-0046, MOE-0048

The tracer bullet. There is no way to narrow a list by a second dimension: `Item` has no tags and
`ActionResult::List` carries only `items` and `detail_full`. ADR-0043 adopts Raycast's
`List.Dropdown` search-bar accessory as a **Filter Control** in the Input Bar plus a **Filter Card**,
with filtering evaluated by the platform.

This ticket delivers the visible path end to end. The correctness rules that surround it — values
that must not vanish while typing, a frozen filter while a stream runs, session-scoped state — are
**MOE-0046**, not here.

## Decision

ADR-0043 holds the reasoning. The load-bearing points:

- **`Item.tags` plus a dimension declaration** on `ActionResult::List` (id, label, optional icon,
  optional explicit value list). Values are **derived from the returned items' tags** when no list is
  given, so a declared list cannot drift from the tags actually present. Filtering is client-side;
  server-side (a filter argument on `Extension::invoke`) is the named fallback if pagination lands.
  `payload` was never an option — it is documented as opaque to the UI.
- **A platform-supplied "All ⟨label⟩" row resets**, and the active value carries a check via the
  existing `CardRow.checked` (`CardRow`, `ui/src/card.ts`) — no new row capability. No sections in
  v1: derived values have no natural grouping.
- **The control is `#query-row`'s third child** (`#query-row`, `ui/index.html`), which has two today,
  rendering **label + chevron** — "Tags" at rest, "Tags: Agentic" when set, truncated — with a right
  inset mirroring the row's `px-5`.
- **It must not change the Input Bar's height.** Panel height is measured from the real bar
  (`measuredPanelHeight`, `ui/src/main.ts`) but `resize_panel` fires only at startup and on a shape
  change — `applyPanelSize` early-returns, and there is no `ResizeObserver`. A control that grew the
  row mid-session would silently clip the ten-row grid (ADR-0026) with no error.
- **Result pages only** — `list` and `split`. Not `detail` (nothing to narrow), not the command layer
  (the root groups by Extension source per ADR-0020, where a tag axis has no meaning).
- **No global shortcut.** Raycast binds ⌘P here; Moe's ⌘P is `Browse` (`Browse`,
  `crates/moe-core/src/keymap.rs`) and toggles the history card in the Side View (`toggleHistoryCard`,
  `ui/src/chat.ts`). Keyboard access comes from a **"Filter by Tag" row in the ⌘K card** — Raycast's
  own `ActionPanel.Submenu` pattern — plus click and Tab. The retired `⌘⇧P` is **not** revived
  (ADR-0015 amendment).
- **The Filter Card anchors top-right under its trigger**, per the now-explicit rule that a card
  anchors near its trigger (ADR-0028 amendment). Built on `createCard` and registered in the layer
  stack.
- **AND with the query** in the basic case: the query re-runs the command (Live List), the tag narrows
  what comes back.
- **First real consumer:** `Conversation.agentic` exists in storage but never reaches the `Item`
  (`Conversation`, `crates/moe-core/src/conversation.rs`) — chat history is the natural first
  dimension, and it is what makes this ticket demoable.

## Delivery

A vertical slice: Rust contract → the shared contract mirror → the control and card → the ⌘K row →
one Extension that declares a dimension.

- `crates/moe-core/src/contract.rs` — `Item.tags` and the dimension declaration on
  `ActionResult::List`; the shared mirror in `ui/src/types.ts`.
- `ui/index.html`, `ui/src/main.ts` — the control as the input row's third child, and the card.
- `ui/src/actions.ts` — a "Filter by Tag" row for pages that declare a dimension.
- `ui/src/styles.css`.
- The first consumer in `crates/moe-extensions/`.

## Acceptance

- [ ] The control renders only on `list` / `split` result pages whose result declares a dimension,
      and the measured Input Bar height is identical with and without it.
- [ ] Choosing a value narrows the list; the label shows the active value; "All ⟨label⟩" restores
      everything; the active value carries a check.
- [ ] Esc / empty-⌫ closes the Filter Card before it backs out of the page, and the ⌘K row opens it.
- [ ] No global binding was added or changed.
- [ ] A row's tags never render as row accessories on a `split` page (ADR-0041 hides them there).
- [ ] `just lint`, `just test`, `just ui-build` green.

## Comments

- 2026-10-12 (agent): filed from the Raycast primitives grilling session; ADR-0043 records the
  decision, and divergence #2 in `docs/research/raycast-ui-primitives.md` records the ⌘P cost.
  Blocked by MOE-0041 and MOE-0042 so the Filter Card is built on the unified component and registers
  in the stack once, rather than becoming a sixth hand-rolled surface.
- 2026-10-12 (agent): **scope trimmed.** This was eight behaviors plus a consumer in one ticket — too
  much for one context window. The visible path stays here; the edge rules move to MOE-0046, which is
  separately demoable ("type a query and the tag list does not shrink"). Splitting also removed the
  reason to hold a second dimension hostage: MOE-0048's TagList needs a declared dimension, so it now
  blocks on this ticket directly.