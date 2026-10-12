# M9z: the list's header and empty view join the harness

- **ID**: MOE-0054
- **State**: done
- **Labels**: ready-for-agent, refactor
- **Created**: 2026-10-12
- **Updated**: 2026-10-12
- **Commit**: a117d00
- **Blocked by**: MOE-0053 (the module it joins)

The last two renderers left in `main.ts`, named when MOE-0053 closed: `sectionHeaderEl` and
`emptyViewEl`. Between them they carry two rules that were hand-checked only — the header's fixed
height, which ADR-0026's ten-row budget counts, and the empty view's non-focusability, which is the
whole reason ADR-0043 replaced the notice row.

## Decision

- **Both move into `rows.ts`**, which is why that module keeps its name: everything in it produces one of
  the `<li>`s the result list is made of — a row, a section header, an empty view — plus the accessory
  elements a row can carry.
- **Neither takes deps.** Unlike the row, neither reaches for panel state: the header is a title and a
  constant, the empty view is an `EmptyView`. They needed only to leave the file a test cannot import.
- **The *decision* to show the empty view stays in `main.ts`**, where it belongs: the platform suppresses
  it while a query-free load is in flight, so a first paint cannot flash "no results". The renderer draws
  what it is given.

## Delivery

- `ui/src/rows.ts` — the two renderers.
- `ui/src/main.ts` — loses them and the now-unused `SECTION_HEADER_HEIGHT` import; keeps their call sites.
- `ui/src/rows.test.ts` — five more tests.

## Acceptance

- [x] The header's height is asserted against `SECTION_HEADER_HEIGHT` rather than a literal, so the test
      fails if the budget constant and the renderer ever disagree.
- [x] The header is asserted to be a header and not a row: no `moe-row` class (so styles.css's hover rule
      cannot match it), no focus mark, and a press that reaches nothing.
- [x] The empty view renders title, description and icon as declared; omits the description block when
      there is none; and is non-focusable by construction — `role="presentation"`, no row class, no focus
      mark, no button, and a press that reaches nothing.
- [x] No behavior change: 60 UI tests (was 55), `tsc`, `vite build`, 253 Rust tests green.
- [x] `main.ts`'s diff is the removals only — 40 lines out, nothing else touched.

## Comments

- 2026-10-12 (agent): filed and closed in one pass. Two of the three edits to `main.ts` for this ticket
  went in with their arguments the wrong way round and one of them clobbered unrelated code
  (`actionsCard`'s declaration); `tsc` and `git status` caught both, and `main.ts` was restored from HEAD
  and redone one exact step at a time. The final diff is the removals only. Recorded because the failure
  mode is worth naming: a fuzzy edit that reports success is not evidence that it matched what you meant.
- 2026-10-12 (agent): **the sweep is complete.** Every renderer in the panel now lives in a module a test
  can reach — `metadata.ts` (the detail pane's block), `rows.ts` (the list), `card.ts` (the floating
  cards, already modular), `kbd.ts`, `composer.ts`, `icons.ts`. What remains hand-checked is the layout
  half only: row height, column width, the measured input bar and the panel's size — none of which jsdom
  can see, and all of which the measured-height path self-corrects at startup.
