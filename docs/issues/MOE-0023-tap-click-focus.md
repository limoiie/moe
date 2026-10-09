# M8e: a remapped tap click keys the Side View like a real click

- **ID**: MOE-0023
- **State**: done
- **Labels**: bug, ux
- **Created**: 2026-10-09
- **Updated**: 2026-10-09
- **Assignee**: agent
- **Commit**: 9a896f1

User feedback: with BetterTouchTool remapping the Magic Mouse's left-tap to "left-single-click",
"after I click outside the side-chat window, then left-tap inner the window, the window wouldn't
get focus, but the three buttons and capsules shows up as muted. then I click inner the window, it
still wouldn't get focus. I have to single click outside the window then single click inside the
window, the window starts get the focus."

## Diagnosis (from the diagnostic runs)

- The tap's click **is delivered** to the app (the local monitor and the DOM both see it; the
  global by-location net never fired). The click just never keys the window.
- Immediate key requests are refused (the mouse-up line proves the down's request did not take),
  the deferred retry is refused even well past the event (three ticks, all still not key), and an
  explicit activation (`activateIgnoringOtherApps:`) is refused too — repeatedly.
- Conclusion: macOS refuses key/focus grants for remapped/synthetic input because no *user
  interaction* is recorded for it — but only for a **non-activating panel**; ordinary windows take
  the standard click-to-focus path (which is why taps focus other apps). Physical clicks keep
  working because the window server grants key on a genuine click.

## Design

- **The chat window's activation classification is dynamic (the fix).** Base: non-activating —
  the show path floats over a full-screen Space and takes key without activating (the summon
  regression is gone). While the pointer is over an *unfocused* window (the AppKit tracking that
  already drives hover): activating, so a click — physical or remapped — takes the ordinary
  click-to-focus path; pointer out flips back. A keyed window's mask is never touched (that is
  when typing happens), and the show path resets the base before showing. The summon panel stays
  non-activating: showing it must not steal the app you are in.
- **The layered click hardening stays.** The local monitor (left/right/other down + up, the
  `orderFrontRegardless` + `makeKeyWindow` sequence), the page fallback (`focus_side_view` when a
  click lands while `moe-window-focused` is absent), the deferred retry (guarded by
  visible/not-key/pointer-inside, escalating to activation in the refused path), and the global
  by-location monitor for clicks that never reach the app at all. For an activating window these
  are usually no-ops; they remain the belt for the summon panel and for exotic input.

### Diagnostics

The routine lines were trimmed after confirmation (they fired on every click / hover); what stays
are the rare failure-path signals:

- `moe: side view keyed by the deferred click retry` — the retry past the event carried.
- `moe: side view keying refused; activating the app` — even the retry was refused.
- `moe: keyed the side view for a click that missed delivery` — a click that never reached the app
  (not observed so far).

## Delivery

- `crates/moe-platform/src/mac.rs`: the monitor + deferred/activation escalation + global
  by-location monitor + diagnostics; the pointer tracking also flips the window's activation
  classification (`apply_window_activation` / `set_window_activating`).
- `crates/moe-app/src/main.rs`: the chat panel keeps the non-activating style at creation; the
  show path resets it; `focus_side_view` (+ registration).
- `ui/src/chat.ts`: the capture-phase fallback.
- Docs: this issue; ADR-0004 amendment (the classification is dynamic).

## Acceptance

- Rust: `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check`,
  `cargo test --workspace` green; UI: `pnpm -C ui build` green.
- GUI check passed (user): click outside → left-tap inside → focus (lit chrome, caret); the summon
  again floats over another app's full-screen Space; typing stays undisturbed while hovering.
- Known follow-up: the first real click after a tap-activation is swallowed — filed as MOE-0024.

## Comments

- 2026-10-09 (agent): filed from user feedback; two diagnostic runs localized the refusal — the
  click is delivered, but key and activation are refused for synthetic input on a non-activating
  panel — and led to the dynamic activation classification. Confirmed in the GUI ("it works"); the
  routine diagnostic lines were trimmed, the rare failure-path ones kept. Milestone prefix guessed
  as M8e — rename if the milestone series differs.
- 2026-10-09 (agent): closed; commit 9a896f1. Follow-up filed: MOE-0024 (the first click after a
  tap-activation is swallowed).
