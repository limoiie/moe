# M9y: the row renderer gets the same DOM harness

- **ID**: MOE-0053
- **State**: done
- **Labels**: ready-for-agent, refactor
- **Created**: 2026-10-12
- **Updated**: 2026-10-12
- **Commit**: d79f76b
- **Blocked by**: MOE-0052 (the seam it reuses)

MOE-0052 moved the metadata block into a module with its side effects injected and set that up as the
rule (ADR-0007's amendment). This is the second renderer to follow it, and the one worth following it
most: the result row is the panel's highest-traffic renderer, it carries four ADRs' worth of structure
(ADR-0018's shapes, ADR-0030's trailing order, ADR-0031's Kbd reveal, ADR-0041's accessories), and every
claim about it — including one it shared with MOE-0039 — was hand-checked.

## Decision

- **`rowEl` and `accessoryEl` move to `ui/src/rows.ts`**, with `RowDeps`: one clock reading per render,
  and a focus sink taking the *flat* index the caller already passes.
- **`Row` moves with them** — it is the renderer's input type, and leaving it behind would have split one
  contract across two files.
- **The clock becomes a dependency rather than a call inside the renderer.** `rowEl` used to call
  `Date.now()` per row; it now takes `deps.nowUnix`, so every row of a list agrees on "now" and a test
  can pin a date accessory's text. A small correctness improvement that fell out of the seam, not a
  change to what renders.

## Delivery

- `ui/src/rows.ts` — the renderer, the `Row` contract, `RowDeps`.
- `ui/src/main.ts` — loses `Row`, `accessoryEl` and `rowEl`; builds `rowDeps` once per render and passes
  it at both call sites (the command layer and the result list).
- `ui/src/rows.test.ts` — the new suite.

## Acceptance

- [x] Seven tests cover what was hand-checked: the focus mark and the reported index; a fallback icon
      dimmed against a declared one; the subtitle inside the title cell (which is why the *title* is what
      truncates when accessories crowd it); accessories between the title and the Kbd/badge; each
      accessory kind's own shape with the palette on the ink class and no class when no color is named;
      the Kbd revealing only where the page asks (ADR-0031); and a bare-title row.
- [x] `main.ts` keeps no copy: the renderer exists once, so the two cannot drift.
- [x] No visual change: 55 UI tests (was 48), `tsc`, `vite build`, 253 Rust tests green.
- [x] The row's **40px height and the title's truncation stay hand-checked** — jsdom lays nothing out, and
      the ticket says so rather than implying coverage it does not have.

## Comments

- 2026-10-12 (agent): filed and closed in one pass, continuing the thread MOE-0052 opened. Two assertions
  needed fixing on the first run, both test-side rather than code-side: `Element.className` is an
  `SVGAnimatedString` on SVG elements (so `getAttribute("class")` is what a class-list read wants), and
  `kbdEl` returns a `.moe-keys` wrapper — the row styles the wrapper, not the `.moe-kbd` blocks inside it.
  Recorded because the second is a real trap for the next reader of this renderer.
- 2026-10-12 (agent): **what is left hand-checked across the panel now:** the layout half everywhere (row
  height, column width, the measured input bar), plus the two renderers that never left `main.ts` —
  `sectionHeaderEl` and `emptyViewEl`. Both are small and would follow the same rule if a third surface
  ever needed them.
