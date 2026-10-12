# M9p: the detail pane grows structured metadata, in Raycast's two placements

- **ID**: MOE-0044
- **State**: todo
- **Labels**: ready-for-agent, feature, ux
- **Created**: 2026-10-12
- **Updated**: 2026-10-12
- **Blocked by**: MOE-0039, MOE-0043
- **Blocks**: MOE-0045

`Item.detail` is Markdown and nothing else (`contract.rs:63`), so structured facts about a result — a
status, a link, a set of tags — must be flattened into prose or dropped. ADR-0042 adopts Raycast's
metadata block (`Label`, `Link`, `TagList`, `Separator`) and **both** of its placements: at the
bottom of a `List.Item.Detail`, and in the right-hand column of a standalone `Detail`. This is the
first change to ADR-0018's `detail` shape since it was decided.

## Decision

ADR-0042 holds the reasoning. The load-bearing points:

- **`Item.metadata`** — `label` (title, optional icon, optional text in a palette color), `link`
  (title, text, target), `tagList` (title, plus items of text/icon/color), `separator`. Order is the
  Extension's; the platform neither sorts nor groups.
- **Two placements, exactly Raycast's:** bottom of the detail pane in `split`, right-hand column in
  `detail`. Not a compromise between two Moe options — Raycast's own two components differ this way,
  and the difference is legible (a pane sharing the window with a list puts metadata last; a page
  owning the window puts it to the side).
- **The `detail` shape is full-width only when metadata is absent.** The column's width joins the
  other shape constants in `ui/src/layout.ts`, so no renderer composes widths (ADR-0018's rule).
- **Metadata alone opens a pane:** `pageShapeOf`'s test widens from
  `item.detail != null || item.pending === true` (`layout.ts:98-107`) to include metadata. A
  metadata-only item has something to show, and silently rendering no pane would be a worse surprise
  than a pane with no prose.
- **ADR-0013 is not reversed.** `detailFull` remains the Extension's declaration; this widens only
  *whether a pane exists*, not which shape a result takes.
- **A tag may be clickable, by reference** — an optional `actionId` that must resolve to one of the
  item's declared `Action`s, dispatched through `run_item_action` (`contract.rs:331-338`). No
  `actionId` means display-only; an *unresolvable* id also renders display-only rather than erroring,
  so the row's other tags keep working.
- **A tag matching a declared filter dimension sets the Filter Control** instead of running an action
  (ADR-0043). This closes the loop between `Item.tags` and the pane's `TagList`: see a tag, click it,
  the list narrows. A Moe addition with no Raycast counterpart — divergence #7.
- **One palette:** ADR-0041's nine tokens. No second color vocabulary for the detail pane.

## Delivery

- `crates/moe-core/src/contract.rs` — the metadata enum and the `Item` field; `ui/src/types.ts` — the
  mirror.
- `ui/src/main.ts` — the detail renderer, both placements, and the tag dispatch path.
- `ui/src/layout.ts` — the metadata column constant and the widened `hasDetail` test.
- `ui/src/styles.css` — the metadata block's typography and separators.
- Meets MOE-0039's counterpart here: ADR-0041 hides row accessories in `split` *because* the same
  information belongs in this block's `TagList`.

## Acceptance

- `split`: metadata renders under the Markdown, at the pane's bottom. `detail`: metadata renders in a
  right-hand column; with no metadata the shape is pixel-identical to today's.
- A metadata-only item (no Markdown) opens a pane rather than rendering nothing.
- `detailFull` still decides the `detail` shape — a multi-item result with metadata stays `split`.
- A tag with a valid `actionId` runs that action; an invalid one renders display-only and the same
  list's other tags still work.
- Clicking a tag whose text matches a declared filter dimension narrows the list and updates the
  Filter Control's label.
- The column width lives in `layout.ts`, not in a renderer class string.
- `just lint`, `just test`, `just ui-build` green.

## Comments

- 2026-10-12 (agent): filed from the Raycast primitives grilling session; ADR-0042 records the
  decision and amends ADR-0018's `detail` shape. Blocked by MOE-0039 (the palette and the
  accessories-hide-in-split rule it completes) and MOE-0043 (the tag→filter rule needs a declared
  dimension to match against). If filters are ever removed, that rule goes with them — ADR-0042's
  Cost names the coupling.
