# M10k: ⌘W closes without remembering the page, and a delivered result closes too

- **ID**: MOE-0065
- **State**: done
- **Labels**: ready-for-agent, feature, ux
- **Created**: 2026-10-15
- **Updated**: 2026-10-15
- **Commit**: b5fa0a7
- **Assignee**: agent

Asked from a session: "I want cmd+w to close moe's window without remembering the page, that is, the
next launch should locate the root page without inputs rather than restoring the page before closing
the window. additionally, I want the primary action, the copy action, the write back action, and
similar actions which finishes user's intentions, like getting the results, should close the window
after action applied with restoring the page to the root just as cmd+w. Remember to write down that
design."

## Problem

- **The panel kept its page across summons, always.** Hiding never touched `view`, `q.value`, the chat
  state or the cards, so the next summon returned to the page — the intended "interrupted, not done"
  behavior (Esc / blur), but the *only* behavior. There was no way to leave without leaving the state.
- **⌘W did nothing on the panel.** Not bound in `ui/src/main.ts`; on macOS it was an AppKit menu key
  equivalent (Tauri default menu: File ▸ Close and Window ▸ Close, both ⌘W, action `performClose:`)
  that dead-ends on a borderless non-closable panel.
- **The two errand-finishing actions mishandled the exit.** Write back hid the panel but left the page
  lived-in, so the next summon reopened a result already delivered to the host app. Copy did not close
  at all: it returned `ActionResult::Silent`, and the UI answered with a "Copied" toast — a toast that
  could even survive the hide to greet the next summon (the panel-wide state reset is new).

## Design — ADR-0047

- **Two postures, two exits.** `back()` (Esc / empty ⌫ / blur) = page-keeping hide, unchanged. ⌘W
  (**`SystemKey::Close`**, "⌘W" in the platform keymap) = `closePanelToRoot()`: `resetToRoot()` then
  `hide_panel`. `resetToRoot` closes the confirm dialog, attachment mode, the peek, all four cards and
  the model switcher; clears the composer draft and the chat state (conversation, title, pair, history,
  generating flag); clears the detail pane and the result set; clears the Input Bar; and re-searches the
  empty query so Suggestions/Favorites are current. Argument values (ADR-0045) and the per-command
  filter memory (MOE-0046) are session memories, not on-screen page state — they survive.
- **`ActionResult::Delivered`** is the new "the outcome left the panel" result, so Copy can end the
  errand like Write back does: `applyResult` resets the page on `Delivered` (and hides the panel) and on
  `WriteBack` (the backend already hides). `Silent` stays as it was — "nothing for the UI to do" still
  covers theme switches and `moe.open-config`, which must keep the page. The three Copy actions (AI
  Chat, AI Commands, Echo) and the registry's test fixture return `Delivered`; the `⌥⏎` "Copied" toast
  is removed at all three call sites.
- **⌘W reaches the webview on macOS: the app menu drops its Close items.** `install_app_menu` builds the
  default menu minus File ▸ Close / Window ▸ Close (App / Edit / View / Window / Help kept, so
  ⌘C/⌘V/⌘Z/⌘Q/⌘H/⌘M and Services are untouched) and `set_menu`s it at setup. The Settings window keeps
  ⌘W as a webview binding (`hide`, the same as its close button).
- **A live generation is not stopped by ⌘W** (stop is explicit and platform-wide, ADR-0036); a chat
  answer still lands in history, a result page's stream dies with the page. Deliberate, recorded in the
  ADR's Cost section, and reversible if the owner prefers an ask-first close.

## Delivery

- `crates/moe-core/src/contract.rs`: `ActionResult::Delivered`.
- `crates/moe-core/src/keymap.rs`: `SystemKey::Close` + `("⌘W", Close)` (+ the count assertion 18 → 19 and
  a binding test).
- `crates/moe-extensions/{ai,ai_commands,echo}.rs`, `crates/moe-core/src/registry.rs` (fixture): Copy →
  `Delivered`.
- `crates/moe-app/src/main.rs`: `install_app_menu` (macOS) + its call in `setup`.
- `ui/src/main.ts`: `resetToRoot` / `closePanelToRoot`, the ⌘W branch (before the modal precedence: a
  modal *is* page state), `applyResult`'s `delivered` / `writeBack` handling, toast removals.
- `ui/src/settings.ts`: ⌘W → hide.
- `ui/src/types.ts`: comment for the new unit variant.
- `docs/adr/0047-close-and-delivered-results-forget-the-page.md`.

## Acceptance

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test
  --workspace` green (9 + 57 + 130 + 72 tests). `pnpm -C ui build`, `pnpm -C ui test` green (96 tests).
- GUI check pending (no display session here): ⌘W on any page lands the next summon on a fresh root with
  an empty bar; ⌥⏎ copy (AI answer / AI Command / Echo) copies, dismisses and forgets; ⏎ write back does
  the same and still writes into the host app; Esc still keeps the page; ⌘C/⌘V still work in the Input
  Bar, the composer and the Settings window's fields (the app menu kept its Edit submenu); ⌘W hides the
  Settings window.

## Comments

- 2026-10-15 (agent): filed, implemented and documented in one pass. The macOS finding that shaped the
  design: Tauri's default menu owns ⌘W through *predefined* items (muda: "a predefined item emits no
  event" — they run `performClose:` down the responder chain), so no `on_menu_event` hook can intercept
  it and the panel's non-closable borderless window dead-ends it; the menu had to lose those items
  rather than be handled. Delivered as b5fa0a7.
