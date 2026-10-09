# M7o: the AI Command result is a decision point — no automatic write-back

- **ID**: MOE-0007
- **State**: in-progress
- **Labels**: feature, ux
- **Created**: 2026-10-09
- **Updated**: 2026-10-09
- **Assignee**: agent
- **Commit**: (pending)

User feedback: "once the ai command finishes generation, it writes back the outputs directly without my
confirmation. but, I want to see the modified text in moe, and decide if I want to improve again, or
discard, or apply by writing back, or just copy to clipboard."

## Requirement

1. Natural completion must not write back or dismiss the panel: the result stays on screen for review.
2. Four decisions from the card: apply (write back) / copy / improve again / discard.

## Decision (ADR-0024 amendment)

- The auto write-back (`on_done` → `CommandEvent::WriteBack`) is removed from the AI Commands stream.
  The contract variant stays (the platform intercept is intact for extensions that want automatic
  write-back); this extension simply no longer emits it, and the blocking `invoke` path returns the
  same review card instead of a `WriteBack`.
- Apply = the card's primary `write-back` action (⏎, the manual path ADR-0036 already primed) →
  `deliver_writeback` (writes + dismisses the panel). Copy = ⌥⏎ (unchanged). Discard = Back (Esc /
  empty ⌫, ADR-0038) — the host text is untouched.
- **Improve Again** = a new declared action `rerun`, titled "{Command} Again" ("Make Shorter Again",
  "Translate to Chinese Again"…): the same transform re-run **on the current result**, streaming into
  the same card. Contract: `ActionResult::Rerun { text }` — the extension states the intent; the
  platform intercepts it in the `run_item_action` command and invokes the command again with `text`
  as the input **instead of the captured Selection**, handing that call's result back to the UI.
  This way the extension needs no emitter in `run_item_action` and no duplicated config/key pipeline.
  A rerun while the card is mid-stream is a no-op (one stream per card).

## Delivery

- `crates/moe-core/src/contract.rs`: `ActionResult::Rerun` + doc + serde test.
- `crates/moe-extensions/src/ai_commands.rs`: `on_done` → no-op; `transform_item(title, …)` + the
  rerun action; `run_item_action`'s `rerun` branch; `invoke` returns the card.
- `crates/moe-app/src/main.rs`: `apply_result_effects` factored out; the Rerun intercept in the
  `run_item_action` command.
- `ui/src/types.ts` + `ui/src/main.ts`: the `rerun` result branch (defensive no-op — the platform
  intercepts it before it reaches the UI) and the rerun row icon.
- Docs: ADR-0024 amendment (Chinese 增补); CONTEXT (`AI Command`); README (AI Commands bullet).

## Acceptance

- Let a transform finish: the panel stays, the card holds the text, nothing is written back.
- ⏎ writes back and dismisses; ⌥⏎ copies; ⌘K lists "{Command} Again"; Esc leaves the host text alone.
- "{Command} Again" streams a new result into the same card (⏎ still stops it mid-stream).
- `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test
  --workspace`, `pnpm build` green.

## Comments

- 2026-10-09 (agent): filed from user feedback; implementing.