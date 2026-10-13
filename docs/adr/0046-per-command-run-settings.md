# Per-command run settings: the model, the reasoning effort, the temperature

Every AI Command (ADR-0024) ran on the active pair (ADR-0040), with the provider's own reasoning
default and no temperature at all. That is one setting for twelve jobs: Translate wants a cheap, fast,
low-temperature model, Rewrite Writing wants the strongest one, and a batch transform rarely wants the
chat's reasoning budget. The settings window managed providers, keys and picker visibility (MOE-0029)
but nothing about how a command actually runs.

## Decision

- **`[ai.commands."<command-id>"]`, one block per command, four optional keys**: `provider` + `model`
  (both or neither — half a pair falls back to the active pair rather than guessing which model was
  meant), `reasoning` (`off` / `low` / `medium` / `high`), and `temperature`. Absent means exactly
  what the command did before this ADR: the active pair, the provider's own reasoning default, and no
  temperature key on the wire. An empty table changes nothing, and every key the settings window
  clears is *removed* from the file rather than written as a no-op value.
- **Reasoning becomes a request-level vocabulary.** `ChatRequest.reasoning` was a bool; it is now
  `Reasoning { Off, Default, Effort(level) }`, resolved as *the command's setting when it has one,
  else the provider's catalog/block flag*. Each adapter maps it to its own dialect: Anthropic's
  `thinking` budget (the old half-of-`max_tokens` formula for `Default`, 1024/2048/4096 for the
  levels; MiniMax's single on-state stays `adaptive`), the Responses API's `reasoning.effort`, and —
  honestly — nothing on the Chat Completions dialect, where reasoning is a property of the model and
  not of the request. The settings row says so instead of offering a knob that does nothing.
- **Temperature is absent by default, and that is the only safe default**: several reasoning models
  reject an explicit value, so `None` means the key is not sent, and the range the window accepts
  (0.0–2.0) is the widest the protocols allow — a provider with a narrower one says so on its own 4xx.
- **The resolver grew a pair parameter, not a second resolver.** `resolve_pair_with` takes an
  optional provider/model override (the active pair when there is none) and `resolve_command_run`
  layers the command's block on top; the chat surfaces keep calling plain `resolve` and get exactly
  the old behavior — the per-command knobs belong to the AI Commands, not to conversations.
- **The settings window's new section is driven by the registry.** The rows come from the real
  `ai-commands` registry (title, subtitle, icon), so the window cannot drift from what the extension
  offers, and each row shows the pair the command *resolves to* (its own or the inherited active one)
  rather than making the reader work out what "default" means. The controls are the platform's own
  select/input widgets: this is a real window, and the checkboxes already are (MOE-0029).

## Cost

- `CommandAiConfig` carries an `f64`, so `AiConfig` and `MoeConfig` lost their `Eq` (they keep
  `PartialEq`).
- `ChatRequest.reasoning` changed type and gained `temperature`, so every request literal moved —
  the compiler enumerated them (agent, the three adapters' tests, AI Commands).
- The Anthropic budget arithmetic moved out of the adapter into the resolver (`anthropic_thinking`
  now takes the `Reasoning`), which is why the adapter's test passes budgets explicitly: the adapter
  emits what it is given, and the ladder is pinned where it is computed.
- Only bounded effort levels exist: a free-form budget or an `effort: "minimal"` (OpenAI's lowest)
  has no vocabulary yet, and the values are not per-provider (a provider that only supports `low` and
  `high` is not modeled).
