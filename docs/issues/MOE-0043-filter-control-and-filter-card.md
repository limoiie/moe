# M9o: the Filter Control narrows a result list by tags

- **ID**: MOE-0043
- **State**: todo
- **Labels**: ready-for-agent, feature, ux
- **Created**: 2026-10-12
- **Updated**: 2026-10-12
- **Blocked by**: MOE-0041, MOE-0042
- **Blocks**: MOE-0044

There is no way to narrow a list by a second dimension: `Item` has no tags and `ActionResult::List`
carries only `items` and `detail_full` (`contract.rs:71-106`). ADR-0043 adopts Raycast's
`List.Dropdown` search-bar accessory as a **Filter Control** in the Input Bar plus a **Filter Card**,
with filtering evaluated by the platform.

## Decision

ADR-0043 holds the reasoning. The load-bearing points:

- **`Item.tags` plus a dimension declaration** on `ActionResult::List` (id, label, optional icon,
  optional explicit value list). Values are **derived from the returned items' tags** when no list is
  given, so a declared list cannot drift from the tags actually present. Filtering is client-side;
  server-side (a filter argument on `Extension::invoke`) is the named fallback if pagination ever
  lands. `payload` was never an option — it is "opaque to the UI" (`contract.rs:59`).
- **A platform-supplied "All ⟨label⟩" row resets**, and the active value carries a check via the
  existing `CardRow.checked` (`card.ts:24`) — no new row capability. No sections in v1: derived values
  have no natural grouping.
- **The control is `#query-row`'s third child** (`ui/index.html:22-36` has two today), rendering
  **label + chevron** — "Tags" at rest, "Tags: Agentic" when set, truncated — with a right inset
  mirroring the row's `px-5`.
- **It must not change the Input Bar's height.** Panel height is measured from `#query-row`
  (`main.ts:435-439`) but `resize_panel` fires only at startup and on a shape change —
  `applyPanelSize` early-returns and there is no `ResizeObserver` (`main.ts:424-433`). A control that
  grew the row mid-session would silently clip the ten-row grid (ADR-0026) with no error.
- **Result pages only** — `list` and `split`. Not `detail` (nothing to narrow), not the command layer
  (the root groups by Extension source per ADR-0020, where a tag axis has no meaning).
- **No global shortcut.** Raycast binds ⌘P here; Moe's ⌘P is Browse on every surface
  (`keymap.rs:66`) and toggles the history card in the Side View (`chat.ts:883-888`). Keyboard access
  comes from a **"Filter by Tag" row in the ⌘K card** — Raycast's own `ActionPanel.Submenu` pattern —
  plus click and Tab. The retired `⌘⇧P` is **not** revived (ADR-0015 amendment).
- **The Filter Card anchors top-right under its trigger**, per the now-explicit rule that a card
  anchors near its trigger (ADR-0028 amendment). Built on `createCard` and registered in the layer
  stack.
- **AND with the query**, with two sub-rules: the value list is derived **before** the tag filter is
  applied, so values cannot disappear while typing and the active tag can never vanish from its own
  picker; and while an item is `pending` a filter change re-runs nothing — ADR-0038's "the page owns
  the input" guard covers the filter exactly as it covers the Input Bar.
- **Session-scoped per (Extension, command), never on disk, and it adds no Back layer.** Entering the
  list fresh via ⌘P/⌘N resets it, consistent with ADR-0038's "every entry clears it".

## Delivery

- `crates/moe-core/src/contract.rs` — `Item.tags` and the dimension declaration; `ui/src/types.ts` —
  the mirror.
- `ui/index.html`, `ui/src/main.ts` — the control as the input row's third child, the card, and the
  derive-before-filter bookkeeping.
- `ui/src/actions.ts` — a "Filter by Tag" `ActionSource` row for pages that declare a dimension.
- `ui/src/styles.css`, and `ui/src/layout.ts` if the control needs a constant.
- **First real consumer:** `Conversation.agentic: bool` exists in storage but never reaches the
  `Item` (`conversation.rs:7-15`) — chat history is the natural first dimension.

## Acceptance

- The control renders only on `list` / `split` result pages whose result declares a dimension, and
  the measured Input Bar height is byte-identical to today's with and without it.
- Choosing a value narrows the list and the label shows it; "All ⟨label⟩" restores everything; the
  active value carries a check.
- Values do not vanish while typing, and the active tag never disappears from its own picker.
- Changing the filter while an item is `pending` re-runs nothing and does not disturb the stream.
- Filter state survives leaving and re-entering the same command within a session, is not written to
  disk, and resets on ⌘P/⌘N entry.
- Esc / empty-⌫ closes the Filter Card before it backs out of the page; the ⌘K row opens it.
- No global binding was added or changed.
- `just lint`, `just test`, `just ui-build` green.

## Comments

- 2026-10-12 (agent): filed from the Raycast primitives grilling session; ADR-0043 records the
  decision and divergence #2 in `docs/research/raycast-ui-primitives.md` records the ⌘P cost (with
  ADR-0014's attribution corrected by amendment). Blocked by MOE-0041 and MOE-0042 so the Filter Card
  is built on the unified component and registers in the stack once, rather than becoming a sixth
  hand-rolled surface and a fourth place in the if-chain.
