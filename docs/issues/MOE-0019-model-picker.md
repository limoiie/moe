# M8a: pick a model — the catalog reaches the palette

- **ID**: MOE-0019
- **State**: done
- **Labels**: ready-for-agent, feature, ux
- **Created**: 2026-10-10
- **Updated**: 2026-10-10
- **Assignee**: agent
- **Commit**: 10f5ad4

The owner's original ask included browsing models with their properties (context window, max
output, tools, images, reasoning, …). MOE-0016 embedded the catalog and MOE-0018 built the provider
layer, but both are config-file driven: the catalog's facts are still invisible in the product.
ADR-0009 excluded a settings UI from v1; this issue narrows that exclusion to one catalog browser +
pair selector (amendment recorded at delivery) — keys and provider blocks stay file-managed.

## Decisions

- **Surface**: one live `ai` command, `ai.choose-model` / "Set AI Model" (`InputKind::Query`,
  `live`): type to filter, the focused row's detail pane shows the model's properties, Enter (the
  primary "Use This Model" action) switches the active pair. No provider-management UI — saving
  keys stays `key <value>`, adding providers stays config blocks.
- **The list**: every catalog provider's models plus configured custom providers' models. Ranking:
  the active provider's models first, then the rest by release date (newest first, undated last;
  provider/model name break ties). The result is capped (50 rows) — the list is search-driven and
  the cap keeps the live IPC payload small. An empty query shows the ranked head (the active
  provider's models are what one normally switches between).
- **Matching**: every whitespace token must appear (case-insensitive substring) in the row's
  haystack: provider display name, provider id, model display name, model id, and capability words
  (`tools`, `vision`, `image`, `reasoning`, `text`) derived from the catalog flags — so
  "vision tools" finds the models that have both.
- **Row**: title = the catalog display name (or the model id for custom entries); subtitle =
  `Current · {Provider} · 1M ctx · tools · vision · reasoning` (only the facts the catalog
  carries); payload carries the `provider` + `model` ids.
- **Detail pane** (markdown): provider (display name, id, protocol), limits (context window / max
  output, formatted `1M`/`393k`), modalities (input/output), capability lines (tool calls,
  reasoning, structured output, attachments, open weights), pricing (input/output and cache per M
  tokens when present), knowledge cutoff / release date / status, the docs link, and the key state
  (its file path, or exactly how to save one). Custom-provider models (no catalog entry) show the
  configured facts and say the catalog has no metadata for them.
- **Apply**: `set_ai_selection_in_toml` writes `[ai] provider` + `model` in place (comment
  preserving — the `set_theme_in_toml` rule, ADR-0035 amendment), then a card confirms the pair,
  repeats the key state, and notes that existing conversations replay to the new model on their
  next message while new chats (⌘N) start with it (ADR-0040: one active pair in config; switching
  mid-conversation is accepted in v1, and agentic records keep their openai-chat gate).
- **Non-goals**: key entry inside the picker, provider add/edit UI, per-conversation model
  override, local (Ollama/LM Studio) model discovery, OAuth, editing catalog properties.

## Delivery

- `crates/moe-platform/src/config.rs`: `set_ai_selection_in_toml` / `store_ai_selection`, factored
  with the theme writer through one comment-preserving section editor + tests.
- `crates/moe-extensions/src/ai_models.rs` (new): catalog/config → ranked rows, the detail
  formatter, token-count formatting, the apply path (the store is injected for tests) + tests.
- `crates/moe-extensions/src/ai.rs`: the command meta, invoke routing, and `run_item_action`
  routing by command id.
- Docs at delivery (MOE-0012 pattern): ADR-0009 amendment (the narrow exception), README (the
  picker), CONTEXT (**Model Catalog** term).

## Acceptance

- Typing "model" in the palette finds "Set AI Model"; opening it lists the active provider's models
  newest-first, then every other provider's.
- Typing a provider name, a model fragment, or a capability word (`vision` / `tools` / `reasoning`)
  filters across providers; the detail pane shows context window, max output, modalities,
  capabilities, pricing, dates, docs, and the key state.
- Enter on a row writes `[ai] provider` / `model` into `config.toml` **without disturbing any other
  line (comments included)**, and the confirmation card names the pair and the key state; a
  provider without a key says exactly how to save one (the `key <value>` command and the file path).
- A custom `[ai.providers.<id>]` provider's models appear with their configured facts and no
  catalog metadata.
- Deprecated / alpha / beta models are listed with their status (selecting one is the user's call).
- `cargo test` and `pnpm build` green; no UI change is needed (the generic list + detail render).

## Comments

- 2026-10-10 (agent): filed + claimed per the owner's go-ahead ("file it as MOE-0019 and start on
  it"), immediately after MOE-0018. No grilling round this time: the decisions above are the
  natural continuation of ADR-0040's "the picker command that consumes the catalog comes later" and
  ADR-0009's excluded-settings-UI clause (narrowed here); they are recorded as settled for this
  implementation and can be amended in comments if the owner wants a different surface later.
- 2026-10-10 (agent): **done** (10f5ad4). `crates/moe-extensions/src/ai_models.rs`: the picker
  itself — `choose_model`/`choose_model_with` (rows from the catalog plus config-only custom
  providers; active provider first, then release date desc; token AND-matching over
  names/ids/capability words; 50-row cap), the detail formatter (limits/modalities/capabilities/
  pricing/dates/docs + the key state), `token_count`, and `apply_selection`/`apply_selection_with`
  (store injected; confirmation card reuses the detail). `config.rs` gained
  `set_ai_selection_in_toml`/`store_ai_selection`, factored with the theme writer into one
  comment-preserving `edit_section`/`set_section_assignment` editor (values escaped via
  `toml_string`, so the written file always parses). `model_catalog.rs` gained `Catalog::entries()`
  (key-preserving iteration — the fixture exposed that the entry's own `id` field is not
  guaranteed). `ai.rs`: the live `ai.choose-model` command, invoke routing, and `run_item_action`
  routing by command id. Docs: ADR-0009 amendment (the exception and why it is not a settings-
  panel precedent), README (the picker bullet), CONTEXT (**Model Catalog** term). Validation:
  `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo test --workspace` green
  (app 5, core 50, extensions 103, platform 67); `pnpm -C ui build` green (no UI change needed —
  the generic list + detail render the picker).

  Remaining manual: a visual pass in the running app (the row subtitle/detail layout, the search
  feel with the 730-model catalog) and selecting against a live key; the config write is covered
  by unit tests but has not been exercised through the real IPC path.
