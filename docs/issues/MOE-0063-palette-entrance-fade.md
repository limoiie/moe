# M10i: the palette's entrance is a short, owned fade

- **ID**: MOE-0063
- **State**: done
- **Labels**: ready-for-agent, ux
- **Created**: 2026-10-15
- **Updated**: 2026-10-15
- **Commit**: 58435a4
- **Assignee**: agent

Asked from a session: "what is the open animation when I launch the moe's palette? what is the
duration? design a more clean and more short animation for it as it's the most frequent views that
user can feel and see."

## Diagnosis

- **The repo authored no entrance.** `show_panel_blocking` orders the panel front
  (`NSPanel::show_and_make_key`, i.e. `orderFrontRegardless` + `makeKeyWindow`); the view layer does
  nothing on show — `styles.css` has no window-level transition and `main.ts` only refreshes the
  banner and cancels attachment input on focus. So the motion the user sees was **AppKit's inferred
  orderFront animation** (`NSWindowAnimationBehaviorDefault`, NSWindow.h: "Let AppKit infer animation
  behavior for this window"). Its curve and duration are system-owned: not a constant in this repo,
  not tunable, and it does not follow `prefers-reduced-motion` (the app's own reduced-motion rule
  collapses CSS animations only).
- **The one surface where it matters most is the one we did not own.** The palette is summoned
  constantly (double-tap ⌘), so its entrance is the animation the user feels most. A system fade of
  indeterminate length is the wrong instrument.

## Design

- **Suppress the system animation for the panel** (`moe-platform::mac::suppress_window_animation` →
  `setAnimationBehavior: NSWindowAnimationBehaviorNone = 2`, set once at the NSPanel conversion). The
  panel keeps its cached native shadow — `refresh_window_shadow` still runs before every show.
- **Own the entrance, and keep it short.** One CSS transition on the card: **opacity, 20ms,
  `ease-out`** (`.moe-panel-entrance` in `styles.css`). No scale, no translate, no bounce: at the
  palette's frequency the fade should read as "already there, now solid", and transform-free means
  the compositor does the work and the cached window shadow cannot mismatch a scaled card. The
  duration was first set to 100ms and then dialed to **20ms** at the owner's request to feel the
  floor — 20ms is under one frame at 60 Hz, so the entrance is deliberately at the edge of
  perceptible (a hairline blend rather than a cut).
- **Arm while hidden, so there is no opaque flash.** A CSS transition needs the card to be at
  opacity 0 *before* the window is ordered front; the webview is hidden and re-shown, never reloaded,
  so the backend reports visibility (`panel-visibility`) and the view layer toggles
  `.moe-panel-dormant` (opacity 0, `transition: none`) on every hide and at boot. `panel-shown`
  removes it and the 20ms fade plays over an already-transparent first frame.
- **Reduced motion is automatic**: the existing global `prefers-reduced-motion` rule collapses the
  transition to ~0, and with AppKit's animation suppressed the palette then appears instantly.
- **Out of scope**: the Side View (`chat`) keeps the system animation — it is opened rarely and its
  own material is native.

## Delivery

- `crates/moe-platform/src/mac.rs`: `suppress_window_animation` (`setAnimationBehavior: 2`).
- `crates/moe-app/src/main.rs`: call it in the panel's NSPanel conversion; `emit_panel_visibility`
  emitted after showing (`show_panel_blocking`) and after hiding (`hide_panel_blocking`).
- `ui/src/styles.css`: `.moe-panel-entrance` / `.moe-panel-entrance.moe-panel-dormant`.
- `ui/src/main.ts`: arm at boot, toggle on `panel-visibility`.

## Acceptance

- `cargo clippy -p moe-platform -p moe-app --all-targets -- -D warnings` and `cargo fmt --all --check`
  green.
- `pnpm -C ui build` (tsc + vite) and `pnpm -C ui test` green.
- GUI check pending: the palette fades in over ~20ms with no opaque first frame and no double
  animation (system + CSS); with "Reduce motion" on it appears instantly.

## Comments

- 2026-10-15 (agent): duration dialed 100ms → **20ms** at the owner's request, to feel how fast the
  launch can be (`ui/src/styles.css`). 20ms is below a 60 Hz frame, so the fade is effectively a
  cut with the faintest blend; the mechanism (arming on hide, no opaque flash, reduced-motion
  collapse) is unchanged. If it reads as too abrupt, 40–80ms is the next step up.
- 2026-10-15 (agent): filed and delivered in one pass from the session's ask; delivered as 58435a4.
  The AppKit behavior value (2 = None) is taken from the SDK header, not guessed; the previous
  animation's exact duration was **not** measurable from this repo and is deliberately not claimed
  — it is the reason for owning the entrance instead.
