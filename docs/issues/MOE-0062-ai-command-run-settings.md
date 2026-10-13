# M10h: the AI Commands' own model, reasoning effort and temperature

- **ID**: MOE-0062
- **State**: done
- **Labels**: ready-for-agent, feature, ux
- **Created**: 2026-10-13
- **Updated**: 2026-10-13
- **Commit**: 4b9c573

Requested from use: "add a configuration section in the settings for configuring the underlying models
of these ai commands, including the reasonble effect (if possible), the temperature, etc."

Every AI Command ran on the active pair with the provider's own reasoning default and no temperature —
one setting for twelve jobs. The settings window could pick the *chat's* model but had nothing to say
about how a command runs.

## Decision

- **A new "AI Commands" section**: one row per command (from the real registry, so it cannot drift),
  showing the model the command resolves to — its own override, or the active pair it inherits — and
  a disclosure with three controls: the model (a select of the enabled models, grouped by provider,
  plus "Active model (default)"), the reasoning effort (Provider default / Off / Low / Medium / High)
  and the temperature (a number, empty = not sent).
- **`[ai.commands."<id>"]` in config.toml** (ADR-0046): four optional keys, written and removed by
  text surgery so the file keeps its comments and order, and a block with nothing left goes away
  entirely. Each control writes exactly one key: a change to the temperature cannot drop the model.
- **The reasoning effort is real where the protocol has a knob** (Anthropic's thinking budget, the
  Responses API's `effort`) and says so where it does not (the Chat Completions dialect: reasoning is
  a property of the model) — the select is disabled with that sentence instead of silently doing
  nothing.
- **The temperature is off by default** because several reasoning models reject an explicit value;
  empty means the key is not sent, and the accepted range is 0.0–2.0.

## Delivery

- `crates/moe-platform/src/config.rs` — `CommandAiConfig`, `ReasoningSetting`, the `[ai.commands.*]`
  parsing and the `set_command_ai_in_toml` surgery (two tests).
- `crates/moe-extensions/src/ai_client/` — `Reasoning` (Off / Default / Effort) replacing the bool,
  `ChatRequest.temperature`, the three adapters' mapping, `resolve_pair_with` +
  `resolve_command_run` + `protocol_of`, and `anthropic_thinking(provider, reasoning)`.
- `crates/moe-extensions/src/ai_commands.rs` — the run settings reach `prompt_request`.
- `crates/moe-extensions/src/ai_models.rs` — `CommandSettings` + `command_settings_with`, the three
  setters (+ `_with` variants for tests), two tests.
- `crates/moe-app/src/main.rs` — four IPCs: the settings rows and the three setters.
- `ui/src/settings.ts` — the section, the rows and the three fields.

## Acceptance

- [x] The config parses per command, validates the effort, and an unknown value is an error.
- [x] The surgery round-trips: keys are added, replaced and removed in place, an emptied block goes
      away, and the rest of the file stays byte-identical.
- [x] The resolver is pinned: a command's own pair wins, half a pair falls back, the effort and the
      temperature resolve, and the Anthropic ladder is right (default formula, three levels, MiniMax's
      single on-state, off omits).
- [x] The request carries them: the effort and the temperature reach the protocol bodies, and a run
      that sets neither sends neither.
- [x] The settings rows are pinned: every AI Command appears, an unconfigured one shows the inherited
      pair, a configured one shows its own plus its knobs and the protocol's support.
- [x] The setters are pinned: each keeps the keys it does not own, and unknown effort / half a pair /
      out-of-range temperature are refused.
- [x] `just test` green (267 Rust tests), `just ui-test` 96, `clippy`, `tsc`, `vite build` green.
- [ ] The live pass: the section lists every AI Command; changing the model of one command and running
      it uses that model; the reasoning select is disabled with the note for an openai-chat provider;
      a temperature shows up in the request (the app log / the provider's dashboard).

## Comments

- 2026-10-13 (agent): filed and closed in one pass, with ADR-0046 recording the schema and the
  protocol mapping. Two judgment calls worth naming: the reasoning vocabulary is a three-level ladder
  rather than free text (the adapters need to map it), and the per-command settings are the AI
  Commands' only — the chat keeps the active pair and the provider's defaults, because a conversation
  is one long context where a per-command knob has no meaning.
- 2026-10-13 (agent): the pre-existing `cargo fmt` diff in `anthropic.rs` was formatted while that
  file was open for this change (the adapter's body and tests were rewritten), so `just lint` is now
  clean rather than red for that one line.
