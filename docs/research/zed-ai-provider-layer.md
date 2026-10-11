# Zed's AI provider layer: openness, licensing, and reuse in Moe

Research note, 2026-10-09. Every claim below cites a primary source (repository files or official docs) fetched on that date and pinned to commit `129c68ff4bc136b3fac2557388321374b4e5c858`; URLs are in [Sources](#sources). Claims not verified against a live primary source are marked in [Verification notes](#verification-notes).

## Context

Moe is a keyboard-first desktop command palette (Tauri 2, Rust backend, vanilla TS UI). Its AI feature today speaks one OpenAI-compatible endpoint: `AiConfig { base_url: Option<String>, model: Option<String> }` (`crates/moe-platform/src/config.rs`, L100–103; default model `gpt-4o-mini`, L113–118), with hand-rolled request bodies and `data:`-line SSE parsing in `crates/moe-extensions/src/ai_client.rs` per ADR-0005. The target end-state — official providers plus user-defined custom providers on three wire protocols (OpenAI Chat Completions, OpenAI Responses, Anthropic Messages) with streaming, images, reasoning and later tools — is exactly the problem the Zed editor solves in the open, in Rust.

Zed is the reference implementation candidate: provider abstraction, per-provider HTTP clients, protocol event mapping, agent, and settings all live in the public `zed-industries/zed` repository (default branch `main`; HEAD at fetch time `129c68ff4bc136b3fac2557388321374b4e5c858`, committed 2026-10-09T14:06:54Z). This note establishes what is open, under which licenses, whether any of it is published, and what a Tauri/Rust app with **no LICENSE file at its root** (verified 2026-10-09 by root directory listing) may legally and practically do with it.

Companion notes: [`ai-provider-libraries-rust.md`](./ai-provider-libraries-rust.md) (standalone Rust client crates; concludes `genai` is the leading choice) and [`ai-model-catalogs.md`](./ai-model-catalogs.md) (model metadata; concludes models.dev).

## Findings

### What is open

All of the client-side AI layer is in the public repo — provider abstraction, per-provider HTTP clients, agent crates, model definitions, and the settings schema; no part of the client path described here is closed-source. Per-directory GitHub contents listings at HEAD show the crate layout. The relevant crates:

**Abstraction and aggregation**

| Crate (path) | Purpose (verified from source fetched at the pinned commit) | License |
| --- | --- | --- |
| `language_model_core` (`crates/language_model_core`) | Protocol-agnostic core: completion stream type and event enum (`Text`, `Thinking`, `RedactedThinking`, `ToolUse`, `UsageUpdate`, `Compaction`, `Started`, `Stop`, …), `chat_completion` / `request` / `role` / `tool_schema` / `rate_limiter` modules, provider ID/name constants, `GOOGLE_AI_API_URL` (`src/language_model_core.rs`) | `GPL-3.0-or-later` |
| `language_model` (`crates/language_model`) | The abstraction: `pub trait LanguageModelProvider: LanguageModelClient` (L529), `pub trait LanguageModelClient` (L380), `pub trait LanguageModelProviderState` (L663), the `LanguageModel` plain-data struct (identity, capabilities, limits), registry, API-key plumbing, image helpers; re-exports `language_model_core::*` (`src/language_model.rs`, `src/registry.rs`) | `GPL-3.0-or-later` |
| `language_models` (`crates/language_models`) | The aggregator: settings + registration for every built-in provider; one adapter module per provider under `src/provider/` (`anthropic.rs` + `anthropic/` submodules, `anthropic_compatible.rs`, `api_compatible.rs`, `bedrock.rs`, `cloud.rs`, `copilot_chat.rs`, `deepseek.rs`, `google.rs`, `llama_cpp.rs`, `lmstudio.rs`, `mistral.rs`, `ollama.rs`, `open_ai.rs`, `open_ai_compatible.rs`, `open_router.rs`, `openai_subscribed.rs`, `opencode.rs`, `vercel_ai_gateway.rs`, `x_ai.rs`, `x_ai_subscribed.rs`) | `GPL-3.0-or-later` |
| `language_models_cloud` (`crates/language_models_cloud`) | Zed-hosted ("zed.dev") completion plumbing used by the cloud provider adapter (`language_models/src/provider/cloud.rs`): builds on `cloud_llm_client`/`cloud_api_client` and the Anthropic/Google event mappers | `GPL-3.0-or-later` |

**Protocol crates** (wire format + streaming per vendor; "—" = not verified in the fetched source):

| Crate (path) | Protocol evidence from fetched source | Features verified from source | License |
| --- | --- | --- | --- |
| `anthropic` (`crates/anthropic`) | `pub struct Request { model, max_tokens, messages, tools, thinking, tool_choice, system, cache_control, context_management, metadata, output_config, … }` (`src/anthropic.rs`); streaming/event mapping in `src/completion.rs` (`AnthropicEventMapper`, imported by `language_models_cloud`); also `src/batches.rs` | Anthropic Messages; tools, extended thinking, prompt `cache_control`; streaming event mapper | `GPL-3.0-or-later` |
| `open_ai` (`crates/open_ai`) | `OPEN_AI_API_URL = "https://api.openai.com/v1"` and `{api_url}/chat/completions` (`src/open_ai.rs`); Responses API types in `src/responses.rs` (`Request`, `CountTokensRequest`, `CompactRequest`, `CompactedResponse`) | Chat Completions + Responses; `stream`, `stream_options`, `tools`, `tool_choice`, `parallel_tool_calls`, `prompt_cache_key` | `GPL-3.0-or-later` |
| `google_ai` (`crates/google_ai`) | Re-exports `GOOGLE_AI_API_URL` from `language_model_core` (`https://generativelanguage.googleapis.com`); serializes `"generateContent"` (Gemini native) | — | `GPL-3.0-or-later` |
| `deepseek` (`crates/deepseek`) | `DEEPSEEK_API_URL = "https://api.deepseek.com/v1"` | — | `GPL-3.0-or-later` |
| `ollama` (`crates/ollama`) | `OLLAMA_API_URL = "http://localhost:11434"`, `{api_url}/api/chat`, `{api_url}/api/tags`; `struct ChatRequest { model, messages, stream, keep_alive, options, tools, think }`, `ChatOptions { num_ctx, num_predict, … }` | Ollama native; streaming, tools, thinking (`think`), context length (`num_ctx`) | `GPL-3.0-or-later` |
| `lmstudio` (`crates/lmstudio`) | `LMSTUDIO_API_URL = "http://localhost:1234/api/v0"`, `{api_url}/chat/completions` | — | `GPL-3.0-or-later` |
| `llama_cpp` (`crates/llama_cpp`) | `LLAMA_CPP_API_URL = "http://localhost:8080"`, `/v1/chat/completions`, `/props` | — | `GPL-3.0-or-later` |
| `mistral` (`crates/mistral`) | `MISTRAL_API_URL = "https://api.mistral.ai/v1"` | — | `GPL-3.0-or-later` |
| `x_ai` (`crates/x_ai`) | `XAI_API_URL = "https://api.x.ai/v1"` | — | `GPL-3.0-or-later` |
| `open_router` (`crates/open_router`) | `OPEN_ROUTER_API_URL = "https://openrouter.ai/api/v1"` | — | `GPL-3.0-or-later` |
| `codestral` (`crates/codestral`) | `CODESTRAL_API_URL = "https://codestral.mistral.ai"`; not present in `language_models/src/provider/` | — | `GPL-3.0-or-later` |
| `bedrock` (`crates/bedrock`) | AWS Bedrock (docs: Converse API; "mantle" OpenAI-compatible models auto-routed per model with `protocol` ∈ `chat_completions`/`responses`) | — | `GPL-3.0-or-later` |
| `copilot_chat` (`crates/copilot_chat`) | `DEFAULT_COPILOT_API_ENDPOINT = "https://api.githubcopilot.com"`; `copilot_oauth::DeviceFlow` module | — | `GPL-3.0-or-later` |
| `opencode` (`crates/opencode`) | OpenCode API (docs: model list + capabilities fetched remotely; per-model `protocol` ∈ `anthropic` / `openai_responses` / `openai_chat` / `google`) | — | `GPL-3.0-or-later` |
| `openai_subscribed` (`crates/openai_subscribed`) | `CODEX_BASE_URL = "https://chatgpt.com/backend-api/codex"` (ChatGPT subscription) | — | `GPL-3.0-or-later` |
| `x_ai_subscribed` (`crates/x_ai_subscribed`) | Registered by `language_models`; purpose beyond the crate name not source-verified | — | `GPL-3.0-or-later` |

Layout corrections versus expectations: there is **no `vercel` crate** at HEAD — Vercel AI Gateway is a module, `crates/language_models/src/provider/vercel_ai_gateway.rs` (`API_URL = "https://ai-gateway.vercel.sh/v1"`, `VERCEL_AI_GATEWAY_API_KEY`); there is **no `groq` crate** (Groq is not a first-class provider in the docs; it would go through OpenAI-compatible endpoints); there is no `cloud` provider crate either — the Zed-hosted provider is `language_models_cloud` plus the `provider/cloud.rs` adapter. Custom endpoints are providers too: `provider/anthropic_compatible.rs` and `provider/open_ai_compatible.rs`.

**Infrastructure (selected)**

| Crate | Purpose / evidence | License |
| --- | --- | --- |
| `http_client` | Cargo description: "A HTTP client library for Zed and GPUI" (types `AsyncBody`, `HttpClient`, `Method`, `Response`) | **`Apache-2.0`** |
| `http_client_tls`, `reqwest_client` | TLS configuration; reqwest-backed `HttpClient` implementation | **`Apache-2.0`** |
| `credentials_provider`, `zed_credentials_provider` | Credential plumbing (keychain, env vars) | `GPL-3.0-or-later` |
| `agent` | The agent crate (deps: `client`, `http_client`, `language_model`); internals not inspected | `GPL-3.0-or-later` |
| `agent_settings`, `agent_ui` | Agent settings/UI crates; internals not inspected | `GPL-3.0-or-later` |
| `settings`, `settings_content` | Settings machinery; `settings_content/src/language_model.rs` defines `AllLanguageModelSettingsContent { anthropic_compatible, openai_compatible, … }` | `GPL-3.0-or-later` |
| `collections`, `gpui_shared_string`, `gpui` | Utility collections; string type; UI framework | **`Apache-2.0`** |

### Licensing

The repo is **non-uniformly licensed** and has changed over time; the root contains exactly two license files:

- `LICENSE-GPL` — standard GPLv3 text, first lines "GNU GENERAL PUBLIC LICENSE" / "Version 3, 29 June 2007".
- `LICENSE-APACHE` — first line "Copyright 2022 - 2025 Zed Industries, Inc.", then the Apache License 2.0 text ("Licensed under the Apache License, Version 2.0 …").
- There is no `LICENSE-AGPL` at the root at this commit (there is a `legal/` directory with terms/third-party material, not crate licenses).

Workspace root `Cargo.toml` sets `[workspace.package] publish = false` / `edition = "2024"` and **no** license field; each crate declares its own. Exact `license` strings fetched from each crate's `Cargo.toml`:

- `license = "GPL-3.0-or-later"` — `language_model`, `language_model_core`, `language_models`, `language_models_cloud`, `anthropic`, `open_ai`, `google_ai`, `deepseek`, `ollama`, `lmstudio`, `llama_cpp`, `mistral`, `x_ai`, `open_router`, `codestral`, `bedrock`, `copilot_chat`, `opencode`, `openai_subscribed`, `x_ai_subscribed`, `credentials_provider`, `zed_credentials_provider`, `agent`, `agent_settings`, `agent_ui`, `settings`, `settings_content`.
- `license = "Apache-2.0"` — `http_client`, `http_client_tls`, `reqwest_client`, `collections`, `gpui_shared_string`, `gpui`, `cloud_llm_client`.

So the split is structural: **transport/utilities are Apache-2.0; every provider, the abstraction, the agent, and the settings are GPL-3.0-or-later.** There is no permissively licensed provider/protocol crate.

What this implies for Moe (engineering reading, not legal advice):

- **Permissive crates** may be vendored into Moe with attribution and the Apache-2.0 license/NOTICE obligations, regardless of Moe's licensing. A missing LICENSE in Moe does not block *consuming* Apache-2.0 code; it only means others may not reuse Moe's code (all rights reserved by default).
- **GPL-3.0-or-later crates** may be copied/vendored/linked only if the resulting distribution complies with the GPL: the combined/derivative work must be distributed under GPL-3.0(-or-later) terms with source and notices. That requires Moe to adopt GPL-3.0-or-later (compatible with the Apache-2.0 parts; e.g. an "MIT OR Apache-2.0" Moe is *not* compatible for the GPL parts). Today, with **no `LICENSE` file at all**, Moe cannot ship GPL-derived code.

### Published packages

Checked against the crates.io API on 2026-10-09 (HTTP status + metadata per crate):

- **Not published (404)** — every Zed-specific AI crate name: `language_model`, `language_models`, `language_model_core`, `language_models_cloud`, `credentials_provider`, `zed_credentials_provider`, `agent_settings`, `agent_ui`, `google_ai`, `deepseek`, `lmstudio`, `open_router`, `copilot_chat`, `codestral`, `cloud_llm_client`, `settings_content`, `settings_json`. This matches `publish = false` in the workspace; these crates are workspace-internal only — reuse requires a Cargo **git dependency** or vendoring.
- **Name exists but is a different crate (trap)** — `http_client` (repo `http-rs/http-client`, "Types and traits for http clients"), `anthropic` (`abdelhamidbakhta/antrhopic-rs`, "Anthropic Rust SDK"), `open_ai` (`decomoraes/crab_ai`), `ollama` ("Ollama"), `mistral` ("RISC-V Operating System"), `x-ai` (`wiseaidotdev/x-ai`), `bedrock` ("Glue library between Vulkan and Rust"), `llama_cpp` (`edgenai/llama_cpp-rs`), `agent` ("A flexible AI Agent SDK…"), `settings` ("Extensions for the directories crate"), `reqwest_client` (`i18n-site/rust.git`). A naive `cargo add anthropic` gets someone else's SDK.
- **Zed's own published crate** — `gpui` 0.2.2 (published 2025-10-22; crates.io metadata points at `github.com/zed-industries/zed`, homepage `gpui.rs`; no `license` field returned by the API for the published version, while the repo's current `gpui/Cargo.toml` says `Apache-2.0`). It is the UI framework, not the AI layer.
- **Ambiguous** — `zed_llm_client` 0.8.7 (published 2025-07-10, "A client for interacting with the Zed LLM API"); no repository and no license field in the API response, so Zed ownership could not be confirmed. Do not rely on it.

