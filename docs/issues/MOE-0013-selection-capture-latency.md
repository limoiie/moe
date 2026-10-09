# M7u: the selection capture stops stalling the summon — double-tap ⌘ no longer pays ~250 ms before the panel shows

- **ID**: MOE-0013
- **State**: done
- **Labels**: bug, perf
- **Created**: 2026-10-09
- **Updated**: 2026-10-09
- **Assignee**: agent
- **Commit**: cfb690a

User report: "I can feel very slight delay before moe pops up after I pressed cmd+cmd every time."

## Problem

Summoning showed the panel after ~265–350 ms of pre-show work, even with nothing selected: measured
with temporary stage timings in `show_panel_blocking` (since removed), the summon path spent
252–289 ms inside `read_selection` alone, while placement, shadow refresh and the AppKit show
together stayed under ~60 ms.

The capture is synchronous by design — the selection must be read before the panel becomes key
(ADR-0021), and `show_panel_blocking` (`crates/moe-app/src/main.rs`) runs it on the main thread right
before `show_and_make_key()`. `HybridTextTarget::read_selection`
(`crates/moe-platform/src/text_target.rs`) falls back to a synthesized ⌘C whenever AX hands over no
text — including when AX *reports no selection* (`AXSelectedText` exists but is empty) or cannot
answer at all (no AX focus) — and that fallback cost two fixed sleeps on the main thread:
`MacClipboard::copy` 100 ms + `MacClipboard::restore` 150 ms
(`crates/moe-platform/src/mac_text.rs`). In the common "nothing selected" case the ⌘C round trip
also fired a stray copy into the host app and briefly swapped the clipboard, only to conclude
"no selection" after all.

## Root cause

The strategy layer's `if let Ok(Some(text)) = ax.selected_text()` cannot distinguish "no selection"
(cursor / no AX focus) from "a selection exists whose text the app doesn't expose" — both are
`Ok(None)` — so every summon from a non-exposing app ran the fallback, and the fallback was
clamped to fixed sleeps instead of observing when the pasteboard write actually landed.

## Delivery

- `crates/moe-platform/src/text_target.rs`: the fallback is gated — an explicit empty AX range
  (`selected_text_range() == Some((_, length))` with `length <= 0`) is a cursor and nothing more:
  return `Ok(None)` without touching the clipboard. A *missing* range keeps the fallback, which is
  the "selection exists but its text is not exposed" case it exists for (e.g. Zed does not expose
  `AXSelectedText`). Trait docs now say where the waits belong: `paste`/`copy` wait for the target
  app, `restore` is immediate. Tests: `empty_ax_range_skips_the_clipboard_fallback` (red first,
  then green) and `nonempty_ax_range_still_falls_back_to_copy`.
- `crates/moe-platform/src/mac_text.rs`: `copy()` polls `NSPasteboard.changeCount` (5 ms interval,
  100 ms budget) and returns as soon as the app's write lands — a real selection costs a poll or
  two. The 150 ms settle moved from `restore()` to `paste()`, where it is load-bearing (the
  write-back paste path keeps its timing; the read path no longer pays it at all).
- `crates/moe-platform/src/mac_text.rs` (follow-up, same day): the element resolution behind both
  reads gains a second source — when the system-wide element reports no focus, `focused_element()`
  asks the frontmost application's own AX tree (`AXUIElementCreateApplication(pid)` →
  `AXFocusedUIElement`) before conceding. The reported app class ("no system-wide AX focus") only
  surfaces focus there; every case that resolves is one where the empty-range gate can skip the ⌘C
  fallback instead of paying its budget. Two stderr lines make the outcome visible per summon:
  "AX focus resolved via the frontmost app's own tree (pid N)" / "no AX focus from the system-wide
  element nor the frontmost app (pid N)". Platform glue only — the strategy layer and its tests
  are unchanged, the suite stays green (49/49). Retired later the same day after live measurement:
  the probe's first AX message to an app costs ~170–190 ms (connection setup, not capped by
  `AXUIElementSetMessagingTimeout`) and resolved nothing usable on the actual apps (see the last
  Comments); the read path is back to system-wide focus only, now with a 50 ms per-message ceiling
  and a 100 ms wall-clock budget for the text walk.
