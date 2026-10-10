# Rust libraries for multi-provider AI clients

Research note, 2026-10-09. Every claim below cites a primary source (official docs or source code) fetched on that date; URLs are in [Sources](#sources). Claims not verified against a live primary source are marked in [Verification notes](#verification-notes).

## Context

Moe is a keyboard-first desktop command palette (Tauri 2, Rust backend, vanilla TS UI). Its AI feature is an extension that streams answers from a single OpenAI-compatible endpoint:

- `crates/moe-extensions/src/ai_client.rs` builds `POST {base_url}/chat/completions` bodies and parses `data:`-prefixed SSE lines by hand — pure, unit-tested functions (`sse_delta`, L14–29; `chat_completions_url`, L31–33; `chat_body_*`, L35–59). It ignores anthropic- and Responses-style event frames entirely.
- `crates/moe-extensions/src/ai.rs` runs the HTTP request with blocking `reqwest` 0.12 on a worker thread (`run_stream`, L422–526), emits `CommandEvent::ItemUpdated` per chunk, and strips model-emitted reasoning wrappers (`<thinking>`, `…`, etc.) from accumulated text (`split_reasoning`, L392–418). History, attachments (ADR-0010), persistence, stop flags, and the keychain key handoff are all app-owned.
- `crates/moe-platform/src/config.rs` stores `AiConfig { base_url: Option<String>, model: Option<String> }` (L100–103).
- ADR-0005 records the current decision — OpenAI-compatible only, one endpoint — and this note incubates its replacement.

The goal being researched: official providers (DeepSeek, OpenAI, Anthropic, Google, xAI, Mistral, Groq, OpenRouter, Ollama/local, …) plus user-defined custom providers speaking one of three protocols — OpenAI Chat Completions, OpenAI Responses, Anthropic Messages — with streaming, multi-turn history, image attachments, reasoning extraction, later tool calling, per-provider API keys, and custom base URLs.

## Findings

Baseline compatibility test used throughout: none of today's `ai_client.rs` code would work unchanged for Anthropic Messages (event-typed SSE, `max_tokens` required, top-level `system`) or OpenAI Responses (event-typed SSE, different delta paths) — this is the gap any candidate must close.

| Crate | Latest release (date) | License | Protocols spoken | Custom OpenAI base URL | Streaming | Runtime | Main blocker for Moe |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `genai` | 0.7.0-rc.4 (2026-10-07); latest stable 0.6.5 (2026-06-06) | MIT OR Apache-2.0 | Chat Completions, Responses (`openai_resp`), Anthropic Messages, Gemini native, Ollama native, + 20 more adapters | Yes — `genai_{n}::` adapter via `GENAI_{n}_ENDPOINT`/`GENAI_{n}_API_KEY`, or `ServiceTargetResolver` | Yes (`exec_chat_stream`) | async, tokio `rt-multi-thread`; reqwest **0.13** | 0.7 in RC; pulls a second reqwest major; needs a runtime |
| `rig` / `rig-core` | 0.44.0 (2026-10-07) | MIT | Chat Completions, Responses, Anthropic Messages, Gemini, + ~25 provider modules | Yes — `OpenAIConfig::with_base_url` / `with_dialect` | Yes (streaming completion + agent runtime) | async, tokio; `rig-core` has no bundled transport since 0.44 (`rig-reqwest` does) | Agent-framework scope; breaking changes nearly monthly |
| `llm` (graniet) | 1.3.8 (2026-04-19) | MIT | Chat Completions, Anthropic Messages, Gemini, Ollama, Cohere, … (feature-gated) | Not verified | Yes (per-provider streaming examples) | async, tokio | Slower cadence; default features pull a CLI/ratatui stack |
| `async-openai` | 0.42.1 (2026-09-28) | MIT | Chat Completions, Responses | Yes (`OPENAI_BASE_URL`, `Config`, per-request path/header overrides) | Yes (SSE) | async, tokio-adjacent (reqwest) | OpenAI-only; no Anthropic Messages |
| `claudius` | 0.33.0 (2026-06-10) | Apache-2.0 | Anthropic Messages | n/a | Not verified | reqwest (TLS features); async unverified | Single provider |
| `ollama-rs` | 0.3.6 (2026-07-24) | MIT | Ollama native API | n/a | Yes (`stream` feature) | async, tokio | Local Ollama only |
| `misanthropy` | 0.0.8 (2025-06-08) | — | Anthropic | n/a | — | — | Unmaintained (see below) |
| `anthropic` (abdelhamidbakhta) | 0.0.8 (2024-09-03) | — | Anthropic | n/a | — | — | Unmaintained (see below) |

### `genai` — jeremychone/rust-genai

- **What it is.** "A Native-Protocol Multi-AI Provider Library for Rust … Over 200+ LLM models, 27+ LLM providers out of the box" (README). Repo has 901 stars and 1,096 commits; edition 2024; `unsafe_code = "forbid"` (`Cargo.toml`).
- **History.** Created 2024-06-01; 146 published versions. Latest stable **0.6.5** published 2026-06-06; the 0.7 line is in RC, latest **0.7.0-rc.4** published 2026-10-07; main is at `0.7.0-rc.5-WIP` as of 2026-10-09 (crates.io API; `Cargo.toml` at `main`).
- **Protocol coverage.** Out-of-the-box adapters include `openai`, `openai_resp` (OpenAI Responses), `anthropic`, `gemini`, Ollama/Ollama Cloud, DeepSeek, Mistral, xAI, grok-family, Groq, Cohere, together, fireworks, `open_router`, vertex, Bedrock, GitHub Copilot, and more (README provider list). v0.6.0 notes list "OpenAI Responses stateful sessions" (`previous_response_id`, `store`) and `open_router` as a v0.6 addition.
- **Custom endpoints.** Two mechanisms: a generic `genai_{n}::` custom adapter configured by `GENAI_{n}_ENDPOINT` / `GENAI_{n}_API_KEY` (v0.7.0-beta notes; README), and `ServiceTargetResolver` for "any other providers" / "custom auth, endpoint, and model" (`examples/c06-target-resolver.rs`, README). Custom base URLs are also settable for the OMLX adapter via `OMLX_ENDPOINT`.
- **Streaming/tools/images/reasoning.** Streaming: `client.exec_chat_stream` (README example). Function calling: yes; v0.6 adds a provider-neutral `ChatOptions::with_tool_choice` and typed built-in tools incl. WebSearch. Images: "Image analysis (for OpenAI, Gemini Flash-2, Anthropic)" plus `examples/c07-image.rs`. Reasoning: v0.6 refines `ReasoningContent`/`StopReason` including `ContentPart::ReasoningContent`; Mistral "typed thinking chunks are normalized to reasoning content"; reasoning effort can even be a model-name suffix (`gpt-5.4-mini-high`) and Anthropic uses `ReasoningEffort::Max` (README, v0.6.0 notes).
- **Model naming/listing.** Model names resolve to adapters by prefix (`claude-*`, `gemini-*`, `grok-*`, …) or can be forced with `adapter::model` namespacing (README "Model to Adapter Resolution"). v0.6.0 added model listing: `Client::all_model_names(adapter_kind, provider_config)` (README/CHANGELOG). Per-model *capability* metadata is not documented in the README — see [Verification notes](#verification-notes).
- **Footprint.** Async-only; `tokio` with `macros`, `rt-multi-thread` features; `reqwest = "0.13"` with `rustls-tls` default forwarding to `reqwest/rustls` + aws-lc-rs provider + OS trust store; `eventsource-stream` in 0.6.x (dropped in 0.7.0-rc.4). MoE is on reqwest **0.12** with `rustls-tls-native-roots`, so adoption creates a two-major reqwest graph unless Moe upgrades reqwest (`genai` `Cargo.toml` at `v0.6.5` and `v0.7.0-rc.4`; `moe` workspace deps).

### `rig` — 0xPlaygrounds/rig (`rig`, `rig-core`)

- **What it is.** "Rust library for building scalable, modular, and ergonomic LLM-powered applications" with agents, hooks, memory, vector stores, and "20+ model providers, all under one singular unified interface" (README). 8,838 stars; MIT; repo pushed 2026-10-09.
- **History.** 0.39.0 (2026-06-19) → 0.40.0 (2026-07-11) → 0.41.0 (2026-07-28) → 0.42.0 (2026-08-17) → 0.43.0 (2026-09-30) → **0.44.0 (2026-10-07)** (GitHub releases API). The README warns: "future updates **will** contain **breaking changes**"; the 0.41–0.44 release bodies list dozens of `[breaking]` items (e.g. split into `rig-core` + `rig-agent`, transport extracted into `rig-reqwest`, `OneOrMany` → `Vec`, `#[non_exhaustive]` removal).
- **Protocol coverage.** Provider modules at HEAD (2026-10-09) include `openai` (both wires), `anthropic`, `gemini`, `deepseek`, `groq`, `mistral`, `ollama`, `openrouter`, `xai`, `azure`, `chatgpt`, `copilot`, `together`, `venice`, `llamacpp`, `minimax`, `moonshot`, `perplexity`, `zai`, `xiaomimimo`, plus `bedrock`/`vertexai` companion crates (`crates/rig-core/src/providers` listing; README).
- **Protocols.** OpenAI Chat Completions and Responses are two `Route`s on one OpenAI wire ("OpenAI itself, xAI and ChatGPT serve `/responses` as their primary API"; `Route::Chat` remains available) — `providers/openai/wire.rs`. Anthropic Messages is a native provider; Anthropic-dialect gateways share `AnthropicCompatibleProvider` (release notes 0.42/0.44).
- **Custom endpoints.** `OpenAIConfig` holds `api_key`, `base_url` ("The base URL every path resolves against"), and a `Dialect`; `with_base_url(...)` overrides it and `Dialect::gateway(name, base_url, api_key_env)` builds a dialect — `providers/openai/wire.rs`, verified on `main` 2026-10-09.
- **Features.** Streaming completion + agent loop (README features); tool calling and structured output are core; images ride content parts (`reads_images` gate in `providers/openai/mod.rs`; 0.44 notes "images ride the regrouped stream"; 0.42 notes Anthropic strict tool use). Reasoning: release notes across 0.41–0.44 map DeepSeek `reasoning_content`, Anthropic `thinking_tokens` → `reasoning_tokens`, OpenAI Responses reasoning metadata, and OpenRouter reasoning share. Model listing: `ModelListingClient::list_models()` with `context_length`/`max_output_tokens` fields for OpenRouter, plus listing for OpenAI/Anthropic/Gemini/Groq/Mistral/Moonshot/MiniMax (release notes 0.42/0.44).
- **Footprint.** Async (README examples use `#[tokio::main]`). 0.44 "cut the bundled transport into its own crate; rig-core has no default transport and no reqwest/tokio" (release notes v0.44.0) — the `rig` facade plus `rig-reqwest` provides reqwest. Browser wasm is supported for the portable core.

### `llm` — graniet/llm

- **What it is.** "A Rust library unifying multiple LLM backends" with "a unified API and builder style" (README). 365 stars; MIT; repo last pushed 2026-06-06; v1.3.8 published 2026-04-19 (crates.io).
- **Coverage.** Feature-gated backends: OpenAI, Anthropic, Ollama, DeepSeek, xAI, Phind, Groq, Google Gemini, Cohere, Mistral, Hugging Face, ElevenLabs, OpenRouter, Azure OpenAI, Bedrock (`full` feature list, crates.io 1.3.8 metadata; README). Anthropic streaming, OpenAI streaming, Google streaming, and xAI streaming have dedicated examples (README example table).
- **Features.** Chat + completion traits; function calling; vision; reasoning (with `openai_reasoning` and `anthropic_thinking` examples); structured output; memory with trimming; agents; evaluation; model listing (`model_listing_example`); retries; a REST-API mode (README).
- **Footprint.** Async (`#[tokio::main]` in README example); reqwest with `default-tls` by default, `rustls-tls` opt-in feature; **default features include `cli`**, which pulls `ratatui`, `crossterm`, `syntect`, `portable-pty`, etc. (crates.io 1.3.8 feature table) — a consumer must use `default-features = false` plus the provider features it wants.
- **Gaps.** No GitHub releases exist (404 on `releases/latest`; versioning lives on crates.io). Custom base-URL support was not verified this pass.

### `async-openai`

- **What it is.** "Unofficial Rust library for OpenAI, based on OpenAI OpenAPI spec" (README). 2,018 stars; MIT; repo pushed 2026-09-28; **v0.42.1 published 2026-09-28** (GitHub releases API).
- **Coverage.** Chat Completions (`chat-completion` feature) and the Responses API (`responses` feature) incl. streaming events (README feature table). SSE streaming, retries with exponential backoff, granular feature flags, tower middleware. OpenAI-compatible providers are supported via `OPENAI_BASE_URL`, global `Config`/`OpenAIConfig`, per-request `path()`/`query()`/`header()` overrides, and a "bring your own types" (`byot`) mode that accepts `serde_json::Value` in and out (README "OpenAI Compatible Providers").
- **Not covered.** No Anthropic Messages or Gemini support by design; per-provider abstraction is out of scope. Reasoning-field handling was not verified this pass (see notes). Vision: the crate covers the official OpenAI APIs (images, audio, etc.), but image *input* handling was not verified in this pass.
- **Footprint.** Async. TLS default `rustls` (aws-lc-rs bundled), alternatives `rustls-no-provider`, `native-tls`, `native-tls-vendored` (README).

### Anthropic-only clients

There is **no official Anthropic Rust SDK**. Anthropic's own SDK page lists "Python, TypeScript, C#, Go, Java, PHP, and Ruby" only (docs.anthropic.com "SDKs, CLI, and libraries", fetched 2026-10-09). Third-party crates, by maintenance status:

| Crate | Latest (date) | Repo activity | License | Notes |
| --- | --- | --- | --- | --- |
| `claudius` (rescrv) | 0.33.0 (2026-06-10) | Repo pushed 2026-09-30; 17 stars | Apache-2.0 (crates.io) | Most actively developed Anthropic-only crate found; ships CLI binaries in the same crate |
| `threatflux-anthropic-sdk` | 0.4.1 (2026-10-07) | New (first published 2026-06-24), trustpub-only | Not checked | Very fresh; not deep-verified |
| `anthropic-ai-sdk` (katsuhirohonda) | 0.2.27 (2026-01-11) | Last publish 2026-01-11 | Not checked | 52 versions; description "unofficial" |
| `anthropic-sdk-rust` (dimichgh) | 0.1.1 (2025-06-11) | Stale | Not checked | Two versions only |
| `misanthropy` (cortesi) | 0.0.8 (2025-06-08) | Stale | — | Last touched over a year ago |
| `anthropic` (abdelhamidbakhta) | 0.0.8 (2024-09-03) | Abandoned (2024) | — | crates.io metadata even points `repository` at a misspelled repo path |

Sources: crates.io search results for `anthropic` and `misanthropy`, plus the specific-version endpoints (links in [Sources](#sources)). Feature-level claims (streaming, thinking blocks, tool use) for these crates were not verified this pass.

### `ollama-rs`

- **What it is.** "A simple and easy-to-use library for interacting with the Ollama API" — native Ollama protocol, not OpenAI-compat. 1,062 stars; MIT; repo pushed 2026-10-09; **v0.3.6 published 2026-07-24**.
- **Coverage.** Chat with history (`ChatHistory`), completion + chat streaming behind the `stream` feature, embeddings (single + batch), model listing (`list_local_models`), `show_model_info`, model create/copy/delete, function calling (`function` macro + `Coordinator`), and thinking via `.think(true)`. Custom host/port via `Ollama::new`/builder (README). Async, tokio.
- **Fit.** Relevant only as the *local Ollama* leg of a multi-provider layer, or not at all if Ollama is addressed through its OpenAI-compatible endpoint. Image input in messages was not verified this pass.

### Other multi-provider crates surfaced by search (not deep-verified)

These appeared in crates.io searches while looking for maintained options; only version/date/description metadata is verified, nothing else:

| Crate | Latest (date) | Description (crates.io) |
| --- | --- | --- |
| `brazen` | 0.0.18 (2026-09-06) | "A stateless, swiss-army-knife adapter for every LLM provider and protocol." |
| `open-agent-sdk` | 0.11.5 (2026-09-29) | Two wire protocols: OpenAI chat completions and Anthropic Messages; streaming, tools |
| `llm-bridge-core` | 0.5.0 (2026-07-01) | "Protocol transform library for LLM API translation between Anthropic and OpenAI." |
| `motosan-ai` | 0.27.1 (2026-07-28) | "Rust SDK for multi-provider AI chat" |
| `llmrust` | 0.1.4 (2026-09-12) | "Unified Rust interface for 7 LLM providers … with HTTP proxy" |

`brazen` and `llm-bridge-core` are interesting because they treat protocol translation as the product, which is exactly the hardest part of Moe's stated goal; none of these were evaluated for correctness, streaming behavior, or maintenance beyond the metadata above.

### Framework crates adjacent to the client layer

`swiftide` (bosun-ai, 791★, MIT, pushed 2026-10-08, "Fast, streaming indexing, query, and agentic LLM applications in Rust"), `langchain-rust` (Abraxas-365, 1,349★, MIT, pushed 2026-10-08), and `kalosm` (floneum, 2,229★, Apache-2.0, pushed 2026-10-05, "Instant, controllable, local pre-trained AI models in Rust") are RAG/indexing/agent or local-inference frameworks. They would sit *around* a client layer (and several depend on one) rather than replace `ai_client.rs`; none is a candidate for the client seam itself (repo descriptions + metadata, GitHub API).

### Adjacent approaches

**OpenRouter as an aggregator** keeps Moe's architecture nearly unchanged: point `AiConfig.base_url` at `https://openrouter.ai/api/v1` and keep the existing hand-rolled client. OpenRouter's quickstart documents one `/api/v1/chat/completions` endpoint for "hundreds of AI models", streaming, an OpenAI SDK drop-in mode, and `GET /api/v1/models` for listing. The trade-offs: requests and responses stay in OpenAI Chat Completions format — so Anthropic *native* thinking blocks, Responses-API statefulness, and vendor-specific fields are provider-normalized away or reachable only through OpenRouter's own extensions; the user needs an OpenRouter account/key; and local Ollama still needs a second endpoint anyway. It buys breadth without buying protocol coverage.

**LiteLLM proxy (Python)** would move all provider logic out of the app into a locally-run gateway: LiteLLM's repo describes an "AI Gateway … call 100+ LLM APIs in OpenAI (or native) format" (description fetched 2026-10-09; repo licence reported as non-standard by the GitHub API). Moe would then only ever speak OpenAI-compatible HTTP to `localhost`, keeping `ai.rs` untouched, at the cost of shipping/supervising a Python service (or its newer Rust core) with the desktop app — a heavyweight dependency for a Tauri utility. **Vercel AI SDK (TS)** is the same move in the other direction: it is a TypeScript toolkit with provider packages, streaming, and agent harnesses ("AI SDK by Vercel", ai-sdk.dev) — using it would mean building the AI client into the frontend or a Node sidecar, which contradicts the Rust-backend design and still leaves keys/history split across processes.

## Fit for Moe

Leading options, in order of how directly they replace today's code:

1. **`genai` — closest single-crate replacement.** It is the only crate checked that covers all three required protocols (Chat Completions, Responses, Anthropic Messages) plus Ollama/local and the named official providers, with streaming, images, tool calling, reasoning normalization, model-name routing, per-adapter auth, and two documented custom-endpoint paths. Migration shape:
   - Replaces: `sse_delta`, `chat_completions_url`, `chat_body*` (`ai_client.rs` L14–59) — request building and SSE parsing for every protocol; model→provider resolution; per-provider auth resolution (`AuthResolver`/`ServiceTargetResolver`).
   - Keeps in Moe: the worker-thread execution model (host a current-thread tokio runtime on the existing stream thread and drive the crate's stream there), `CommandEvent::ItemUpdated` emission, stop flags (`ACTIVE` streams), persistence/titles, history, attachment loading, and keychain handling. `StreamRequest` (`ai.rs` L235–244) changes from `{base_url, key, body}` to a provider/model/config descriptor plus the same emitter/persist closures.
   - `split_reasoning` (L392–418) becomes a fallback only: normalized reasoning would arrive as structured content, but MoE should keep tag-stripping for OpenAI-compatible endpoints that still embed `<thinking>` in text.
   - Trade-offs: async-only (needs a runtime — acceptable on the existing worker thread, or convert streaming commands to async Tauri commands); genai depends on reqwest **0.13** while Moe uses 0.12 — either two reqwest majors in the graph or a workspace upgrade; default TLS provider is aws-lc-rs (C/assembly build) unless features are trimmed; adoption of 0.7 today means an RC, with 0.6.5 as the stable fallback missing some 0.7 provider work (`genai` manifests).
2. **Extend the hand-rolled client with a protocol enum.** Add `ChatCompletions | Responses | AnthropicMessages` request builders and three SSE parsers, keeping the existing tested-function style and the blocking worker thread. Smallest dependency risk and full control over quirks (Anthropic `max_tokens` required, top-level `system`, event-typed SSE; Responses `output_text.delta` paths). Costs: MoE then owns every provider quirk and reasoning format forever, gets no model metadata/listing, and the three-protocol matrix is exactly where one-off bugs (ADR-0005's original rationale) recur.
3. **`async-openai` + `claudius` (or similar) + `ollama-rs`.** Highest spec fidelity per protocol and all crates are async; but three crates, three error models, three feature systems, and no unified model/provider layer — a bigger integration surface than a single multi-provider crate.
4. **`rig`.** Technically capable of everything required and very actively developed, but it is an agent framework: agents, hooks, vector stores, memory policies, MCP, a cassette test system, and a transport split that are all dead weight here, plus a stated breaking-change policy with releases roughly monthly (0.39 → 0.44 in under four months). Its value would only appear if Moe later wants a full tool-calling agent runtime.

**Recommendation.** Adopt `genai` as the client layer, most realistically at its stable 0.6.5 line first (or 0.7 once out of RC), behind a thin in-repo adapter that preserves the current seam: a single `run_stream`-shaped function that takes a provider/model/key descriptor and the existing `item_of`/`persist`/`on_done` closures. That concentrates the replacement in `ai_client.rs` plus the top of `ai.rs`, keeps history/persistence/attachments/keychain untouched, and eliminates the protocol-parsing work that motivated this research. Keep the current hand-rolled path as a fallback feature flag until 0.7 stabilizes. If the second reqwest major or the RC timing is judged unacceptable, option 2 is the honest alternative — the existing pure-function design was built for exactly that expansion.

Explicit trade-offs against ADR-0005's replacement:

- Adopting any crate makes provider configuration a first-class app concern (provider id per conversation, key per provider in the keychain, base URL defaults per provider) — `AiConfig` (`config.rs` L100–103) grows from two optional strings to a provider list.
- Anthropic via any crate still requires the app to supply `max_tokens` semantics — genai defaults to 1024 per its README's `ChatOptions` table, which differs from today's "whatever the endpoint defaults to" behavior.
- Protocol quirks the app stops carrying (event-typed SSE for Anthropic/Responses; Anthropic's required `max_tokens`; per-provider reasoning fields) transfer to the crate's maintainers — and the crate's release cadence becomes a dependency of Moe's.

## Sources

All URLs fetched 2026-10-09.

**genai**
- Repo + README: https://github.com/jeremychone/rust-genai , https://raw.githubusercontent.com/jeremychone/rust-genai/main/README.md
- Manifests: https://raw.githubusercontent.com/jeremychone/rust-genai/v0.6.5/Cargo.toml , https://raw.githubusercontent.com/jeremychone/rust-genai/v0.7.0-rc.4/Cargo.toml , https://raw.githubusercontent.com/jeremychone/rust-genai/main/Cargo.toml
- crates.io: https://crates.io/api/v1/crates/genai

**rig**
- Repo + metadata: https://github.com/0xPlaygrounds/rig , https://api.github.com/repos/0xPlaygrounds/rig
- Releases: https://api.github.com/repos/0xPlaygrounds/rig/releases?per_page=6
- README: https://raw.githubusercontent.com/0xPlaygrounds/rig/main/README.md
- Source: https://api.github.com/repos/0xPlaygrounds/rig/contents/crates/rig-core/src/providers , https://raw.githubusercontent.com/0xPlaygrounds/rig/main/crates/rig-core/src/providers/openai/mod.rs , https://raw.githubusercontent.com/0xPlaygrounds/rig/main/crates/rig-core/src/providers/openai/wire.rs

**llm (graniet)**
- Repo + metadata: https://github.com/graniet/llm , https://api.github.com/repos/graniet/llm
- README: https://raw.githubusercontent.com/graniet/llm/main/README.md
- crates.io: https://crates.io/api/v1/crates?q=llm&per_page=5 , https://crates.io/api/v1/crates/llm/1.3.8

**async-openai**
- Repo + metadata: https://github.com/64bit/async-openai , https://api.github.com/repos/64bit/async-openai
- README: https://raw.githubusercontent.com/64bit/async-openai/main/async-openai/README.md
- Release: https://api.github.com/repos/64bit/async-openai/releases/latest

**Anthropic-only clients**
- Official SDK list: https://docs.anthropic.com/en/api/client-sdks
- crates.io searches: https://crates.io/api/v1/crates?q=anthropic&per_page=25 , https://crates.io/api/v1/crates?q=misanthropy&per_page=10
- claudius: https://crates.io/api/v1/crates/claudius/0.33.0 , https://github.com/rescrv/claudius

**ollama-rs**
- Repo + metadata: https://github.com/pepperoni21/ollama-rs , https://api.github.com/repos/pepperoni21/ollama-rs
- README: https://raw.githubusercontent.com/pepperoni21/ollama-rs/master/README.md
- Release: https://api.github.com/repos/pepperoni21/ollama-rs/releases/latest

**Frameworks**
- swiftide: https://api.github.com/repos/bosun-ai/swiftide
- langchain-rust: https://api.github.com/repos/Abraxas-365/langchain-rust
- kalosm: https://api.github.com/repos/floneum/floneum

**Adjacent approaches**
- OpenRouter quickstart: https://openrouter.ai/docs/quickstart
- LiteLLM: https://api.github.com/repos/BerriAI/litellm
- Vercel AI SDK: https://api.github.com/repos/vercel/ai , https://ai-sdk.dev/docs/introduction

**Repo files (Moe)**
- `crates/moe-extensions/src/ai_client.rs`, `crates/moe-extensions/src/ai.rs`, `crates/moe-platform/src/config.rs`, `docs/adr/0005-openai-compatible-endpoints-only-local-history.md`

## Verification notes

- **Fetch failures.** `https://api.github.com/repos/graniet/llm/releases/latest` returns 404 — that repo has no GitHub releases, so `llm` version/date data comes from crates.io only. `https://raw.githubusercontent.com/64bit/async-openai/main/README.md` is a pointer file ("async-openai/README.md"); the real README was fetched from the subdirectory. No other source in [Sources](#sources) failed.
- **genai 0.7 vs 0.6.** The genai README on `main` describes the 0.7 line (it says "Current release genai = 0.6", then documents v0.7.0-beta changes). Feature claims attributed to v0.6.0 above are taken from that README's "v0.6.x Released" section; per-API details (exact stream event names, `ServiceTargetResolver` signature) were not re-verified against docs.rs for 0.6.5 specifically. `reqwest = "0.13"` is verified in both the `v0.6.5` and `v0.7.0-rc.4` manifests as of the fetch.
- **rig source is HEAD, not the 0.44.0 release tag.** `providers/openai/wire.rs` and the provider listing were fetched from `main` (which is post-0.44.0); `OpenAIConfig`/`with_base_url`/`Dialect::gateway` are verified at HEAD and are consistent with v0.44.0 release notes, but the exact API of the published 0.44.0 crate was not diffed against the tag.
- **Unverified claims, by crate.**
  - `genai`: per-model capability metadata (context windows etc.) is not documented in the fetched README; embeddings support is not claimed in the README's stated focus ("text chat, vision, and function calling APIs") and was not checked further.
  - `rig`: the exact model-listing capability matrix per provider is taken from release notes, not source; browser-wasm support for the whole `rig` facade vs only `rig-core`/portable parts is from README wording.
  - `llm`: custom OpenAI-compatible base-URL support was not verified; reasoning/image/tool claims rest on README bullets and example names.
  - `async-openai`: reasoning-field extraction and image-input handling were not verified; only the feature table (Responses, Chat Completions, streaming) and BYOT/base-URL mechanisms are verified.
  - Anthropic-only crates: only version/date/activity/license metadata is verified; no feature-level verification (streaming, extended thinking, tool use) was done for `claudius`, `threatflux-anthropic-sdk`, `anthropic-ai-sdk`, or the stale crates. `threatflux-anthropic-sdk` publishes via crates.io trusted publishing and is two months old — treat as unproven.
  - `ollama-rs`: image input in chat messages not verified; streaming requires the non-default `stream` feature per README.
  - Other crates table (`brazen`, `open-agent-sdk`, `llm-bridge-core`, `motosan-ai`, `llmrust`): metadata only; no code, docs, or license verification; the crates.io search snippets are marketing text.
- **Metadata caveat.** `claudius` has no LICENSE file visible in its GitHub repo metadata (`license: null` on the GitHub API), while crates.io reports `Apache-2.0` for the published crate; the discrepancy is unresolved.
- **Bench/behavior claims.** Nothing in this note was executed, compiled, or benchmarked; all statements are documentation/source reads. Any performance or ergonomics statement ("heavier", "dead weight") is an assessment, not a measurement.
