# M9x: the metadata renderer gets a DOM test harness

- **ID**: MOE-0052
- **State**: done
- **Labels**: ready-for-agent, refactor
- **Created**: 2026-10-12
- **Updated**: 2026-10-12
- **Commit**: 0819c0c
- **Blocks**: the renderer tickets that would otherwise end in "not verified on screen"

Six tickets in this stretch ended with the same unticked box, and the shape of it never changed:
*nothing has rendered on screen*. The cause is structural rather than a shortage of diligence —
`main.ts` queries the DOM at module load, so no test can import it, and every renderer therefore lives
one layer below anything the runner (MOE-0050) can reach. The only verification available was a human
pressing keys, which is why the same caveat had to be written six times.

## Decision

- **A renderer worth testing lives in its own module and takes its side effects as arguments.** The
  metadata block moves to `metadata.ts` with `MetadataDeps`: the page's dimension values, `onFilter`,
  `onAction`, `onOpenLink`. `main.ts` passes the panel's real state in and keeps nothing of the block's
  internals; the block keeps no reference to the panel.
- **jsdom, per file rather than globally.** A `// @vitest-environment jsdom` docblock means only this
  file pays for an environment; the pure-rule suites (`filter`, `layers`, `accessories`, `dates`,
  `metadata`'s own rule) keep running in node. The whole file costs ~18ms.
- **The harness asserts what a DOM can honestly assert**: element kind, class, declaration order and
  dispatch. jsdom does not lay anything out, so row heights, column widths and the measured panel path
  stay hand-checked — written in the file rather than left implied.

## Delivery

- `ui/src/metadata.ts` — the block's renderer, moved and parameterised; `tagIntent` unchanged.
- `ui/src/main.ts` — loses five renderer functions and passes `metadataDeps` at the paint site.
- `ui/src/metadata-block.test.ts` — the new suite.
- `ui/package.json` — `jsdom` as a devDependency, the only new dependency.

## Acceptance

- [x] Five tests cover the claims that were previously hand-checked only: rows in the Extension's order;
      the palette color landing on the inner span (the reason `valueEl` has two spans at all); a `link`
      as a button that hands its target to the platform; a display-only tag as a `span` against a live
      one as a `tabindex="-1"` button; and one bad `actionId` leaving the row's other tags working.
- [x] `main.ts` keeps no copy of the block: the local `metadataValueEl` / `metadataRowEl` /
      `metadataTagEl` / `metadataTagListEl` / `metadataEl` are gone, so the two cannot drift.
- [x] No behavior change: 48 UI tests (was 43), `tsc`, `vite build`, and 253 Rust tests green.
- [x] The pattern is recorded as ADR-0007's amendment, so the next renderer follows it instead of
      rediscovering the problem.

## Comments

- 2026-10-12 (agent): filed and closed in one pass, from the "what's next" question after MOE-0051. The
  trigger was noticing that a DOM environment was *never configured* — not that tests were skipped.
  The two edits before this one went in backwards and were caught by the compiler and by `git status`;
  `main.ts` was restored from HEAD and redone, which is the reason the final diff only removes lines it
  was supposed to remove.
- 2026-10-12 (agent): **not verified on screen** — deliberately, and this ticket is the answer to that
  sentence rather than another instance of it. The five assertions above are all machine-checked now;
  what a pass would still add is the layout half (row height with three accessories, the column's
  width), which jsdom cannot see.
