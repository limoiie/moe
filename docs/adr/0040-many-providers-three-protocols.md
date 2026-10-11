# Many providers, three protocols: in-house adapters, catalog defaults, per-provider keys

Moe's AI speaks exactly one wire today: an OpenAI-compatible chat endpoint, one `base_url` + `model`
(ADR-0005's first clause), one key (ADR-0019), with the daily tool loop now being built on top of it
(ADR-0039 — in-house, chat completions, dsh-style faithful replay). The owner's direction: official
providers (DeepSeek, OpenAI, Anthropic, Google, xAI, Mistral, Groq, OpenRouter, Ollama, …) plus
custom providers speaking one of **three protocols** — OpenAI Chat Completions, OpenAI Responses,
Anthropic Messages — with the model picker fed by the embedded/refreshed model catalog (MOE-0025).

The research is settled: `docs/research/ai-provider-libraries-rust.md` (crate landscape; `genai` is
the best standalone multi-provider crate, `rig` is an agent framework), `docs/research/ai-model-catalogs.md`
(models.dev is the catalog; its `npm` field is the protocol hint), `docs/research/zed-ai-provider-layer.md`
(Zed is the best reference implementation — GPL-3.0-or-later where it matters and unpublished, so
study, never copy).

## Decision

- **The client grows three in-house protocol adapters; no multi-provider crate.** `ai_client` becomes
  one `Protocol` enum with three backends — `openai-chat` (today's code, kept byte-compatible),
  `openai-responses`, `anthropic` — each a request builder plus an SSE parser in the existing
  pure-function, recorded-fixture style. The blocking worker-thread streaming design, `CommandEvent`
  emission, stop flags, persistence, and the ADR-0039 loop are untouched; adapters normalize into one
  delta vocabulary (text / reasoning / tool-call fragment / done) so `ai.rs` stays protocol-agnostic.
  - *Rejected: `genai`*, even though it covers all three protocols and more: it is async-only (a tokio
    runtime inside a blocking Tauri app), it moves the wire layer — and therefore the dsh replay
    economics of ADR-0039 — into crate-normalized types, it pulls a second reqwest major (0.13 vs the
    workspace's 0.12), and its 0.7 line is an RC. Provider breadth, its main selling point, is config
    here, not code. *Revisit when* a vendor-native protocol is required (e.g. Gemini without its
    compatibility surface), tool calls generalize across all three protocols, or a fourth protocol
    appears — the seam is designed so `genai` can replace the adapter internals without touching
    `ai.rs`. Also rejected: `rig` (agent framework, ~monthly breaking releases), `async-openai` +
    `claudius` + `ollama-rs` (three crates, three error models, no provider layer).
- **Providers are config; official defaults come from the model catalog.** `[ai] provider` +
  `[ai] model` select the active pair; `[ai.providers.<id>]` blocks override or define endpoints.
  An official provider works with an empty block: base URL, key env names, docs link, and the model
  list with properties come from the catalog (MOE-0025); the protocol comes from a small app-side
  mapping table with a citation per provider (the catalog's `npm` is the hint, hand-checked at
  implementation). Custom providers state `protocol`, `api_url`, and `models` — nothing about them
  can be catalogued.
- **Backward compatibility is free.** The legacy `[ai] base_url` is sugar for an implicit
  `[ai.providers.default]` on `openai-chat`; an existing config.toml and the legacy `api-key` file
  keep working with zero edits.
- **Keys become per-provider** (amends ADR-0019; the module name `keychain.rs` stays): a provider's
  key lives at `data_dir/moe/keys/<provider-id>` (0600). Resolution order per provider: its file →
  the catalog's `env[]` names (official providers) → `MOE_AI_API_KEY` (any provider, last — the
  simple one-key path). The legacy `data_dir/moe/api-key` is the implicit `default` provider's key;
  no migration step. `key <value>` (Moe: Save AI Key) writes the *active* provider's file.
- **Agentic scope: tools land on `openai-chat` only, for now.** An Agentic Conversation (ADR-0039)
  can only be created while the active provider speaks `openai-chat`; other protocols open plain
  conversations — no `tools` in the request, the same mechanics as pre-feature conversations, so no
  DeepSeek 400 trap. Tool deltas for the Anthropic and Responses adapters are the follow-up that
  generalizes the loop; the delta seam reserves the variant from day one.
- **Reasoning and images ride the same item payload.** Anthropic `thinking` blocks and Responses
  reasoning summaries normalize into the existing reasoning payload the UI already renders
  (ADR-0024/MOE-0008); image attachments go out as each protocol's multimodal parts; the
  tag-stripping `split_reasoning` remains the `openai-chat` fallback.
- **Sequencing.** The implementation (MOE-0027) rewrites the same files as MOE-0026 and is blocked
  by it: the refactor wraps the loop's `openai-chat` parser and message-array construction unchanged.

## Config shape (v1)

```toml
[ai]
provider = "deepseek"      # a catalog id, or a [ai.providers.*] key
model = "deepseek-flash"   # required once a provider is chosen

[ai.providers.deepseek]    # optional for official providers: overrides only
# protocol = "openai-chat"              # default from the mapping table
# api_url = "https://api.deepseek.com/v1"
# models = ["deepseek-flash", "deepseek-reasoner"]   # restrict the picker list

[ai.providers.my-gateway]  # custom providers are explicit; same shape as Zed's compatible maps
name = "My Gateway"
protocol = "anthropic"     # openai-chat | openai-responses | anthropic
api_url = "https://gateway.example.com/anthropic"
models = ["claude-sonnet-5-5"]
```

Adapter paths: each adapter appends its canonical path to `api_url` (`/chat/completions`,
`/responses`, `/v1/messages`); the app-side defaults are written accordingly (DeepSeek's
`openai-chat` default is `https://api.deepseek.com/v1`, Anthropic's `anthropic` default is
`https://api.anthropic.com`). OpenAI's default protocol is `openai-chat` in v1 (the tool loop works
against it immediately), with `openai-responses` as an opt-in override until the Responses adapter
carries tools; Google is intended to run through its OpenAI-compatible endpoint (base URL pinned at
implementation with a citation — not verified in the research pass). Local Ollama/LM Studio are
custom providers (`lmstudio` also has a catalog entry; local discovery is out of scope).

## Cost

- Moe owns two new protocol parsers forever (bounded: one builder + one SSE mapper each; the
  reference implementation and its event vocabulary are public — Zed's `anthropic`/`open_ai` crates —
  and this repo already tests streams as fixtures).
- Protocol capability is not uniform in v1: Anthropic/Responses conversations are plain (no tools)
  until the loop generalizes — documented, not a silent degradation.
- The config surface grows by a table and one selector, and the mapping table needs a per-provider
  citation; it will drift as vendors change their compatibility surfaces (it is small, checked in,
  and reviewed like the catalog snapshot).
- Provider switching mid-conversation is not in v1 (one active pair in config); the picker command
  that consumes the catalog comes later.

## Amendment: `hidden_models` — the settings window's picker visibility (MOE-0029)

The settings window's per-model checkboxes needed a place in the config that could not freeze
the daily catalog refresh out of the pickers: writing the complement of a unchecked model into
the hand-writable `models` restriction would pin the list to the catalog of that day. So a
provider block gains **`hidden_models = […]`** — the models subtracted from the effective list
*after* any `models` restriction — written by the comment-preserving section editor like every
other config surgery. An all-visible provider carries no line, and a block that only carried
`hidden_models` is removed again when the list empties. The active pair is unaffected: hiding
the model a conversation runs on only removes it from the pickers, never from the surface that
already uses it.

## Amendment: the tool loop speaks Anthropic too (MOE-0035)

The "openai-chat only" agentic scope is halved: Anthropic-protocol providers (MiniMax's
Claude-Code-compatible surface, Anthropic itself) create and continue Agentic Conversations. The
adapter normalizes the OpenAI-shaped wire intermediate both ways — tool specs to
`{name, description, input_schema}`, assistant rounds to `thinking` + `tool_use` blocks, runs of
`tool` messages to `tool_result` blocks on user messages (results first) — and the shared delta
vocabulary gained Anthropic's `input_json_delta` fragments and a `ReasoningSignature` variant.
The record grows a `reasoning_signature` replay field: MiniMax requires the thinking signature
"unchanged in later turns", and real Anthropic 400s on altered blocks mid-tool-use. MiniMax's
surface whitelists only `thinking.type: disabled|adaptive` (M3.1 rejects even `disabled`, so
reasoning-off must omit the key), making the `thinking` parameter a per-provider dialect:
`adaptive` for `minimax*`, legacy `enabled`+budget for `anthropic`. `openai-responses` remains
plain-conversation-only — its tool generalization is a separate follow-up.
