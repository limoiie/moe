# M10a: the loading hairline costs no layout and stops flickering

- **ID**: MOE-0055
- **State**: done
- **Labels**: ready-for-agent, bug, ux
- **Created**: 2026-10-12
- **Updated**: 2026-10-12
- **Commit**: 763ec03

Reported from use: the palette's loading hairline (ADR-0043) "flickers when I tap, as I input each char
it will show up and disappear immediately. What is worse, it takes space, so the list under it will up
then down very frequently."

Two independent causes, and each needed its own fix.

## Decision

- **It moved the list because it was a flex sibling.** The bar sat between the input row and the list, so
  showing it added its height to the column. It now lives *inside* the input row, positioned out of flow,
  so it costs no layout at all. That also keeps `measuredPanelHeight` honest: the row's rect ignores
  absolutely positioned children, so the ten-row grid's arithmetic is untouched.
- **It flashed because the flag tracked the load exactly.** The palette's search is local and lands in
  milliseconds, so the bar appeared and vanished within a frame. It now waits `delayMs` before showing and
  stays `minVisibleMs` once up: a fast search never shows it, a slow one gets a bar instead of a blink.
- **`searchLoading` itself stays immediate.** The Empty View's no-flash suppression reads it (ADR-0043's
  rule: never show "no results" while a query-free load is in flight), and that rule is about a load being
  in flight — not about the hairline being up. Delaying the flag would have re-opened exactly the flash
  that rule exists to prevent, on the first paint.
- **The timing is a module, not a closure in `main.ts`** (`ui/src/loading.ts`, per ADR-0007's amendment),
  because it has real cases: the burst case is the interesting one — typing starts a re-run before the
  previous one settles, and that must stay one bar rather than two.

## Delivery

- `ui/src/loading.ts` + `ui/src/loading.test.ts` — the timing, six tests with fake timers.
- `ui/index.html` — the bar moves inside `#query-row`, which gains `relative`.
- `ui/src/styles.css` — `position: absolute` at the row's bottom edge.
- `ui/src/main.ts` — `setSearchLoading` now drives both the immediate flag and the delayed hairline.

## Acceptance

- [x] A load shorter than the delay never shows the bar (the common case).
- [x] A load that outlasts the delay shows it once and hides it after the minimum, not before.
- [x] A burst of overlapping loads is one bar, not two — asserted with fake timers.
- [x] Repeated `set(true)`/`set(false)` calls produce exactly one show and one hide.
- [x] The bar changes no layout: it is absolutely positioned inside the input row, and the row's measured
      rect cannot see it.
- [x] 66 UI tests (was 60), `tsc`, `vite build` green.

## Comments

- 2026-10-12 (agent): filed from the reporter's use, fixed and closed in one pass. The layout half is the
  one worth remembering: a 1px element is still an element, and at typing speed a 1px shift reads as a
  shudder rather than as a shift.
