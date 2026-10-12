# M9s: the detail shape's metadata column

- **ID**: MOE-0047
- **State**: todo
- **Labels**: ready-for-human, feature, ux
- **Created**: 2026-10-12
- **Updated**: 2026-10-12
- **Blocked by**: MOE-0044

Raycast places `Detail.Metadata` in the **right-hand column** of a standalone `Detail`, and that is
the placement ADR-0042 adopts. It is the risky half of the metadata work, which is why it is not
bundled with the split pane's bottom block: **ADR-0018 defines the `detail` shape as 详情占满** (the
detail fills the panel), and a right-hand column means it no longer does when metadata is present.

This is the first change to ADR-0018's `detail` shape since it was decided, and the only ticket in
this set that alters an existing layout contract rather than adding to it.

## Decision

- **The `detail` shape becomes Markdown plus a right-hand column** when metadata is present, and stays
  byte-identical to today when it is not. One shape, two states, decided by data.
- **The column's width joins the other shape constants in `ui/src/layout.ts`**, so no renderer composes
  widths — ADR-0018's rule that the platform decides layout and an Extension never lays itself out.
- **Purely additive to MOE-0044's data model**: the enum already carries everything; this ticket adds
  the second container and nothing else.

## Delivery

- `ui/src/layout.ts` — the metadata column constant.
- `ui/src/main.ts` — the `detail` shape's container, and the shared metadata block reused from
  MOE-0044 in its second placement.
- `ui/src/styles.css`.

## Acceptance

- [ ] With metadata present, the `detail` shape renders Markdown beside a right-hand metadata column;
      with metadata absent it is pixel-identical to today.
- [ ] The column width lives in `layout.ts`, not in a renderer class string.
- [ ] The same metadata renders correctly in both placements (bottom in `split`, right in `detail`)
      from one shared renderer — not two divergent implementations.
- [ ] Long Markdown still scrolls within its own region; the column does not scroll with it.
- [ ] `just ui-build` green.

## Comments

- 2026-10-12 (agent): split out of MOE-0044 during the granularity pass. Labelled `ready-for-human`
  rather than `ready-for-agent`: it is the one ticket that changes an existing layout contract, and
  ADR-0018's shape definition needs a human to confirm the amendment reads the way the reviewer
  expects before the code lands.
- 2026-10-12 (agent): ADR-0018 should carry an amendment for this once the shape ships — the ADR
  currently says the detail shape fills the panel, and that stops being unconditionally true.