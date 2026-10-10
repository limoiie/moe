# Model metadata catalogs for a provider/model picker

Research note, 2026-10-09. Every claim below cites a primary source (official docs or source data) fetched on that date; URLs are in [Sources](#sources). Claims not verified against a live primary source are marked in [Verification notes](#verification-notes).

## Context

Moe is a keyboard-first command palette (Tauri 2, Rust backend, vanilla TS UI). Its AI feature today is one OpenAI-compatible endpoint: `AiConfig { base_url: Option<String>, model: Option<String> }` (`crates/moe-platform/src/config.rs`, L100–103; default model `gpt-4o-mini`, L113–118), per ADR-0005 — the decision this work replaces. The companion note [`ai-provider-libraries-rust.md`](./ai-provider-libraries-rust.md) covers Rust client crates; this note covers the data catalogs a provider/model picker needs.

The picker must expose, per provider: base URL, API-key env-var names, docs link, protocol; per model: context window, max output, tool support, image/vision input, input and output modalities, reasoning, pricing, release date (and ideally streaming). Moe will speak three wire protocols: OpenAI Chat Completions, OpenAI Responses, Anthropic Messages. The client is Rust + `reqwest` consuming JSON, either as an embedded snapshot or a runtime fetch.

## Findings per catalog

### models.dev

An open-source database of AI model specifications, pricing, and capabilities, maintained by the SST/OpenCode team ("Models.dev is created by the maintainers of SST", repo README). GitHub `sst/models.dev`, default branch `dev` (raw `main` paths 404 as of the fetch), listed at 7.2k stars / 11,682 commits on the repo page. License: MIT — `LICENSE` fetched 2026-10-09 reads "MIT License / Copyright (c) 2025 models.dev".

Data layout (README): TOML files by provider and canonical model — provider-agnostic facts in `models/<lab>/<model>.toml`, serving details in `providers/<provider>/provider.toml` plus `providers/<provider>/models/<model>.toml`. Provider models may inherit facts via `base_model`; generated entries then carry `canonical_model_id` linking a provider model to its lab model. Contribution is by pull request; a GitHub Action validates submissions against a schema (`packages/core/src/schema.ts`). Example provider TOML (`providers/deepseek/provider.toml`, 716 B) contains only `name`, `env`, `npm`, `doc`, `api` plus source comments citing DeepSeek docs.

**Endpoints and measured sizes** (all fetched 2026-10-09; sizes are bytes on disk):

| Endpoint | Content | Size / count |
| --- | --- | --- |
| `https://models.dev/api.json` | provider data: providers → models | 5,374,767 B; 225 providers, 8,466 models |
| `https://models.dev/models.json` | provider-agnostic canonical models | 411,416 B; 448 entries |
| `https://models.dev/catalog.json` | combined catalog | 5,786,207 B |
| `https://models.dev/models.json?type=all` | canonical models incl. all types | 412,850 B; 451 entries |
| `https://models.dev/model-schema.json` | schema (mentioned in README; not fetched) | — |

The site text and README describe `?type=` as: "Specialized model types are omitted by default… The currently supported model type is `decision`" (also accepted on `models.json`, `catalog.json`, `model-schema.json`). Measured, this means exactly three entries — `liquid/d1`, `nace-ai/drex-v1.5`, `typesafe/jev-latest`, each `"type": "decision"` — are present with `?type=all` and absent by default. The default files still contain embeddings/image/audio models (e.g. `google/gemini-embedding-2` and `openai/gpt-image-1` are present in the default `api.json`), so "specialized" maps to `type: decision` only as of this fetch.

**Provider entry fields** (verified on `deepseek`):

```json
{
  "id": "deepseek",
  "env": ["DEEPSEEK_API_KEY"],
  "npm": "@ai-sdk/openai-compatible",
  "api": "https://api.deepseek.com",
  "name": "DeepSeek",
  "doc": "https://api-docs.deepseek.com/quick_start/pricing"
}
```

- `id`, `name` — identifier and display name.
- `env[]` — API-key environment variable names (DeepSeek has one; Google lists three aliases: `GOOGLE_API_KEY`, `GOOGLE_GENERATIVE_AI_API_KEY`, `GEMINI_API_KEY`).
- `npm` — the AI SDK package for the provider (all 16 official providers checked carry one; e.g. `@ai-sdk/openai`, `@ai-sdk/anthropic`, `@ai-sdk/google`, `@ai-sdk/xai`, `@ai-sdk/mistral`, `@ai-sdk/groq`, `@openrouter/ai-sdk-provider`).
- `api` — base URL. Per the README schema, optional: "OpenAI-compatible API endpoint. Required only when using `@ai-sdk/openai-compatible`". SDK-native providers (openai, anthropic, google, xai, mistral, groq) have no `api` in this snapshot.
- `doc` — provider documentation link (e.g. DeepSeek → `api-docs.deepseek.com/quick_start/pricing`).
- `models{}` — map of model id → model entry.

**Model entry fields** (verified on `deepseek.models.deepseek-flash`, the current DeepSeek model; ids seen also include `canonical_model_id: "deepseek/deepseek-v4.1-flash"`):

| Field | Meaning (README schema) |
| --- | --- |
| `id`, `name`, `description`, `family` | identity / display / grouping (`deepseek-flash` family) |
| `attachment` | supports file attachments |
| `reasoning`, `reasoning_options[]` | supports reasoning; option objects `{type:"toggle"}` and/or `{type:"effort", values:[...]}` (DeepSeek offers toggle + `low`/`high`/`max`) |
| `interleaved{field}` | interleaved reasoning field, e.g. `reasoning_content` |
| `tool_call`, `structured_output`, `temperature` | booleans |
| `knowledge`, `release_date`, `last_updated` | dates (`YYYY-MM` or `YYYY-MM-DD`) |
| `modalities.input[]` / `modalities.output[]` | e.g. `["text","image"]` / `["text"]`; input values seen: text, image, audio, video, pdf |
| `open_weights`, `license`, `weights[]` (canonical), `benchmarks[]` (canonical) | open-weight facts; canonical entries carry Hugging Face links and benchmark records (name/score/metric/variant/source) |
| `limit.context` / `limit.output` | max context / max output tokens |
| `status` | `alpha` / `beta` / `deprecated` |
| `cost.input`, `cost.output`, `cost.reasoning`, `cost.cache_read`, `cost.cache_write`, `cost.input_audio`, `cost.output_audio` | USD **per million tokens** (README: "Cost per million input tokens (USD)") |

Canonical (`models.json`) entries have no `cost` — pricing lives on provider entries. There is **no streaming flag** anywhere in the schema (observed on all fetched entries).

Other facts: model IDs are "the identifier used by AI SDK" (README). Provider logos are served at `/logos/{provider}.svg`, lab logos at `/logos/labs/{lab}.svg`. `api.json` responses carry `etag` + `cache-control: public, max-age=0, must-revalidate` (HEAD, 2026-10-09), so conditional refresh is possible. An npm SDK exists: `@opencode-ai/models` — "Official typed client for the models.dev API… exports the latest snapshot for offline use" (site); npm registry 2026-10-09: latest `0.0.99`, published `2026-10-09T05:38:16Z`, unpacked size 6,578,925 B.

### LiteLLM

A single flat price/capability map published by the LiteLLM project (`BerriAI/litellm`): `model_prices_and_context_window.json` at the repo root (fetched 2026-10-09; 3,136,328 B on disk). Contents: 4,507 keys = 4,506 model entries + one `sample_spec`; 138 distinct `litellm_provider` values; 3,378 entries with `mode: "chat"`. License (`LICENSE` fetched 2026-10-09): "All content that resides under the `enterprise/` directory… is licensed under… `enterprise/LICENSE`. Content outside… is available under the MIT license"; "Copyright (c) 2023 Berri AI".

`sample_spec` documents the vocabulary. Key fields and measured coverage over the 4,506 entries:

| Field | Meaning | Entries with it |
| --- | --- | --- |
| `litellm_provider`, `mode` | provider id; `chat`, `embedding`, `image_generation`, `audio_transcription`, `rerank`, … | all / 3,378 chat |
| `max_input_tokens`, `max_output_tokens` | token limits (`max_tokens` is documented legacy) | 3,724 / 3,339 |
| `input_cost_per_token`, `output_cost_per_token` | USD **per token** (e.g. `3e-7` = $0.30/1M) | all entries |
| `cache_read_input_token_cost`, `cache_creation_input_token_cost`, `output_cost_per_reasoning_token`, `input_cost_per_audio_token` | finer billing | varies |
| `supports_function_calling`, `supports_parallel_function_calling`, `supports_vision`, `supports_reasoning`, `supports_response_schema`, `supports_system_messages`, `supports_prompt_caching`, `supports_pdf_input`, `supports_audio_input/output`, `supports_web_search`, `supports_native_streaming` | capability booleans | tools 2,673; vision 1,792; reasoning 1,786 |
| `supported_endpoints`, `supported_regions`, `deprecation_date` | serving metadata | varies |
| `source` | URL of the pricing source (e.g. `https://api-docs.deepseek.com/quick_start/pricing`) | varies |
| `off_peak_pricing` | object with alternative per-token rates and UTC windows (present on the `deepseek-v4-flash` entry) | some entries |

A real entry (`deepseek-v4-flash`) includes: `max_input_tokens: 1000000`, `max_output_tokens: 393216`, `input_cost_per_token: 3e-7`, `output_cost_per_token: 1.2e-6`, `supports_function_calling/vision/reasoning/prompt_caching/response_schema/tool_choice: true`, `supports_native_streaming: true`, `supported_endpoints: ["/v1/chat/completions"]`, and an `off_peak_pricing` block whose windows match DeepSeek's peak/off-peak scheme. This is the only catalog checked that carries a **streaming flag** (`supports_native_streaming`).

Update mechanism: not verified. `CONTRIBUTING.md` (raw, fetched) is UI-focused; the `docs.litellm.ai/docs/contributing` page fetched is "Contributing - UI"; the repository README fetched contains no section about the price map. Entries carrying `source` URLs suggest manual curation; the actual process is flagged in [Verification notes](#verification-notes).

### OpenRouter

`GET https://openrouter.ai/api/v1/models` — public, no API key (fetched 2026-10-09: HTTP 200, 782,040 B, `data[]` with 469 models). Response field vocabulary (observed on the live payload):

| Field | Meaning |
| --- | --- |
| `id`, `canonical_slug`, `name`, `description` | identity |
| `created` | unix timestamp |
| `context_length` | context window |
| `architecture` | `{modality: "text+image+file->text", input_modalities[], output_modalities[], tokenizer}` |
| `pricing` | USD **per token** as strings: `prompt`, `completion`, `input_cache_read`, `input_cache_write`, `input_cache_write_1h`, plus `overrides[]` for long context (`min_prompt_tokens`) |
| `supported_parameters[]` | e.g. `tools` (401 of 469 models), `reasoning`, `response_format`, … |
| `reasoning` | `{mandatory, supported_efforts[], default_effort}` |
| `top_provider` | `{context_length, max_completion_tokens, is_moderated}` |
| `knowledge_cutoff`, `expiration_date`, `per_request_limits`, `hugging_face_id`, `links`, `default_parameters`, `supported_voices` | lifecycle / extras |

Example (`anthropic/claude-haiku-5.5`): `context_length: 1000000`, `top_provider.max_completion_tokens: 128000`, `pricing.prompt: "0.0000001"` ($0.10/1M), `completion: "0.0000005"` ($0.50/1M), with long-context `overrides` — consistent with Anthropic's own docs below.

Terms: the models endpoint requires no authentication. OpenRouter's docs index (`llms.txt`, fetched 2026-10-09) states that data returned by its **usage datasets** endpoints is CC BY 4.0 "reuse and republish… including commercially, with attribution to OpenRouter"; no equivalent license statement was found for the `/models` JSON itself (see [Verification notes](#verification-notes)).

### Provider-native list endpoints

Which endpoints return capability metadata versus only IDs (all docs fetched 2026-10-09):

| Provider | Endpoint | Returns | Capability metadata? |
| --- | --- | --- | --- |
| OpenAI | `GET /v1/models` | per model: `id`, `object`, `created`, `owned_by`, `shutdown_date` (openai-openapi spec example) | **No** — IDs/ownership only, no limits, no pricing |
| Anthropic | `GET /v1/models` | paginated `data[]`: `id`, `type`, `display_name`, `created_at`, `deprecated_at`, `lifecycle`, `line` (`haiku`/`sonnet`/`opus`/`fable`/`mythos`), `max_input_tokens`, `max_tokens`, `retires_at`, `capabilities{batch, citations, code_execution, context_management, effort{low…max}, image_input, pdf_input, server_tools{web_search, code_execution}, structured_outputs, thinking{types{adaptive,disabled,enabled}}}`; filters `lifecycle`; pagination `after_id`/`before_id`/`limit`, `first_id`/`has_more`/`last_id` | **Yes** — context window, max output, capability flags. No pricing |
| Google | `GET https://generativelanguage.googleapis.com/v1beta/models` | paginated; `Model{name, baseModelId, version, displayName, description, inputTokenLimit, outputTokenLimit, supportedGenerationMethods[], thinking, temperature, maxTemperature, topP, topK}` | **Yes** — token limits, generation methods, `thinking` flag. No pricing |
| DeepSeek | `GET /models` (base `https://api.deepseek.com`) | `{object:"list", data:[{id, object:"model", owned_by, name, context_window, max_output_tokens, input_modalities[], output_modalities[], effort{supported_levels[], default_level}, api_capabilities{anthropic_messages{system_prompt_update}}}]}` — example: `deepseek-flash`, context_window 1048576, max_output_tokens 393216, efforts low/high/max | **Yes** — limits, modalities, effort levels, per-protocol capabilities. No pricing (separate pricing page) |
| Ollama | `GET /api/tags` | `{models:[{name, model, modified_at, size, digest, details{parent_model, format, family, families, parameter_size, quantization_level}}]}` | **No** (list only) |
| Ollama | `POST /api/show` | `{modelfile, parameters, template, details, model_info{…}, capabilities[]}` — example `capabilities: ["completion","vision"]`; `/api/create` accepts `capabilities` such as `["decision"]` | **Yes** — capability list + technical `model_info` |

In short: Anthropic, Google, DeepSeek, and Ollama `/api/show` return real capability metadata; OpenAI `/v1/models` and Ollama `/api/tags` are ID lists; none return pricing.

## Model property sample table

Selection method: from the 2026-10-09 `api.json` snapshot, for each lab take that lab's own provider entry and up to 3–4 most recent non-deprecated models with text output by `release_date`; `canonical_model_id` was not needed since these are first-party entries. All prices below are the lab's provider entry in models.dev, USD per 1M tokens. "Docs" marks whether the row was compared against the provider's own docs fetched 2026-10-09 (details in [Verification notes](#verification-notes)).

| Model (provider id) | Lab | Context | Max out | Tools | Reason | In modalities | Out | Attach | $/1M in | $/1M out | Released | Docs |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `gpt-6.1-sol` | OpenAI | 1,050,000 | 128,000 | ✓ | ✓ | text+image+pdf | text | ✓ | 2 | 10 | 2026-09-29 | ✓ |
| `gpt-6-sol` | OpenAI | 1,050,000 | 128,000 | ✓ | ✓ | text+image+pdf | text | ✓ | 2 | 10 | 2026-09-22 | — |
| `gpt-6-luna` | OpenAI | 1,050,000 | 128,000 | ✓ | ✓ | text+image+pdf | text | ✓ | 0.10 | 0.52 | 2026-09-22 | docs: $0.50 out |
| `gpt-6-astra` | OpenAI | 1,050,000 | 128,000 | ✓ | ✓ | text+image+pdf | text | ✓ | 10 | 50 | 2026-09-04 | ✓ |
| `claude-haiku-5-5` | Anthropic | 1,000,000 | 128,000 | ✓ | ✓ | text+image+pdf | text | ✓ | 0.10 | 0.50 | 2026-10-07 | ✓ |
| `claude-sonnet-5-5` | Anthropic | 1,000,000 | 128,000 | ✓ | ✓ | text+image+pdf | text | ✓ | 2 | 10 | 2026-09-28 | ✓ |
| `claude-opus-5-5` | Anthropic | 1,000,000 | 128,000 | ✓ | ✓ | text+image+pdf | text | ✓ | 4 | 20 | 2026-09-22 | ✓ |
| `claude-fable-5-1` | Anthropic | 1,000,000 | 128,000 | ✓ | ✓ | text+image+pdf | text | ✓ | 10 | 50 | 2026-09-01 | ✓ |
| `gemini-3.8-flash` | Google | 1,048,576 | 65,536 | ✓ | ✓ | text+image+video+audio+pdf | text | ✓ | 0.75 | 3.75 | 2026-09-02 | — |
| `gemini-3.7-flash` | Google | 1,048,576 | 65,536 | ✓ | ✓ | text+image+video+audio+pdf | text | ✓ | 0.75 | 3.75 | 2026-08-13 | — |
| `gemini-3.5-flash-lite` | Google | 1,048,576 | 65,536 | ✓ | ✓ | text+image+video+audio+pdf | text | ✓ | 0.30 | 2.50 | 2026-07-21 | — |
| `deepseek-flash` | DeepSeek | 1,000,000 | 393,216 | ✓ | ✓ | text+image | text | ✓ | 0.15 | 0.60 | 2026-09-10 | ✓ (off-peak) |
| `deepseek-v4-pro` | DeepSeek | 1,000,000 | 393,216 | ✓ | ✓ | text | text | — | 0.66 | 1.98 | 2026-08-12 | ✓ (off-peak) |
| `grok-4.7` | xAI | 500,000 | 500,000 | ✓ | ✓ | text+image+pdf | text | ✓ | 2 | 6 | 2026-09-21 | ✓ |
| `grok-4.6` | xAI | 500,000 | 500,000 | ✓ | ✓ | text+image+pdf | text | ✓ | 2 | 6 | 2026-08-12 | — |
| `grok-4.5` | xAI | 500,000 | 500,000 | ✓ | ✓ | text+image+pdf | text | ✓ | 2 | 6 | 2026-07-08 | — |
| `mistral-large-4` | Mistral | 1,048,576 | 262,144 | ✓ | ✓ | text+image | text | ✓ | 0.68 | 2.09 | 2026-10-06 | model listed |
| `mistral-medium-latest` | Mistral | 262,144 | 262,144 | ✓ | ✓ | text+image | text | ✓ | 1.50 | 7.50 | 2026-04-29 | — |
| `mistral-medium-2604` | Mistral | 262,144 | 262,144 | ✓ | ✓ | text+image | text | ✓ | 1.50 | 7.50 | 2026-04-29 | — |
| `qwen3.8-omni-flash` | Alibaba | 1,000,000 | 131,072 | ✓ | ✓ | text+image+audio+video | text | ✓ | 0.15 | 0.47 | 2026-09-17 | — |
| `qwen3.8-flash` | Alibaba | 1,000,000 | 131,072 | ✓ | ✓ | text+image+video | text | ✓ | 0.15 | 0.47 | 2026-08-26 | — |
| `qwen3.8-max` | Alibaba | 1,000,000 | 131,072 | ✓ | ✓ | text+image+video+pdf | text | ✓ | 2 | 6 | 2026-08-03 | — |
| `kimi-k3` | Moonshot AI | 1,048,576 | 1,048,576 | ✓ | ✓ | text+image+video | text | ✓ | 3 | 15 | 2026-07-16 | — |
| `kimi-k2.7-code` | Moonshot AI | 262,144 | 262,144 | ✓ | ✓ | text+image+video | text | ✓ | 0.95 | 4 | 2026-06-12 | — |
| `kimi-k2.6` | Moonshot AI | 262,144 | 262,144 | ✓ | ✓ | text+image+video | text | ✓ | 0.95 | 4 | 2026-04-21 | — |
| `glm-5.3-flashx` | Zhipu AI | 1,000,000 | 131,072 | ✓ | ✓ | text+image+video+pdf | text | ✓ | 0.37 | 1.25 | 2026-09-18 | — |
| `glm-5.3-flash` | Zhipu AI | 1,000,000 | 131,072 | ✓ | ✓ | text+image+video+pdf | text | ✓ | 0.15 | 0.50 | 2026-08-26 | — |
| `glm-5.3` | Zhipu AI | 1,000,000 | 131,072 | ✓ | ✓ | text | text | — | 1.40 | 4.40 | 2026-08-14 | — |
| `muse-spark-1.3` | Meta | 1,048,576 | 131,072 | ✓ | ✓ | text+image+video+pdf+audio | text | ✓ | 1.25 | 4.25 | 2026-09-02 | — |
| `muse-spark-1.3-contributor` | Meta | 1,048,576 | 131,072 | ✓ | ✓ | text+image+video+pdf+audio | text | ✓ | 0.10 | 0.20 | 2026-09-02 | — |
| `muse-spark-1.2` | Meta | 1,048,576 | 131,072 | ✓ | ✓ | text+image+video+pdf+audio | text | ✓ | 1.25 | 4.25 | 2026-08-05 | — |

Reading notes:

- Every row has `tool_call: true` and `reasoning: true`; output modality is text by construction (image/audio/video-output models were filtered out). "Attach" is models.dev `attachment`; DeepSeek V4 Pro and GLM‑5.3 report no attachment support.
- DeepSeek prices are **off-peak**; DeepSeek's pricing page lists peak rates of $0.30/1.20 (Flash) and $1.32/3.96 (V4 Pro), with off-peak half of peak. LiteLLM instead stores the peak rate plus an explicit `off_peak_pricing` block — compare like-for-like.
- The models.dev homepage shows canonical-model prices that can differ from a lab's own provider entry (e.g. Grok 4.7 $1.60/$4.80 vs xAI's $2/$6; Mistral Medium $0.40/$2.00 vs the `mistral` provider entry $1.50/$7.50). For "official provider" rows, use the provider entry; the hero table's numbers look like best-across-providers aggregates.
- Streaming is absent from this table because models.dev has no streaming field; LiteLLM's `supports_native_streaming` is the substitute signal.

## Fit for Moe

**Primary: models.dev.** One JSON gives everything the picker needs, from a MIT-licensed repo with a documented schema and active maintenance. Concretely:

- `api.json` provider entries supply the official-provider prefill: `api` (base URL where defined), `env` (keychain onboarding hint), `doc` (docs link), `npm` (protocol hint, below), plus per-model `limit`, `modalities`, booleans, `cost`, `release_date`, `status`, `canonical_model_id`.
- `models.json` canonical entries add provider-agnostic facts: `license`, `weights[]` (Hugging Face links), `benchmarks[]`. Join provider model → canonical via `canonical_model_id` or via the AI SDK model id.
- Refresh: `api.json` returns an ETag with `must-revalidate`, so a Rust `reqwest` loop can refresh cheaply with `If-None-Match`, caching the last snapshot in the app data dir. A one-off 5.4 MB fetch is acceptable on desktop; a reduced snapshot is better (below).

**Fallback: LiteLLM.** One flat map, no provider metadata, but it fills gaps models.dev does not have: `supports_native_streaming`, cache/audio/reasoning billing, `off_peak_pricing` windows, `supported_endpoints`, `source` URLs, and coverage for models/labs absent from models.dev (4,506 entries across 138 providers). MIT except `enterprise/` — shipping the JSON with the app requires carrying the MIT notice. Use as a merge layer keyed by model name, not as the primary UI source (no provider base URLs, no docs links).

**OpenRouter.** Not a general catalog: fetch `https://openrouter.ai/api/v1/models` only when the user picks the OpenRouter provider, to show exactly what OpenRouter serves (469 models) with its own markups/overrides and `supported_parameters`. Public and unauthenticated today; license for reuse beyond the app's own display not established (see notes).

**Ollama / local.** models.dev has no local-Ollama provider entry (only `ollama-cloud`, `api: https://ollama.com/v1`); LM Studio appears as `lmstudio` with `api: http://127.0.0.1:1234/v1`. Local model lists are machine-specific, so enumerate at runtime via `GET /api/tags` and fetch capabilities + `model_info` via `POST /api/show` (the richest capability source in this research: `completion`, `vision`, `tools`, `thinking`, …).

**Embedded snapshot vs runtime fetch** (sizes measured 2026-10-09):

| Artifact | Raw | gzip |
| --- | --- | --- |
| models.dev `api.json` (all 225 providers, 8,466 models) | 5,374,767 B | 548,758 B |
| Reduced-field subset, all providers (name/dates/flags/modalities/limit/cost/canonical id; 8,466 models) | 3,894,121 B | — |
| Reduced-field subset, 16 official providers (825 models) | 342,854 B | 27,990 B |
| models.dev `models.json` (canonical, 448 models) | 411,416 B | 46,182 B |

Recommendation: embed the reduced-field, official-providers subset (~343 KB raw / ~28 KB gzip) compiled into the binary as the offline baseline, and refresh the full `api.json` at runtime (ETag-conditional) into the app cache for custom/relay providers; LiteLLM is an optional second-level fallback. This keeps the binary small, works offline, and still converges to live data.

**License and attribution.** models.dev: MIT (Copyright (c) 2025 models.dev) — keep the notice. LiteLLM: MIT except `enterprise/` (Copyright (c) 2023 Berri AI). OpenRouter datasets: CC BY 4.0 with attribution to OpenRouter (only if ever used; the models JSON has no stated license).

**Mapping catalog entries to Moe's three protocols.** models.dev's `npm` field names the AI SDK integration, which is a strong protocol hint; treat the exact wire protocol as app-side policy verified per provider:

| models.dev `npm` | Providers seen | Suggested Moe protocol |
| --- | --- | --- |
| `@ai-sdk/openai-compatible` | deepseek, alibaba, moonshotai, zhipuai, zai, lmstudio, ollama-cloud, opencode | OpenAI Chat Completions, base URL from `api` |
| `@ai-sdk/anthropic` | anthropic | Anthropic Messages |
| `@ai-sdk/openai` | openai, meta | OpenAI Responses (OpenAI's docs: "All latest OpenAI models… available via the Responses API"; check per provider) |
| `@ai-sdk/google`, `@ai-sdk/mistral`, `@ai-sdk/xai`, `@ai-sdk/groq`, `@openrouter/ai-sdk-provider` | google, mistral, xai, groq, openrouter | Vendor-native SDKs; no direct npm match to Moe's three protocols — pin a hand-checked protocol per provider (some vendors also expose compatible endpoints; e.g. DeepSeek documents both OpenAI- and Anthropic-format base URLs, and its provider TOML comments describe both request shapes) |

The custom-provider flow mirrors what the models.dev README prescribes for OpenAI-compatible services: protocol + base URL chosen by the user; `npm = @ai-sdk/openai-compatible` there corresponds to Moe's "OpenAI Chat" protocol. For official providers, prefill base URL (`api` when present, otherwise an app-side table for SDK-native providers), key env-var name(s), docs link (`doc`) and logo (`/logos/{provider}.svg`).

## Sources

All fetched 2026-10-09.

- models.dev homepage (API/endpoints, SDK, contribution, logos) — https://models.dev/
- models.dev provider data; default and `?type=all`; HEAD for ETag — https://models.dev/api.json
- models.dev canonical models; default and `?type=all` — https://models.dev/models.json
- models.dev combined catalog — https://models.dev/catalog.json
- models.dev repo (default branch `dev`, stars/commits, MIT in About) — https://github.com/sst/models.dev
- models.dev README (schema, TOML layout, `base_model`, `?type=`, validation) — https://raw.githubusercontent.com/sst/models.dev/dev/README.md
- models.dev LICENSE (MIT, Copyright 2025 models.dev) — https://raw.githubusercontent.com/sst/models.dev/dev/LICENSE
- Provider TOML example — https://raw.githubusercontent.com/sst/models.dev/dev/providers/deepseek/provider.toml
- Canonical model TOML example — https://raw.githubusercontent.com/sst/models.dev/dev/models/deepseek/deepseek-v4.1-flash.toml
- npm SDK — https://registry.npmjs.org/@opencode-ai/models (site documents `@opencode-ai/models`)
- LiteLLM model map — https://raw.githubusercontent.com/BerriAI/litellm/main/model_prices_and_context_window.json
- LiteLLM LICENSE — https://raw.githubusercontent.com/BerriAI/litellm/main/LICENSE
- LiteLLM README and CONTRIBUTING.md — https://raw.githubusercontent.com/BerriAI/litellm/main/README.md, https://raw.githubusercontent.com/BerriAI/litellm/main/CONTRIBUTING.md
- LiteLLM contributing docs (UI) — https://docs.litellm.ai/docs/contributing
- OpenRouter models list — https://openrouter.ai/api/v1/models
- OpenRouter docs index (datasets CC BY 4.0 statement) — https://openrouter.ai/docs/llms.txt
- Anthropic List Models — https://docs.anthropic.com/en/api/models-list
- Anthropic Models overview (capabilities, pricing, context/output per model) — https://docs.anthropic.com/en/docs/about-claude/models/overview
- OpenAI OpenAPI spec (Model object fields) — https://raw.githubusercontent.com/openai/openai-openapi/master/openapi.yaml
- OpenAI models page (flagship models, prices, "available via the Responses API") — https://platform.openai.com/docs/models
- Google Gemini API models reference — https://ai.google.dev/api/models
- DeepSeek models & pricing — https://api-docs.deepseek.com/quick_start/pricing
- DeepSeek list-models reference — https://api-docs.deepseek.com/api/list-models
- Ollama API docs — https://raw.githubusercontent.com/ollama/ollama/main/docs/api.md
- xAI models page (Grok 4.7 specs/pricing) — https://docs.x.ai/docs/models
- Mistral models overview (lineup) — https://docs.mistral.ai/getting-started/models/
- This repo — `crates/moe-platform/src/config.rs`, `docs/adr/0005-openai-compatible-endpoints-only-local-history.md`, `docs/research/ai-provider-libraries-rust.md`

## Verification notes

- **Catalog snapshots.** `api.json`, `models.json`, `catalog.json` and both LiteLLM files were fetched on 2026-10-09 (local file mtimes 22:04). `api.json?type=all` did **not** finish within a 60 s budget (>1.37 MB received); its total size is unknown, but `models.json?type=all` was measured in full (412,850 B, 451 entries) and the three extra keys are the `type: decision` models. One `-o /dev/null` probe once reported 319,057 B for `catalog.json` while a completed download wrote 5,786,207 B for the same URL; the discrepancy is unexplained (CDN behavior), so treat endpoint sizes as approximate.
- **models.dev repo layout.** Default branch is `dev`; `raw.githubusercontent.com/sst/models.dev/main/...` paths return 404. License read from the `dev` branch LICENSE (1,067 B).
- **LiteLLM update mechanism not verified.** Fetched README/CONTRIBUTING and the docs contributing page contain nothing about `model_prices_and_context_window.json`; entries' `source` URLs (e.g. DeepSeek's pricing page) suggest manual curation, but the PR/automation process is unconfirmed.
- **OpenRouter terms.** Only the usage-datasets CC BY 4.0 statement was located; the API-reference docs page for the models endpoint was not fetched (the guessed slug 404'd; the docs site is JS-heavy), and no license statement for the `/models` JSON was found. The endpoint itself is public and unauthenticated (fetched without a key).
- **Catalog vs docs conflicts found.** (1) `gpt-6-luna` output price $0.52 in models.dev vs $0.50 on OpenAI's models page (input matches at $0.10). (2) DeepSeek context 1,000,000 in models.dev vs `context_window: 1048576` in DeepSeek's own `/models` docs example (marketing "1M"). (3) Canonical (homepage) prices can differ from provider entries — Grok 4.7 shown as $1.60/$4.80 on models.dev's homepage vs xAI's $2/$6; Mistral Medium $0.40/$2.00 vs the `mistral` provider entry's $1.50/$7.50. This note uses provider entries for the sample table; DeepSeek rows are off-peak.
- **Rows not checked against provider docs.** Google, Alibaba, Moonshot AI, Zhipu AI, and Meta rows are catalog-only: their per-model specs were not compared against vendor pages (Google's and Mistral's endpoint *schemas* were verified; Mistral's docs confirm the lineup including Mistral Large 4 but not its token/pricing numbers). For Meta, models.dev's `meta` provider uses `api: https://api.meta.ai/v1` with `@ai-sdk/openai`; whether Meta's wire protocol is Responses-style was not verified. xAI `grok-4.7` max output (500,000) is catalog-only (docs confirmed context 500k and $2/$6, knowledge cutoff May 2026).
- **Fetch failures and workarounds.** `ai.google.dev`, `docs.x.ai`, and `docs.mistral.ai` were unreachable via `curl` in this environment but reachable via the fetch tool; `platform.openai.com` returned 403 for `curl` HEAD but its models page was fetched; OpenRouter's docs page for the models endpoint returned 404 on the guessed slug. The providers' fetchable host restrictions explain the varied doc coverage; anything unverified above is marked rather than inferred.
- **Correlated but independent "decision" niche.** `type: decision` in models.dev, OpenAI's "Decisions API" (docs nav), LiteLLM's `/v1/decisions` banner, and Ollama's `capabilities: ["decision"]` were each observed on fetched pages; the mapping between them is an observation, not a spec.
- **No live API calls with credentials** were made; all statements come from public JSON, public docs, or repository files. Nothing here was executed or benchmarked.
