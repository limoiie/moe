# M7y: ⌘' / ⌘⇧' toggle Quick Ask and the side chat globally

- **ID**: MOE-0017
- **State**: done
- **Labels**: feature, ux
- **Created**: 2026-10-09
- **Updated**: 2026-10-09
- **Assignee**: agent
- **Commit**: 6ab3c49

User feedback: "bind quick-ask and open-side-chat to cmd+' and cmd+shift+' globally, so that I can
start them directly without open moe previously", followed by "these two keybindings should toggle
the quick-ask and the open-side-chat window properly".

## Design — ADR-0036 amendment

The panel-only ⌘/ / ⌘⇧/ keys (MOE-0016) could not start anything without summoning the panel first.
The launch keys move to **⌘'** / **⌘⇧'** (⌘/ is unbound) and become **global hotkeys**, registered
at startup through the global-shortcut plugin — fixed platform bindings like the summon key, not
configurable; a chord another app owns fails to register (logged), and the same chord keeps working
inside the open panel as the fallback. Both keys **toggle their surface**, the summon key's own
semantics.

- **⌘' — Quick Ask toggle**: a hidden panel is shown (capturing the selection context like a summon,
  ADR-0019/0021) and the `quick-ask-toggle` event carries the visibility the hotkey saw, so the UI
  decides: dismiss (`hide_panel`) when the page is already showing, otherwise enter the page — blank
  from another page, kept as it was when the panel was re-shown on the page. Rust cannot read the
  page state, the webview can: hence the payload.
- **⌘⇧' — side chat toggle**: a visible chat window is hidden, a hidden one is shown *as it was*
  (the `side-show` event refreshes the window without resetting its conversation) — a window toggle
  must not drop the thread the user stepped away from. The tray's "AI Chat" and the `ai.side-chat`
  command still open a blank conversation (the direct-start path), and ⌘J still materializes the
  panel's current conversation.
- The plugin's shared handler routes by chord (`launch_key_of`, Code::Quote); the summon combo
  falls through to the panel toggle. The registered hotkey consumes the chord, so the global path
  and the in-panel fallback cannot double-fire.

## Delivery

- `crates/moe-app/src/main.rs`: `QUICK_ASK_ACCELERATOR` / `SIDE_CHAT_ACCELERATOR`, the `LaunchKey`
  routing, the shared shortcut handler, `toggle_quick_ask` (show + visibility event),
  `show_side_view_blocking` / `hide_side_view_blocking` / `toggle_side_view` (the show core shared
  with `open_side_view`), the `toggle_side_chat` IPC for the in-panel fallback, and startup
  registration; a routing test.
- `crates/moe-core/src/keymap.rs`: the launch bindings move to `⌘'` / `⌘⇧'` — the single display
  source for the rows' Kbd (ADR-0030); test updated.
- `ui/src/main.ts`: `toggleQuickAsk` (dismiss when showing, otherwise enter — kept when re-shown)
  driven by the `quick-ask-toggle` event and the in-panel fallback; `toggleSideChat` for ⌘⇧'.
- `ui/src/chat.ts`: the `side-open` tail factored into `refreshWindow`; the new `side-show` listener
  re-shows the window without resetting the conversation.
- Docs: ADR-0036 amendment, README key table + Quick Ask paragraph.

## Acceptance

- Rust: `cargo test --workspace` green; `cargo fmt --all --check`, `cargo clippy --workspace
  --all-targets -- -D warnings` green.
- UI: `pnpm -C ui build` green.
- GUI check pending (Moe running, panel hidden): ⌘' shows the panel on the Quick Ask page and ⌘'
  again dismisses it; a draft typed before the dismiss is still there when re-shown; ⌘⇧' shows the
  side chat and ⌘⇧' again hides it, with the conversation preserved on re-show; ⌘N / ⌘J / the tray
  paths behave as before; no double-fire.

## Comments

- 2026-10-09 (agent): filed from user feedback; delivered in the working tree (uncommitted).
  Supersedes the key choice of MOE-0016 (the commands and the side-chat entry point stand).
- 2026-10-09 (agent): the user asked for proper toggling — both keys now toggle their surface
  (⌘' dismissal lives in the webview via the visibility payload; ⌘⇧' hides/shows the window with a
  non-resetting `side-show`). ADR-0036 amendment and README updated in the same pass.
- 2026-10-09 (agent): closed as part of the stack commit at the user's request; commit 6ab3c49.