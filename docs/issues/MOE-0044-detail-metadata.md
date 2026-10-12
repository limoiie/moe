# M9p: structured detail metadata in the split pane

- **ID**: MOE-0044
- **State**: done
- **Labels**: ready-for-agent, feature, ux
- **Created**: 2026-10-12
- **Updated**: 2026-10-12
- **Commit**: 8211e95
- **Blocked by**: MOE-0039 (done)
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
      all render, in the Extension's declared order. **Unticked: no Extension produces metadata yet**,
      so there is nothing on screen to check. See the comment below — the renderer is written and the
      first consumer is a decision this ticket deliberately did not take.
- [x] A metadata-only item (no Markdown) opens a pane rather than rendering nothing — `pageShapeOf`'s
      widened test, pinned in `layout.test.ts`.
- [x] `detailFull` still decides the `detail` shape — a multi-item result with metadata stays `split`
      (also `layout.test.ts`; this is the case that would quietly reverse ADR-0013).
- [x] A `tagList` is carried through the contract without loss: the Rust test asserts the whole shape
      (`kind`, `items`, camelCase `actionId` omitted when unset) even though its rendering waits for
      MOE-0048, and the mirror declares it.
- [x] Metadata colors resolve from ADR-0041's tokens — the same `inkClass` mapping the row accessories
      use. Not machine-checked; it is one map, shared.
- [x] 34 UI tests (was 30), 253 Rust tests, `tsc`, `vite build`, clippy green. **`just lint` is red on
      the pre-existing `cargo fmt` diff in `anthropic.rs`** — unchanged by this work.

## Comments

- 2026-10-12 (agent): filed from the Raycast primitives grilling session. Originally one ticket
  bundling both placements, clickable tags and the tag→filter rule. Split three ways: the seam is
  real, not cosmetic — MOE-0047 isolates the ADR-0018 shape change, and MOE-0048's dependency on
  MOE-0043 was previously an invisible coupling inside this ticket.
- 2026-10-12 (agent): blocker narrowed from `MOE-0039, MOE-0043` to `MOE-0039` only — the tag→filter
  rule that needed a declared dimension moved out to MOE-0048 with the `TagList` rendering.
- 2026-10-12 (agent): **landed** (8211e95). `Item.metadata` with all four field types, the `MetadataTag`
  struct (`text`/`icon`/`color`/`actionId`), the widened `pageShapeOf`, and the split pane's bottom
  block rendering `label` / `link` / `separator`. `layout.test.ts` is new and pins the shape rule,
  including the case that would quietly reverse ADR-0013 (a multi-item result with metadata stays
  `split`).

  Two implementation notes. A metadata **link is a button, not an `<a>`**: the panel is a webview, so a
  raw `href` would navigate Moe away from itself — it dispatches through `open_external`, which refuses
  anything but http(s) before spawning a process. And a **metadata value's color rides an inner span**,
  so the palette class wins by specificity rather than by where the rule sits in `styles.css`.

  **The gap this leaves:** nothing produces metadata yet, so the ticket's first acceptance box cannot be
  ticked and the work is invisible in the app. The model picker is the obvious candidate — its detail
  card already carries the same facts as bold prose (`catalog_facts`) — but moving them would change an
  existing surface, and it is genuinely a consumer decision rather than part of this data-model half.
  The natural home for it is **MOE-0048**, which needs a real `TagList` on a real page anyway.