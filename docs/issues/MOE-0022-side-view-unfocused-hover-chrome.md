# M8d: the Side View reveals its chrome on hover even when unfocused

- **ID**: MOE-0022
- **State**: in-progress
- **Labels**: ux
- **Created**: 2026-10-09
- **Updated**: 2026-10-09
- **Assignee**: agent

User feedback: "the three buttons and capsule shows up only if I clicked inner the window and hover
the window. I want that even if I clicked outside the window, hover the window will show the three
buttons and capsule as well, but in muted style (gray/disabled three buttons, flatten/muted
capsule)". After the CSS-only pass: "once I clicked outside the side-chat window, hovering it
wouldn't show the three buttons and capsule".

## Diagnosis

- The reveal was already pointer-driven, but **WebKit gates its mouse tracking on key status**: a
  non-key window's webview receives no pointer events at all, so `mouseenter`/`mousemove` never
  fired after clicking outside — the muted look that existed was unreachable.
- AppKit's own tracking does work in inactive windows (traffic lights highlight on hover); it is
  the webview's DOM layer that goes dark.

## Design

- **Native tracking drives the same state.** `install_side_view_pointer_tracking`
  (`moe-platform::mac`) adds an `NSTrackingArea` (`mouseEnteredAndExited | activeAlways |
  inVisibleRect`) to the webview view, owned by a small `NSObject` subclass; enter/exit are
  evaluated back into the page (`window.__moePointerInside`, published by `ui/src/chat.ts`) and set
  the same `moe-pointer-inside` class the DOM events set while focused. `acceptsMouseMovedEvents`
  is switched on too, so the webview's own `:hover` styles recover while unfocused.
- **The muted variant is a style, not a fade.** Pointer over an unfocused window → the traffic
  lights desaturate and soften (`filter: grayscale(1)` + 0.55, the macOS inactive-window read),
  the action capsule flattens (no sheen, blur or rim; a plain `surface-hover` wash) with its icons
  at 0.6 — all still clickable. Clicked in → the same hover lifts to the lit state. Pointer out →
  hidden chrome, dimmed title.
- The DOM listeners stay: while the window *is* key they are the live path (the native hooks are
  idempotent duplicates).

## Delivery

- `crates/moe-platform/src/mac.rs`: `SideViewPointerOwner` + `install_side_view_pointer_tracking`.
- `crates/moe-app/src/main.rs`: helper + install in setup (evaluates `window.__moePointerInside` in
  the chat webview).
- `ui/src/chat.ts`: publishes the hook; the styled-vs-faded comment update; `mousemove` fallback.
- `ui/src/styles.css`: the muted variant (gray lights, flat capsule).
- Docs: ADR-0035 amendment.

## Acceptance

- Rust: `cargo clippy --workspace --all-targets -- -D warnings` green; UI: `pnpm -C ui build` green.
- GUI check pending: click outside, hover the Side View → gray trio + flat capsule appear without a
  click; click in and hover → lit; pointer out → hidden.

## Comments

- 2026-10-09 (agent): filed from user feedback. The first pass was CSS-only (a styled muted
  variant) and did not fix the missing hover — WebKit delivers no pointer events to a non-key
  window's webview; the native tracking path followed. Delivered in the working tree (uncommitted).
  Milestone prefix guessed as M8d — rename if the milestone series differs.
