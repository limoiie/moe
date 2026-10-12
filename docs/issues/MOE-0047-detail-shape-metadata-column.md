# M9s: the detail shape's metadata column

- **ID**: MOE-0047
- **State**: done
- **Labels**: ready-for-human, feature, ux
- **Created**: 2026-10-12
- **Updated**: 2026-10-12
- **Commit**: c8fb9cd (870a34a adds its producer)
- **Blocked by**: MOE-0044 (done)

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

- [x] With metadata present, the `detail` shape renders Markdown beside a right-hand column; with
      metadata absent the pane class, `paintDetail`'s branch and the CSS are all untouched — the
      additive claim is structural, not a promise. **Verified:** the reporter's pass saw provider and
      model in the column beside the model card's Markdown.
- [x] The column width lives in `layout.ts` (`METADATA_COLUMN_WIDTH`), published as
      `--moe-metadata-col-w` at boot exactly like `--moe-row-h`; no class string composes it.
- [x] Both placements come from one renderer: `metadataEl` is called with the same arguments and only
      its container differs, so a divergence would have to be written deliberately.
- [x] Long Markdown scrolls in its own region: the pane drops to `DETAIL_COLUMNS_CLASS`
      (`overflow-hidden`) only when a column is coming, and the prose region inside takes the scroll.
- [x] 40 UI tests, 253 Rust tests, `tsc`, `vite build`, clippy green. **`just lint` is red on the
      pre-existing `cargo fmt` diff in `anthropic.rs`** — unchanged by this work.

## Comments

- 2026-10-12 (agent): split out of MOE-0044 during the granularity pass. Labelled `ready-for-human`
  rather than `ready-for-agent`: it is the one ticket that changes an existing layout contract, and
  ADR-0018's shape definition needs a human to confirm the amendment reads the way the reviewer
  expects before the code lands.
- 2026-10-12 (agent): ADR-0018 should carry an amendment for this once the shape ships — the ADR
  currently says the detail shape fills the panel, and that stops being unconditionally true.
- 2026-10-12 (agent): **landed** (c8fb9cd, producer 870a34a). ADR-0018 carries the amendment this
  ticket asked for. `paintDetail` now takes one params object — the item is needed in *both*
  placements, while the header row stays the split shape's alone, which is the assumption the earlier
  "metadata renders exactly when a header is present" shortcut rested on and could not survive this
  ticket. The two placements' remaining differences (the split block's top rule and margin) live in CSS
  beside the block rather than in the renderer.

  **The producer is the picker's apply-confirmation card** — the one surface that is already
  `detailFull` with a single item, and whose facts were prose only. Provider and model now ride the
  column. Without it this ticket, like MOE-0044, would have been unverifiable by eye.