- `crates/moe-platform/src/lib.rs`, `text_target.rs`, `mac_text.rs` + `crates/moe-app/src/main.rs`
  (the structural fix, same day): the fallback's observation leaves the pre-show path.
  `TextTarget::read_selection` becomes `capture_selection` returning `Capture::Done` or a Send
  `Capture::Pending`; the strategy fires the ⌘C and waits only a 20 ms grace (100 ms total budget),
  so a copy that lands within it completes exactly as before, while anything slower hands the tail
  to the caller. `MacClipboard` exposes begin/landed/changed-since-copied over the pasteboard's
  changeCount; the snapshot is now owned bytes (`NSData` is not Send). `show_panel_blocking`
  stores the files immediately and runs the tail on a named `moe-selection-capture` worker; late
  text is published only if no newer summon intervened (`selection_generation`). Two disciplines
  fall out: a never-landing copy no longer rewrites the pasteboard at all, and a write that
  arrives after ours (user copied from the panel) skips the restore. Tests: 52/52.

## Measured effect

Same scenario and method as the baseline (AX-opaque frontmost app, nothing selected): pre-show
265–348 ms → 116–179 ms, `read_selection` 252–289 ms → 108–141 ms. In apps that expose an empty
AX range the fallback is skipped entirely (unit-verified).

Live verification (same day; live smoke against the two apps this issue is about — modero and Zed):
with a real 35-char selection in Zed the whole capture took 22.6 ms — the ⌘C landed within the
grace and the clipboard was restored — and with nothing selected the grace expired at ~25 ms
(pending), the finisher concluding `none` ~80 ms later with the clipboard untouched. The split
behaves as designed. Two hazards were measured along the way and are recorded in the last Comments:
the app-tree focus probe cost ~170–190 ms per first AX message to an app (uncapped by the
messaging timeout) and was retired; the clipboard snapshot can block ~530 ms once per lazily-served
clipboard content (pre-existing, fallback path only, left in Remaining).

## Remaining