### Extraction feasibility

Dependency sets from each crate's `Cargo.toml` at the pinned commit (internal Zed crates are workspace deps):

- `anthropic` — internal: `collections` (Apache-2.0), `http_client` (Apache-2.0), `language_model_core` (GPL); external: `anyhow`, `chrono`, `futures`, `log`, `schemars` (optional), `serde`, `serde_json`, `strum`, `thiserror`. **No `gpui`.** Plain HTTP client + serde types.
- `open_ai` — internal: `collections`, `http_client`, `language_model_core`; external: `anyhow`, `futures`, `log`, `rand`, `schemars` (optional), `serde`, `serde_json`, `strum`, `thiserror`. **No `gpui`.**
- `ollama` — internal: `http_client` (Apache-2.0), `settings_content` (GPL); no `language_model_core` (its `LanguageModel` adapter glue lives in `language_models/src/provider/ollama.rs`, which defines `OllamaLanguageModelProvider`). **No `gpui`.**
- `language_model_core` — internal: `cloud_llm_client` (Apache-2.0), `collections`, `gpui_shared_string` (Apache-2.0); external: `anyhow`, `futures`, `http`, `log`, `partial-json-fixer`, `schemars`, `serde`, `serde_json`, `strum`, `thiserror`, `async-lock`. GPL. No `gpui`, no Zed `client`.
- `language_model` (singular) — requires `gpui`, `gpui_util`, `icons`, `credentials_provider`, `env_var`, `image`, … → UI-entangled.
- `language_models` (aggregator) — requires `gpui`, `gpui_tokio`, `client`, `extension`, `extension_host`, `copilot_ui`, `component`, `menu`, `ai_onboarding`, `cloud_api_client`, `cloud_api_types`, `language_models_cloud`, `settings`, `paths`, `fs`, `release_channel`, `aws-*`, all protocol crates, … → drags a large part of the workspace and the GPUI application stack.

