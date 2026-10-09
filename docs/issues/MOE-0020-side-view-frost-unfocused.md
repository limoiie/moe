# M8b: the Side View's frost is a native, active-pinned material

- **ID**: MOE-0020
- **State**: done
- **Labels**: bug, ux
- **Created**: 2026-10-09
- **Updated**: 2026-10-09
- **Assignee**: agent
- **Commit**: a6c4198

User feedback: "when the side-chat window losts focusing (I clicked outside the window), its bg
will turns into transparent a few seconds later. is it by design or by mistakes?" — then, after
checking a first hypothesis (the focus-time shadow refresh): "a fade still appears, the desktop
shows crisp through the card/window, card isn't frosted […] ; clicking inside the card/window will
bring it back".

## Diagnosis

- The card's own fill never changes: `--moe-surface` is 0.80/0.82 alpha, and focus keys nothing in
  the stylesheet except the titlebar chrome. The window staying open on blur is by design (only the
  summon panel dismisses on blur).
- What fades is the **behind-window backdrop**: macOS stops compositing a window's backdrop while
  the window is not key — the same policy `NSVisualEffectView` exposes as
  `followsWindowActiveState`. The card's CSS `backdrop-filter` samples that backdrop, so the frost
  disappeared a moment after clicking outside (desktop crisp through the card) and returned on
  click-in. CSS has no opt-out of that fade.
- The earlier focus-time `refresh_shadow` ping (MOE-0018) was aimed at the same transition from the
  wrong side; it is now scoped to focus *gain* only (the rectangular "block" fix), and the blur
  side gets a real fix.

## Design

- **macOS: the frost moves native.** `moe-platform::mac::install_side_view_material` installs an
  `NSVisualEffectView` (`underWindowBackground`, `behindWindow`) directly behind the webview,
  inset and rounded to the card's geometry (8 px inset / 16 px radius, mirroring ui/chat.html's
  `m-2` + `rounded-2xl`), with `state = active` — the material ignores the window's key status.
  The insets are the autoresizing struts, so resizes keep the margin.
- **Rounded clip is required**: square material corners would leak frost into the card's corner
  cutouts and square off the system shadow's rounded shape (ADR-0016). The corner radius uses the
  private-but-long-stable `setCornerRadius:` (window-vibrancy ships the same call).
- **One blur at a time**: `html[data-platform="macos"]` (set only by chat.html) drops the card's
  `backdrop-filter`; tint, sheen and rim stay CSS. The panel and non-macOS platforms keep the pure
  CSS path unchanged. `NSAppearance` sync is deliberately deferred — the material sits under the
  card's ~0.8-alpha fill, so its tint is ~15–20% of the composite while the blur does the work
  (see the ADR amendment).
- **Install points**: once at startup after the NSPanel conversion; re-asserted in
  `show_side_view_blocking` (the function is idempotent).

## Delivery

- `crates/moe-platform/src/mac.rs`: `install_side_view_material` + geometry constants.
- `crates/moe-app/src/main.rs`: helper + startup install + show-path re-assert.
- `ui/chat.html`: platform seed; `ui/src/styles.css`: the macOS drop rule.
- Docs: ADR-0035 amendment.

## Acceptance

- Rust: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace` green; UI: `pnpm -C ui build` green.
- GUI check pending: click outside → the card keeps its frost (no fade, no crisp desktop); clicking
  back in changes nothing; the window's system shadow stays rounded and the 8 px margin stays
  transparent.

## Comments

- 2026-10-09 (agent): filed from user feedback; delivered in the working tree (uncommitted).
  Milestone prefix guessed as M8b — rename if the milestone series differs.
- 2026-10-09 (agent): closed as part of the stack commit at the user's request ("solved");
  commit a6c4198.