- E2E re-confirmation on the user's own summons: with nothing selected expect `selection pending …
  capture T ms` + `selection late: none` (T ~25 ms when the clipboard reads cheaply), with a
  selection a pre-show capture (`capture ≤ ~25 ms`) or a late one — the old full-budget stall is
  gone either way.
- The caret-exposing gate path still lacks E2E cover (unit-verified only).
- Clipboard snapshot materialization: when the clipboard's data is served lazily by its owner, the
  eager `snapshot()` can block hundreds of ms (measured ~530 ms; one-off per clipboard content
  because the restore materializes it; fallback path only). A cheaper snapshot (pasteboard items
  instead of extracted data, or bounding what is restorable) is a candidate follow-up.
- One uncapped first AX message to an app remains possible (~170–190 ms if its AX connection is
  cold and the app slow); the app-tree probe that hit this is gone and the remaining system-wide
  connection measured fast (tens of µs).
- Bounded by design: a command invoked within ~100 ms of the panel showing may see the selection
  before its verdict lands (`text: None`).

## Acceptance

- Summoning with a caret / nothing selected in an app exposing `AXSelectedTextRange`: no ⌘C fired,
  no clipboard churn, no 100 ms+ wait before the panel shows.
- Summoning with a real selection in an app where `AXSelectedText` is missing (range length > 0):
  the selection is still captured through the fallback.
- Summoning with a slow or stalled AX server: reads are bounded (50 ms per message, 100 ms per
  walk) and degrade to the clipboard fallback instead of stalling the summon.
- Summoning in a fully AX-opaque app with nothing selected: the panel shows after the ~20 ms grace,
  not the full budget; the finisher concludes "none" without touching the clipboard.
- A selection in a fully AX-opaque app is captured within the grace (fast copy) or by the late
  finisher (slow copy); a clipboard write that happens after ours is never clobbered by the restore.
- `cargo test -p moe-platform` green (52).

## Comments

- 2026-10-09 (agent): filed from the user's latency report; the baseline was measured with temporary
  `[DEBUG-lat]` stage timings in `show_panel_blocking` across six summons (pre-show 265–348 ms,
  `read_selection` 252–289 ms; the AX probe returned `kAXErrorNoValue` for the frontmost app), then
  the instrumentation was removed.
- 2026-10-09 (agent): fix applied — currently uncommitted in the working tree (the user commits on
  main). `cargo test -p moe-platform` 49/49 green, and again after the move; the effect re-measured
  with the same instrumentation: pre-show 265–348 → 116–179 ms. End-to-end cover for the gate path
  (a caret-exposing frontmost app) is still pending.
- 2026-10-09 (agent): follow-up on the same working tree — the Remaining item landed: focus
  resolution now falls back to the frontmost app's own AX tree before the ⌘C path (`mac_text.rs`).
  Mechanism verified live against a locked session (no system-wide focus → app-tree resolution →
  `(0, 0)` range read → fallback skipped); 49/49 tests green, `cargo fmt --check` clean.
  E2E on the reported apps is pending (the new stderr lines tell per summon). The ADR-0002
  amendment for the narrowed fallback condition is still unwritten.
- 2026-10-09 (agent): first real summon on the probe build (user's log, an app whose AX surface
  matches Zed's): system-wide focus resolved (no probe line), `AXSelectedText` failed with -25205 at
  levels 0–1 and the range read with -25205 at the focused element, `selection 0 chars` — the
  fully-opaque path: no gate signal, so the ⌘C fallback ran and concluded "nothing selected"
  (full observation budget). Live AX dump of Zed: `AXWindow` → `AXApplication` only, no text
  attributes anywhere — for Zed neither the gate nor the app-tree probe can ever engage; the ⌘C
  route is the only way to capture its selection. The summon log line now carries the missing
  telemetry — `moe: selection N chars, F Finder files, capture T ms (frontmost <bundle>)` — so the
  felt delay is measurable per summon. Next decision: move the fallback's observation off the
  pre-show path, or trim the budget after a landing-latency calibration (see Remaining).
- 2026-10-09 (agent): picked the structural lever (the user's choice): the fallback's observation now
  leaves the pre-show path. `TextTarget::capture_selection` returns `Done` or a Send `Pending(...)`;
  the strategy owns a 20 ms grace inside the 100 ms budget; `MacClipboard` exposes
  begin/landed/changed-since-copied over `changeCount`; `show_panel_blocking` publishes the files at
  once and finishes the tail on a `moe-selection-capture` worker (late text guarded by a per-summon
  generation). The snapshot is owned bytes (`NSData` is not Send), and the restore happens only when
  the copy landed and the pasteboard hasn't moved on — the never-landed case no longer rewrites the
  clipboard at all. 52/52 tests, fmt + workspace checks clean; ADR-0002 amended. E2E: restart and one
  summon — the "capture T ms" field is the number that used to be 108–141 ms in the opaque case.
- 2026-10-09 (agent): live verification pass (throwaway smoke against the real clipboard; since
  removed) found three things and settled two. (1) The split works on the real apps: Zed selection
  captured in 22.6 ms through the grace path, nothing-selected going pending at ~25 ms and finishing
  churn-free; strategy stage timings 0.5–50 ms for text, 0.1–20 ms for range, 0.6 ms prep. (2) The
  app-tree focus probe (delivered earlier the same day on the "no system-wide AX focus" hypothesis)
  was pure cost on the actual apps: first AX message to the frontmost app ~170–190 ms (connection
  setup, intermittent, NOT capped by `AXUIElementSetMessagingTimeout` — tested with a 1 ms timeout),
  and it never resolved a usable element on modero or Zed (Zed: `AXWindow` only; modero: nothing).
  Retired again — read path is system-wide only + a 50 ms per-message ceiling + a 100 ms walk budget
  that lets the ⌘C fallback decide instead of stalling. (3) `snapshot()` can block ~530 ms once per
  clipboard content when the pasteboard data is served lazily by its owner (the user's clipboard had
  4 types) — pre-existing (the old code materialized via `dataForType` too), fallback-path only, left
  in Remaining. 52/52 tests, fmt + workspace checks clean; moe-app rebuilt — restart to pick it up.
- 2026-10-09 (agent): committed in `cfb690a` on the user's request. Workspace verification before
  the commit: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings` and
  `cargo test --workspace` (159) green after two trivial lint cleanups folded into the commit (a
  `-> ()` return on the text_target test stub, a collapsible `if` in the finisher). E2E
  re-confirmation on the user's own summons is still pending (see Remaining).