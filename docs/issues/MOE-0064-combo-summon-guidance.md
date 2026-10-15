# M10j: a combo summon key stops showing the double-tap guidance

- **ID**: MOE-0064
- **State**: done
- **Labels**: ready-for-agent, bug, ux
- **Created**: 2026-10-15
- **Updated**: 2026-10-15
- **Commit**: a656c51
- **Assignee**: agent

Reported from use, after the summon key moved to `cmd+space` (MOE-0063's session): the panel opens
fine with ⌘Space, but carries the banner

> Command+Space to summon is unavailable in this session (Wayland and similar cannot globally
> intercept the keyboard): set [summon] key in config.toml to a combo, or bind `moe --toggle` in
> your WM.

## Diagnosis

- **The listener was asked a question it does not answer.** A combo key is held by the
  global-shortcut plugin, so `main` maps `SummonKey::Combo { .. }` to `UnsupportedSummon` — by
  design ("Combos are handled by the global-shortcut plugin; the listener stays empty"). `summon_status`
  forwarded that `Unsupported` verbatim, and the panel's `unsupported` branch printed the
  Wayland/double-tap guidance.
- **Two statuses were being conflated**: "this session cannot intercept the keyboard" (a double-tap
  fact) and "the plugin could not register this chord" (a combo fact). The startup path already knew
  the difference — `needs_attention` gates `Unsupported` on `DoubleTap` — but the banner path did not,
  so the panel stayed wrong every time it was opened.
- The banner text was also unhelpful for a combo failure in its own right: it advises setting a combo,
  which the user already has.

## Design

- **The combo's registration outcome is its only status.** `AppState` gains
  `summon_registered: AtomicBool`, set by the `global_shortcut().register` result at startup;
  `summon_status` reports `ready`/`unsupported` from it when the key is a combo, and keeps asking the
  listener only for a double-tap key. `SummonStatusPayload` gains `combo: bool` so the panel can tell
  the two stories apart.
- **A combo failure gets its own sentence**: "… could not be registered as a global shortcut: another
  app owns the chord, or this session does not support global shortcuts. Pick another combo in
  config.toml, or bind `moe --toggle` in your WM." The double-tap text is unchanged for double-tap
  keys, and the accessibility branch is unchanged for both.
- **Not attempted: detecting the macOS silent-success case.** With Spotlight holding ⌘Space,
  `RegisterEventHotKey` returns `noErr` (measured with a throwaway Carbon probe: status 0), so the
  chord registers "successfully" and simply never fires; the backend cannot tell that apart from a
  working registration, so no banner would be honest. Recorded as a blind spot in ADR-0008 instead.

## Delivery

- `crates/moe-app/src/main.rs`: `AppState::summon_registered`, the registration result stored at
  startup, `SummonStatusPayload::combo`, and the combo-aware `summon_status`.
- `ui/src/main.ts`: `SummonStatus.combo`; `refreshBanner` splits the combo case (new text) from the
  double-tap `unsupported` text.
- `docs/adr/0008`: dated amendment — the guidance split and the macOS blind spot.

## Acceptance

- `cargo clippy -p moe-app --all-targets -- -D warnings` + `cargo fmt --all --check` green; `pnpm -C ui
  build` and `pnpm -C ui test` green.
- GUI check: with `key = "cmd+space"` and a registered chord, the panel opens with **no** banner (the
  Accessibility banner may still appear if that permission is missing — separate concern); with a
  chord another app owns, the combo sentence replaces the Wayland one; a double-tap key still shows
  the Input Monitoring / Wayland guidance exactly as before.

## Comments

- 2026-10-15 (agent): filed and fixed in the same session as the report. `needs_attention` needed no
  change (it was already combo-aware), which is what pointed at `summon_status` as the single wrong
  seam. Delivered as a656c51.