Conclusion: a **single protocol crate is a small, gpui-free, vendorable unit** (e.g. `anthropic` → 3 internal crates, all license-clean within an Apache/GPL mix), but the crate you'd actually want alongside it — the built-in model catalogs, settings, and provider registry — lives in the entangled aggregator (`language_models`). Vendoring a protocol crate alone gives wire types and a streaming event mapper, not a full provider.

### Model metadata

- The per-model value type is **`LanguageModel`** in `crates/language_model/src/language_model.rs` (GPL-3.0-or-later), documented in-source as "plain data: identity, capabilities, and limits". Verified fields: `id`, `name`, `provider_id`, `provider_name`, `upstream_provider_id`, `upstream_provider_name`, `telemetry_id`, `is_latest`, `disabled_reason`, `requires_data_retention`, `refusal_fallback_model_id`, `cost_info`, `supports_thinking`, `supports_disabling_thinking`, `supports_fast_mode`, `supported_effort_levels`, `supports_server_side_compaction`, `supports_explicit_compaction`, `supports_explicit_compaction_output_limit`, `minimum_explicit_compaction_input_tokens`, `supports_images`, `supports_tools`, `tool_choice_support` (`auto`/`any`/`none`), `supports_streaming_tools`, `supports_split_token_display`, `max_token_count`, `max_input_tokens`, `max_output_tokens`.
- **Built-in catalogs are per-provider, in the aggregator**, not in the protocol crates. Example: `crates/language_models/src/provider/anthropic.rs` (36,042 bytes) contains 8 `"claude…"` model-ID strings (the count includes test fixtures) and populates `max_input_tokens` from settings, `supports_images: true`. The OpenAI adapter (`provider/open_ai.rs`, 25,109 bytes) contains **no** `"gpt-…"` string literals — its model surface is settings/discovery-driven at HEAD (see [Verification notes](#verification-notes)).
- Some providers **fetch** models at runtime: OpenCode ("Zed fetches the available Zen and Go models and their capabilities from OpenCode" — docs), llama.cpp (`/props` for context length; `/models/sse` discovery in router mode — docs), Ollama/LM Studio autodiscovery (`auto_discover` setting — docs).
- License: all of the above is GPL-3.0-or-later source code, not a data feed. For a model catalog Moe can embed, **models.dev (MIT) remains the better data source** ([`ai-model-catalogs.md`](./ai-model-catalogs.md)); Zed's value here is the *capability schema* (field list above) and the settings-override pattern (below).

### Custom endpoints

- From `docs/src/ai/use-api-access.md`, `use-a-gateway.md`, `use-a-local-model.md` (repo copies; the zed.dev rendering of `use-api-access` was fetched live and matches):
- **Per-provider `api_url` overrides** — documented for Mistral (`https://api.mistral.ai/v1`), DeepSeek (`https://api.deepseek.com/v1`), xAI (`https://api.x.ai/v1`), OpenRouter (`https://openrouter.ai/api/v1`), Vercel AI Gateway (`https://ai-gateway.vercel.sh/v1`), with provider-specific `available_models` entries carrying `name`, `display_name`, `max_tokens`, `max_output_tokens`, `max_completion_tokens`, `supports_tools`, `supports_images`, and `mode` (`{ "type": "thinking", "budget_tokens": … }`).
- **Local providers** — `language_models.ollama` / `llama.cpp` / `lmstudio` with `api_url` (defaults `http://localhost:11434`, `http://localhost:8080`, `http://localhost:1234/api/v0`) plus `auto_discover` and optional `context_window`.
- **Anthropic-dialect custom endpoints: yes** — `language_models.anthropic_compatible` is a map keyed by provider name; the settings struct (`crates/settings_content/src/language_model.rs`) requires `api_url: String` per entry; models carry `capabilities { tools, images, prompt_caching }` (defaults `true`/`false`/`false`) plus `default_temperature`, `extra_beta_headers`, `mode`, `tool_override`. The key is stored in the keychain or read from a generated `<PROVIDER_NAME>_API_KEY` env var; `custom_headers` adds extra headers (Zed-managed headers such as `X-Api-Key`/`Anthropic-Version` cannot be overridden). Docs define this path as "a service implements Anthropic's Messages API (`/v1/messages`)".
- **OpenAI-dialect custom endpoints including Responses** — `language_models.openai_compatible` is a map keyed by provider name with `api_url` and per-model capability flags: `tools`, `images`, `parallel_tool_calls`, `prompt_cache_key`, `chat_completions`, `interleaved_reasoning`, `max_tokens_parameter`, plus `reasoning_effort` (`none`…`max`). Setting `capabilities.chat_completions: false` makes Zed use the **Responses** endpoint for that model — one config surface spanning both OpenAI protocols.
- **Other protocol knob** — Bedrock "mantle" models take `protocol: chat_completions | responses`; OpenCode custom models take `protocol: anthropic | openai_responses | openai_chat | google` and `custom_model_api_url`.
- **Settings namespace quirks** — the settings key is `language_models` (plural); Google AI is configured under `google` (crate `google_ai`), OpenAI under `openai`, OpenRouter under `open_router`, Vercel under `vercel_ai_gateway`.

## Leverage options for Moe

Ranked by cost/benefit, license-aware. Moe currently has **no LICENSE file** (verified 2026-10-09): its own code is all-rights-reserved, which does not prevent consuming Apache-2.0 code but does prevent shipping GPL-derived code.

1. **Reference-only (recommended; no license change needed).** Study and reimplement the patterns with Moe's own code:
   - Anthropic Messages request shape and streamed event mapping — `crates/anthropic/src/anthropic.rs` (`Request` fields: top-level `system`, required `max_tokens`, `thinking`, `cache_control`) and `src/completion.rs` (`AnthropicEventMapper`).
   - OpenAI Responses request/response types — `crates/open_ai/src/responses.rs`.
   - The protocol-agnostic event taxonomy worth mirroring in Moe's client trait: `language_model_core`'s `LanguageModelCompletionEvent` (`Text`, `Thinking`, `RedactedThinking`, `ToolUse`, `UsageUpdate`, `Compaction`, …).
   - The custom-endpoint capability checklist Moe should adopt: `chat_completions` toggle (Chat Completions vs Responses), `interleaved_reasoning` (`reasoning_content` echo), `max_tokens_parameter` (which field carries the output limit), `parallel_tool_calls`, `prompt_cache_key`, plus thinking mode/budget and `custom_headers`.
   Reading GPL code for ideas is unconstrained; **do not copy code or comments verbatim** into Moe.
2. **Vendor the permissive subset (Apache-2.0).** `http_client` + `http_client_tls` + `reqwest_client` (+ `collections`, `gpui_shared_string`) are Apache-2.0 and could be vendored with attribution. Practical value for Moe is low — it already runs `reqwest`; and no provider/protocol code is in this subset (`cloud_llm_client` is Apache-2.0 but Zed-cloud-specific). Vendoring any of these also means Moe should add its own LICENSE and third-party notices when it starts shipping vendored code.
3. **Git-dependency on Zed crates.** Technically possible (crates are `publish = false`, so `git = "https://github.com/zed-industries/zed"` + `rev`); but the small crates (`anthropic`, `open_ai`, `ollama`, `language_model_core`) are GPL-3.0-or-later (see §4), and the crate that would give Moe the built-in catalogs/registry (`language_models`) pulls `gpui`, `client`, `extension_host`, and the app stack — effectively adopting GPUI application internals inside a Tauri app. Not practical.
4. **Copy GPL code into Moe.** Permitted only if Moe adopts **GPL-3.0-or-later** (then copying is possible with attribution and source distribution; Apache-2.0 parts remain compatible). Incompatible with an MIT/Apache-licensed Moe, and with the current no-license state. If Moe ever wants this, the smallest legally-usable unit is one protocol crate + `language_model_core`; model metadata would still come from models.dev.

**Effect on the companion recommendation: none.** Zed's layer is not a reusable dependency for Moe today — it is GPL-3.0-or-later where it matters, unpublished, and entangled at the registry/settings layer. [`ai-provider-libraries-rust.md`](./ai-provider-libraries-rust.md)'s conclusion stands: `genai` (MIT OR Apache-2.0) remains the leading standalone multi-provider crate; Zed is the best *reference implementation* to study for Anthropic/Responses streaming and for the capability knobs a custom-endpoint UX needs.

## Sources

Pinned revision: `129c68ff4bc136b3fac2557388321374b4e5c858` (default branch `main`; commit date 2026-10-09T14:06:54Z). All raw file fetches below are under `https://raw.githubusercontent.com/zed-industries/zed/129c68ff4bc136b3fac2557388321374b4e5c858/…` unless a GitHub/crates.io URL is given.

- Repository metadata and HEAD — https://api.github.com/repos/zed-industries/zed, https://api.github.com/repos/zed-industries/zed/commits?per_page=1, commit page https://github.com/zed-industries/zed/commit/129c68ff4bc136b3fac2557388321374b4e5c858
- Directory listings (GitHub contents API): `crates/`, `crates/anthropic/src`, `crates/open_ai/src`, `crates/settings_content/src`, `crates/language_models/src/provider`, `docs/src/ai` — https://api.github.com/repos/zed-industries/zed/contents/crates (and subpaths)
- Root licenses — `LICENSE-GPL`, `LICENSE-APACHE`; root manifest `Cargo.toml`; `legal/` listing
- Crate manifests (license + deps), `crates/<name>/Cargo.toml`: `language_model`, `language_model_core`, `language_models`, `language_models_cloud`, `anthropic`, `open_ai`, `google_ai`, `deepseek`, `ollama`, `lmstudio`, `llama_cpp`, `mistral`, `x_ai`, `open_router`, `codestral`, `bedrock`, `copilot_chat`, `opencode`, `openai_subscribed`, `x_ai_subscribed`, `http_client`, `http_client_tls`, `reqwest_client`, `collections`, `gpui_shared_string`, `gpui`, `credentials_provider`, `zed_credentials_provider`, `agent`, `agent_settings`, `agent_ui`, `settings`, `settings_content`, `cloud_llm_client`
- Abstraction sources — `crates/language_model_core/src/language_model_core.rs`, `crates/language_model_core/src/provider.rs`, `crates/language_model/src/language_model.rs` (traits at L380/L529/L663), `crates/language_model/src/registry.rs`
- Aggregator sources — `crates/language_models/src/language_models.rs`, `crates/language_models/src/provider/{anthropic,open_ai,ollama,vercel_ai_gateway}.rs`, `crates/language_models_cloud/src/language_models_cloud.rs`
- Protocol sources — `crates/anthropic/src/anthropic.rs`, `crates/open_ai/src/{open_ai.rs,responses.rs}`, `crates/google_ai/src/google_ai.rs`, `crates/deepseek/src/deepseek.rs`, `crates/ollama/src/ollama.rs`, `crates/lmstudio/src/lmstudio.rs`, `crates/llama_cpp/src/llama_cpp.rs`, `crates/mistral/src/mistral.rs`, `crates/x_ai/src/x_ai.rs`, `crates/open_router/src/open_router.rs`, `crates/codestral/src/codestral.rs`, `crates/copilot_chat/src/copilot_chat.rs`, `crates/openai_subscribed/src/openai_subscribed.rs`
- Settings schema and defaults — `crates/settings_content/src/language_model.rs`, `assets/settings/default.json`
- Official docs (repo copies) — `docs/src/ai/llm-providers.md`, `docs/src/ai/use-api-access.md`, `docs/src/ai/use-a-gateway.md`, `docs/src/ai/use-a-local-model.md`, `docs/src/ai/by-company.md`; live zed.dev page fetched: https://zed.dev/docs/ai/use-api-access (other zed.dev doc URLs assumed to mirror the repo files)
- crates.io API — https://crates.io/api/v1/crates/{http_client,agent,anthropic,open_ai,ollama,mistral,x_ai,bedrock,llama_cpp,settings,reqwest_client,gpui,zed_llm_client,language_model,language_models,language_model_core,language_models_cloud,credentials_provider,zed_credentials_provider,agent_settings,agent_ui,google_ai,deepseek,lmstudio,open_router,copilot_chat,codestral,cloud_llm_client,settings_content,settings_json}
- This repo — `crates/moe-platform/src/config.rs` (L100–103, L113–118); `crates/moe-extensions/src/ai_client.rs`; companions [`ai-provider-libraries-rust.md`](./ai-provider-libraries-rust.md), [`ai-model-catalogs.md`](./ai-model-catalogs.md); root directory listing (no `LICENSE` file)

## Verification notes

- **Fetch reliability.** `raw.githubusercontent.com` single-request pipes returned empty bodies intermittently from this sandbox (several commands initially failed with exit 1 and no output); affected values were re-fetched with retries and cross-checked (e.g. `anthropic`/`ollama`/`open_ai` manifests, license files). All quoted license strings and endpoints come from fetches that returned HTTP 200 with a body; a reader re-verifying with the URLs above should reproduce them.
- **Not verified:** the internals of `agent`, `agent_settings`, `agent_ui` (existence and licenses only); the purpose of `x_ai_subscribed` beyond its name and registration; whether `language_models/src/provider/open_ai.rs` is fully settings/discovery-driven (verified only that it contains no `"gpt-…"` string literals at this commit); Gemini-native streaming (`streamGenerateContent`) in `google_ai`; images support inside the `anthropic` crate's source (only the settings-level capability flags and the `language_model` `supports_images` field were verified); the OpenCode/SDK wire protocol beyond the documented `protocol` enum; `crates/language_models/src/provider/api_compatible.rs` contents (existence only); `copilot_chat` beyond its default endpoint constant; `bedrock` beyond the manifest and docs. The anthropic model-ID count (8) includes test fixtures and is a rough figure.
- **zed.dev.** Only `https://zed.dev/docs/ai/use-api-access` was fetched live (it matches the repo's `docs/src/ai/use-api-access.md`); other zed.dev doc URLs (`/docs/ai/llm-providers`, `/docs/ai/use-a-gateway`, `/docs/ai/use-a-local-model`) were not fetched, their repo copies were. `search`-based discovery inside GitHub was not attempted (code-search needs auth), so the crate inventory is built from per-directory listings at the pinned commit.
- **crates.io.** Ownership of `zed_llm_client` could not be attributed to Zed (no repository/license fields); the published `gpui` version's crates.io metadata has no license field, while the repo manifest at HEAD says `Apache-2.0`. Statuses change over time; the pin here is 2026-10-09.
- **Moe.** No `LICENSE` file at the repo root as of 2026-10-09 (root listing). The AI-layer licensing implications above are an engineering reading of the quoted license fields, not legal advice.
