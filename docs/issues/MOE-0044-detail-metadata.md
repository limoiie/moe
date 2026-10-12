# M9p: structured detail metadata in the split pane

- **ID**: MOE-0044
- **State**: todo
- **Labels**: ready-for-agent, feature, ux
- **Created**: 2026-10-12
- **Updated**: 2026-10-12
- **Blocked by**: MOE-0039
- **Blocks**: MOE-0047, MOE-0048

`Item.detail` is Markdown and nothing else, so structured facts about a result — a status, a link —
must be flattened into prose or dropped. ADR-0042 adopts Raycast's metadata block; this ticket lands
the **data model and the split pane's bottom placement**, which is the additive half.

The other two halves are separate tickets because they carry different risk: the `detail` shape's
right-hand column **changes ADR-0018's shape** and deserves isolated review (**MOE-0047**), and the
`TagList`'s clickable tags plus the tag→filter rule need a declared filter dimension and so depend on
MOE-0043 (**MOE-0048**).

## Decision

ADR-0042 holds the reasoning. The load-bearing points:

- **`Item.metadata`** — `label` (title, optional icon, optional text in a palette color), `link`
  (title, text, target), `tagList` (title, plus items of text/icon/color), `separator`. Order is the
  Extension's; the platform neither sorts nor groups. **The whole enum lands here** so the rendering
  tickets only grow renderers; only the `tagList` *rendering* is deferred to MOE-0048.
- **Placement here is the split pane's bottom**, under the Markdown — Raycast's own placement for
  `List.Item.Detail.Metadata`. The `detail` shape's right-hand column is MOE-0047.
- **Metadata alone opens a pane:** `pageShapeOf`'s test widens from `item.detail != null ||
  item.pending === true` (`pageShapeOf`, `ui/src/layout.ts`) to include metadata. A metadata-only item
  has something to show, and silently rendering no pane would be a worse surprise than a pane with no
  prose.
- **ADR-0013 is not reversed.** `detailFull` remains the Extension's declaration; this widens only
  *whether a pane exists*, not which shape a result takes.
- **The palette is ADR-0041's** nine tokens. No second color vocabulary for the detail pane.

## Delivery

- `crates/moe-core/src/contract.rs` — the metadata enum and the `Item` field; the shared mirror in
  `ui/src/types.ts`.
- `ui/src/layout.ts` — the widened `pageShapeOf` test.
- `ui/src/main.ts` — the detail renderer's bottom block, for `label` / `link` / `separator`.
- `ui/src/styles.css` — the block's typography and separators.

## Acceptance

- [ ] In `split`, metadata renders under the Markdown at the pane's bottom; label, link and separator
      all render, in the Extension's declared order.
- [ ] A metadata-only item (no Markdown) opens a pane rather than rendering nothing.
- [ ] `detailFull` still decides the `detail` shape — a multi-item result with metadata stays `split`.
- [ ] A `tagList` in the data is carried through the contract and the mirror without loss, even
      though its rendering arrives in MOE-0048.
- [ ] Metadata colors resolve from ADR-0041's tokens.
- [ ] `just lint`, `just test`, `just ui-build` green.

## Comments

- 2026-10-12 (agent): filed from the Raycast primitives grilling session. Originally one ticket
  bundling both placements, clickable tags and the tag→filter rule. Split three ways: the seam is
  real, not cosmetic — MOE-0047 isolates the ADR-0018 shape change, and MOE-0048's dependency on
  MOE-0043 was previously an invisible coupling inside this ticket.
- 2026-10-12 (agent): blocker narrowed from `MOE-0039, MOE-0043` to `MOE-0039` only — the tag→filter
  rule that needed a declared dimension moved out to MOE-0048 with the `TagList` rendering.