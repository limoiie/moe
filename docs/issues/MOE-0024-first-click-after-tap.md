# M8f: the first click after a tap-activation is swallowed

- **ID**: MOE-0024
- **State**: todo
- **Labels**: bug, ux
- **Created**: 2026-10-09
- **Updated**: 2026-10-09

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
