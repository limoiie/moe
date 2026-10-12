# M8f: the first click after a tap-activation is swallowed

- **ID**: MOE-0024
- **State**: todo
- **Labels**: bug, ux, needs-info
- **Created**: 2026-10-09
- **Updated**: 2026-10-12

User feedback, after the tap fix landed (MOE-0023): "after left-tap to activate the view, the first
click inner the view has no response, but after that, everything works."

## Symptom

1. The Side View is visible and unfocused (e.g. after clicking outside).
2. A left-tap (BetterTouchTool's remap of the Magic Mouse tap) inside focuses it.
3. The **next real click** inside does nothing — no caret move, no button action.
4. Every click after that behaves normally.

## Hypotheses to check when picked up

- The synthetic tap's mouse-down/up may be consumed as the activation gesture, and the following
  genuine click can be merged into it as a double-click (`clickCount` 2), which our controls read
  as "no response".
- Or the tap activates the app without giving the webview first-responder status, so click #1 is
  spent establishing focus. `acceptFirstMouse` is on for both windows, so the classic
  activation-click swallow should not apply — verify what the webview actually receives.
- The page fallback (`focus_side_view` on `mousedown` while `moe-window-focused` is absent) also
  runs on that first click; it only keys the window, but the ordering is worth excluding.

## Notes

- Deferred deliberately: the workaround is one extra click, and the condition needs BetterTouchTool
  taps to reproduce. Revisit with a scratch build that logs the first two clicks' `NSEvent` details
  (`clickCount`, window key state, first responder) from the chat webview.

## Findings from a code pass (2026-10-12) — three of four hypotheses excluded

Picked up again with the whole click-to-key path read end to end. **No fix was written:** the remaining
hypothesis cannot be confirmed without the reporter's hardware, and the plausible-looking fix turns out
to have been considered and rejected in the source already.

- **The window re-key is not a swallow candidate, and guarding it is wrong.** The local mouse-down
  monitor re-runs `orderFrontRegardless` + `makeKeyWindow` unconditionally
  (`install_panel_click_to_key`, `crates/moe-platform/src/mac.rs`), and its doc comment says why: the
  state "can *read* key while the keyboard focus never arrived — a guard would skip exactly that case".
  `makeKeyWindow` is a no-op when the window is already key, so on the second click this path costs
  nothing. Excluded in the code itself.
- **The page's fallback is a no-op in the same situation.** The DOM-side `focus_side_view` on a click
  while `moe-window-focused` is absent (`ui/src/chat.ts`) ends in that same sequence, and the two
  monitors that could act mid-click both early-return on `isKeyWindow()`
  (`install_panel_click_by_location`; the deferred retry's `tick`). So H3 — "the fallback also runs on
  that first click" — excludes itself: by the time it acts, the window is key.
- **H1 (a merged double-click) cannot explain "no response" in this codebase.** Every interactive
  handler is a plain `click` listener and none branches on `MouseEvent.detail`, so a click arriving as
  the second of a merged pair still fires. If the merge happens at all it happens at the AppKit level
  and the click is lost before the DOM sees it — which is what the diagnostic below would show.
- **Left: H2, the activation-click path.** `acceptFirstMouse` is on for both windows, so the classic
  activation swallow should not apply — but MOE-0023's own diagnosis established that macOS refuses
  key/focus grants for *remapped/synthetic* input, and that the chat panel's classification flips to
  *activating* while the pointer is over an unfocused window. Whether the first genuine click after a
  tap finds the mask still on the activating side is a question about AppKit state, not about this
  repository's code.

### What the diagnostic build should log (the ask)

For the first two clicks after a tap, per click:

1. the `NSEvent`'s `clickCount` and `type`, and whether `NSApp.keyWindow` is the chat window — at
   mouse-down *and* at mouse-up;
2. `window.isKeyWindow()` and the window's style mask at those same two points;
3. the window's first-responder class at mouse-down;
4. the DOM side: whether a `mousedown` and then a `click` reached the document, with each event's
   `detail` and target.

That separates all four hypotheses in one run: no DOM `click` at all, with a complete AppKit sequence,
points at the activation path; a `click` with `detail === 2` that runs no handler points back at the
merge; a mask still on the activating side points at MOE-0023's flip.

## Comments

- 2026-10-09 (agent): filed from MOE-0023's GUI pass — that ticket fixed tap-activation itself, and this
  is the residue: the first *real* click after a tap does nothing.
- 2026-10-12 (agent): **moved to `needs-info` after a code pass**, with three of the four hypotheses
  excluded and the diagnostic recipe written out above. Nothing was changed: the plausible fix (guard
  the re-key on `isKeyWindow`) is already rejected in `mac.rs` with a stated reason, and what remains is
  a question about AppKit state that only the reporter's tap can answer. This is the one ticket in the
  current set that cannot be finished from the repository — left open rather than closed with a guess.
