# M8i: three protocols, many providers — the AI layer learns Anthropic and Responses

- **ID**: MOE-0027
- **State**: done
- **Labels**: ready-for-agent, feature
- **Created**: 2026-10-10
- **Updated**: 2026-10-10
- **Blocked by**: MOE-0026 (done)
- **Assignee**: agent
- **Commit**: 6f29793

The owner's direction: official providers (DeepSeek, OpenAI, Anthropic, Google, xAI, Mistral, Groq,
OpenRouter, Ollama, …) plus custom providers speaking one of three protocols — OpenAI Chat
Completions, OpenAI Responses, Anthropic Messages — with the model list and its properties coming
from the embedded/refreshed catalog (MOE-0025). Decision and rationale: **ADR-0040** (supersedes
ADR-0005's "OpenAI-compatible only" clause; amends ADR-0019 for per-provider key files). Facts:
`docs/research/ai-provider-libraries-rust.md`, `docs/research/ai-model-catalogs.md`,
`docs/research/zed-ai-provider-layer.md`.

**Sequencing:** this issue rewrites the same files as MOE-0026 (`ai_client.rs`, `ai.rs`,
`config.rs`), so it is deliberately **blocked by the tool loop**; the `openai-chat` backend must
keep MOE-0026's SSE tool parsing and message-array construction byte-compatible — the dsh
economics of ADR-0039 (prefix-cache hits, `reasoning_content` echo) depend on the request body
staying exactly as the loop builds it.

## Decisions

- **In-house adapters, three protocols** (ADR-0040): `ai_client` gains a `Protocol` enum —
  `openai-chat` (today's code, unchanged behavior), `openai-responses`, `anthropic` — each a
  request builder + SSE parser in the existing pure-function style, normalizing into one delta
  vocabulary (text / reasoning / tool-call fragment / done). No `genai`/`rig` dependency.
- **Config** (v1): `[ai] provider` + `[ai] model` select the active pair; `[ai.providers.<id>]`
  blocks override official defaults or define custom endpoints (`name`, `protocol`, `api_url`,
  `models`). Legacy `[ai] base_url` is sugar for an implicit `default` provider on `openai-chat` —
  zero-edit backward compatibility. `model` is required once `provider` is set; the error lists the
  provider's available models.
- **Official provider defaults from the catalog**: api base URL, `env[]` key names, docs link, and
  the model list come from `model_catalog`; the protocol comes from a new app-side table (protocol
  + base URL + citation per provider, checked in — the catalog's `npm` field is the hint,
  hand-checked). Adapter paths: `api_url` + `/chat/completions`, `/responses`, `/v1/messages`;
  defaults written accordingly. OpenAI defaults to `openai-chat` in v1 with `openai-responses` as
  an opt-in override until the Responses adapter carries tools.
- **Per-provider keys** (amends ADR-0019): `data_dir/moe/keys/<provider-id>` (0600). Resolution
  per provider: its file → catalog `env[]` names (official providers) → `MOE_AI_API_KEY` (any
  provider, last). The legacy `data_dir/moe/api-key` is the implicit `default` provider's key; no
  migration. `key <value>` / Moe: Save AI Key writes the *active* provider's file.
- **Scope of v1**: streaming text + reasoning + images on all three protocols; **tool calls only on
  `openai-chat`** — Agentic Conversations (ADR-0039) are gated to it, other protocols open plain
  conversations (no `tools` sent, same as pre-feature conversations). AI Commands stay tool-free.
- **Non-goals**: OAuth/subscription auth (ChatGPT/Claude plan logins), Bedrock/Vertex native auth,
  provider-switch commands or picker UI (config-file driven, ADR-0009), local Ollama discovery,
  non-OpenAI tool calls (follow-up).

## Delivery

- `crates/moe-extensions/src/ai_client.rs` → a protocol module tree: `openai_chat.rs` (moved
  as-is, tool parsing intact), `openai_responses.rs`, `anthropic.rs`, `providers.rs` (the checked-in
  protocol/base-URL/citation table), dispatch + shared delta vocabulary; recorded-stream fixtures
  per protocol.
- `crates/moe-extensions/src/ai.rs` (+ `ai_commands.rs`): resolve provider + model → adapter;
  replace the `config.ai.base_url` / `model_or_default()` call sites (Quick Ask, Side View, AI
  Commands invoke); gate Agentic Conversations to `openai-chat`.
- `crates/moe-platform/src/config.rs`: provider schema, legacy sugar, validation errors.
- `crates/moe-platform/src/keychain.rs`: per-provider paths + resolution order + legacy file.
- Docs at delivery (MOE-0012 pattern): README Config section (provider examples per protocol),
  CONTEXT.md terms (**Provider**, **Protocol**), config sample comments.

## Acceptance

- Existing config.toml (`base_url` + `model`) and the legacy `api-key` file work unchanged —
  same behavior as today end to end (Quick Ask, Side View, AI Commands).
- `provider = "deepseek"`, `model`, and a key — no provider block: chat streams using catalog
  defaults; a wrong model name errors with the provider's model list; a missing key names the
  expected file path.
- A custom provider on each of the three protocols streams text; Anthropic `thinking` renders in
  the Thinking block; Responses reasoning summaries do too; image attachments go out as multimodal
  parts on all three.
- Per-provider keys: switching `[ai] provider` uses that provider's key file; `MOE_AI_API_KEY` is
  the last-resort fallback; `key <value>` writes the active provider's file.
- `[ai] provider` on `anthropic`/`openai-responses`: new conversations are plain — a
  request-capture test proves no `tools` field is sent; `openai-chat` keeps MOE-0026's agentic
  behavior and all of its tests stay green.
- `cargo test` green; no regression in the AI Commands (tool-free) path.

## References

- **ADR-0040** (this work's decision), ADR-0039 (tool loop, the blocker), ADR-0005 + its two
  amendments, ADR-0019 (amended for per-provider keys), MOE-0025 (catalog).
- Research: `docs/research/ai-provider-libraries-rust.md`, `docs/research/ai-model-catalogs.md`,
  `docs/research/zed-ai-provider-layer.md`.

## Comments

- 2026-10-10 (agent): filed (initially numbered MOE-0018; see the last comment for the final
  renumbering after the rebase onto main). `Blocked by: MOE-0026` — same files, and the openai-chat wire behavior must not move underneath
  it; the `Blocks` line was added to MOE-0026's header. Design specifics (config shape, per-provider
  keys, tools gating) are proposals in ADR-0040 from the owner's three-protocol direction — amend
  via comments here if any should change before implementation starts.
- 2026-10-10 (agent): **done** (10e0917; schema + keys in 18fa99e). `ai_client` is a module tree:
  `mod.rs` (shared delta vocabulary + `ChatRequest` + dispatch), `openai_chat.rs` (moved as-is,
  bodies byte-identical — all MOE-0026 tests green), `anthropic.rs` / `openai_responses.rs` (pure
  adapters: system→`system`/`instructions`, multimodal parts, the reasoning opt-in, SSE
  normalization, blocking-response text, with recorded-fixture tests; two subagents landed them in
  parallel), `providers.rs` (the citation table + `resolve`/`resolve_with`). `config.rs` gained
  `[ai] provider` + `[ai.providers.<id>]` (legacy `base_url` = implicit `default`); `keychain.rs`
  per-provider files + the resolution order; `ai.rs`/`ai_commands.rs` resolve per invoke and gate
  the tool loop to `openai-chat`; `moe.rs` saves the active provider's key. End-to-end
  request-capture tests prove the acceptance wire facts (Anthropic: `/v1/messages`, `x-api-key` +
  version, no `tools`; Responses: `/responses`, bearer, `store: false`, summaries, no `tools`).

  Five implementation notes that refine ADR-0040's letter (recorded here as the decision log):
  (1) `Protocol` lives in `moe-platform::config` (the config vocabulary owns it; the adapters
  import it) rather than in `ai_client`, so parse errors surface at config load. (2) A provider
  block's optional `reasoning` overrides the reasoning opt-in (Anthropic `thinking` / Responses
  summaries); unset = the catalogued model's `reasoning` flag for official ids, off for custom
  ones. (3) Unknown model names are **not** rejected locally: catalog lists are volatile
  (OpenRouter, local LM Studio), so a 4xx card appends the provider's known models
  (`ActiveProvider::model_hint`, plumbed through the stream requests) instead of blocking a model
  the endpoint would accept. (4) `continue_agentic` refuses to continue a stored agentic record
  under a non-openai-chat provider (a protocol switch would break the Tool Call replay) with
  guidance to switch back or start a new chat. (5) Anthropic `max_tokens` comes from the catalogued
  output limit clamped to 8192; the `thinking` budget is half of it clamped to 1024..=4096 and is
  omitted when no legal budget exists.

  Validation: `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo test
  --workspace` green (app 5, core 50, extensions 95, platform 64); `pnpm -C ui build` green (no UI
  change needed). Manual acceptance against live endpoints (real provider keys, image attachments,
  per-provider key switching) still needs the owner; the legacy path was verified by the resolve
  and keychain tests (base_url + legacy `api-key` resolve to the implicit `default` provider).
- 2026-10-10 (agent): renumbered to **MOE-0027 / M8i** after rebasing this branch onto main (main
  had concurrently allocated its own MOE-0016–0024 to the UI workstream); the sweep covered the
  ADRs, the research notes, code comments, and the config template. The rebase also rewrote the
  delivery commits: `10e0917` → `6f29793` (the adapter tree) and `18fa99e` → `29ff372` (schema +
  keys); the shas in the comments above predate the rebase.
