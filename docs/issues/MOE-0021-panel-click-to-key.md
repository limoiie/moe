# M8c: the Side View keys itself on the first click

- **ID**: MOE-0021
- **State**: in-progress
- **Labels**: bug, ux
- **Created**: 2026-10-09
- **Updated**: 2026-10-09
- **Assignee**: agent

User feedback: "sometimes I clicked outside, a few seconds later, I clicked inside the window, but
it is possible that the cursor is missing no matter how many times I clicked in the composer or
other component inner window. I have to click outside then inside, there is a chance to get the
cursor back."

## Diagnosis

- The Side View is a non-activating `NSPanel` (`MoeChatPanel`: `can_become_key_window`,
  `is_floating_panel`, `NonactivatingPanel` style, `hides_on_deactivate(false)`).
- The click lands (hover states and buttons respond) but no caret appears and typing goes nowhere:
  the panel simply is not the key window, so keyboard focus never arrives. AppKit's click-to-key
  path for non-activating panels occasionally drops that transition (seen after app/Space
  switches); repeated clicks only re-roll the same path — which is why the outside/inside ritual
  "sometimes" helps.
- The show path is reliable: `show_and_make_key` calls `makeKeyWindow` programmatically, and typing
  works immediately after showing. The fix follows that proven path instead of AppKit's.

## Design

**The panels key themselves on the first click.** `moe-platform::mac::install_panel_click_to_key`
installs a local `LeftMouseDown` monitor: before the event dispatches, if the receiving window is
ours and not key, it calls `makeKeyWindow()`. The click is then handled like a normal click in a key
window — caret included. A local monitor only sees events bound for this app, and `makeKeyWindow`
does not activate the app, so the non-activating panels keep their semantics (no app activation, no
Space stealing). Installed once at startup; the monitor block and its removal token live for the
process, and a `static` guard keeps the install idempotent.

## Delivery

- `crates/moe-platform/src/mac.rs`: `install_panel_click_to_key` (+ `block2` as a macOS dependency).
- `crates/moe-app/src/main.rs`: installed once in setup.

## Acceptance

- Rust: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace` green.
- GUI check pending: click into the Side View from another app / the desktop, any number of times —
  the caret appears on the first click and typing works immediately.

## Comments

- 2026-10-09 (agent): filed from user feedback; delivered in the working tree (uncommitted).
  Milestone prefix guessed as M8c — rename if the milestone series differs.
