//! The model picker (MOE-0028): the model catalog (MOE-0025) reaches the palette — every model
//! with its properties, searchable, and one Enter switches the active provider/model
//! (`[ai] provider` + `model` in config.toml, ADR-0040).
//!
//! The list is search-driven and capped: the rows are built from the catalog plus the config's
//! custom providers, ranked with the active provider first (newest first, undated last), filtered
//! by whitespace tokens against a per-row haystack that includes capability words — so
//! `vision tools` finds the models that have both.

use moe_core::contract::{
    Action, ActionKind, ActionResult, EmptyView, Filter, Item, Metadata, MoeError,
};
use moe_platform::config::{MoeConfig, Protocol, ProviderConfig};
use moe_platform::keychain;
use moe_platform::model_catalog::{self, Catalog, Cost, Model as CatalogModel};

use crate::ai_client::providers;

/// Rows per query: the list is search-driven, and the cap keeps the live IPC payload small.
const ROW_LIMIT: usize = 50;

/// Models per provider in the switcher card's unfiltered head (MOE-0030): a catalog-rich
/// provider (a hundred models) must not push every other provider past the row cap — the card's
/// first screen is the switching surface, so each provider gets a slice of it. A query lifts the
/// per-provider cap: search reaches the whole catalog.
const HEAD_PER_PROVIDER: usize = 10;

/// One selectable model, resolved from the catalog and the config.
struct Row<'a> {
    provider_id: &'a str,
    provider_name: &'a str,
    model_id: &'a str,
    model_name: &'a str,
    /// The catalogued metadata; `None` for a custom provider's model.
    meta: Option<&'a CatalogModel>,
    /// The effective protocol (config override, else the checked-in table).
    protocol: Option<Protocol>,
    /// The effective API URL (config override, else the catalog, else the table).
    api_url: Option<String>,
    /// The provider's documentation link (catalog).
    doc: Option<&'a str>,
    /// Prebuilt lowercase haystack: names, ids, and the capability words derived from the flags.
    haystack: String,
    current: bool,
}

/// The picker's entry: `query` filters the catalog, an empty query shows the ranked head.
pub fn choose_model(query: &str) -> ActionResult {
    let config = MoeConfig::load();
    let catalog = model_catalog::load();
    let has_key = |provider_id: &str| {
        keychain::ai_api_key_for(provider_id, &env_names(&catalog, provider_id)).is_some()
    };
    choose_model_with(query, &config, &catalog, &has_key)
}

/// The picker with injected catalog / key lookup (tests).
pub fn choose_model_with(
    query: &str,
    config: &MoeConfig,
    catalog: &Catalog,
    has_key: &dyn Fn(&str) -> bool,
) -> ActionResult {
    let mut rows = all_rows(config, catalog);
    filter_rows(&mut rows, query);
    // Provider-major so the list's group headers (`Item.group`, ADR-0018 amendment) never repeat:
    // the active provider first, then every other provider by name; newest first within a group.
    rank_rows(&mut rows);
    rows.truncate(ROW_LIMIT);
    if rows.is_empty() {
        return ActionResult::list_with_empty(
            Vec::new(),
            EmptyView::new(
                "No model matches",
                format!(
                    "No catalogued model matches \"{query}\". Try fewer words (a model name, a \
                     provider name, or a capability such as vision), or add a custom provider in \
                     config.toml."
                ),
            )
            .with_icon("search"),
        );
    }
    // The capability vocabulary reaches the platform as tags and one derived dimension (ADR-0043's
    // named follow-up). Same words the card shows, and the same client-side narrowing history uses —
    // no second code path, just a declaration.
    ActionResult::list(rows.iter().map(|row| model_item(row, has_key)).collect())
        .with_filter(Filter::derived("capability", "Capability"))
}

/// The picker's item action (routed here by `ai.rs`; `command_id == "ai.choose-model"`).
pub fn run_action(item: &Item, action: &Action) -> Result<ActionResult, MoeError> {
    match action.id.as_str() {
        "use" => apply_selection(item, |provider, model| {
            moe_platform::config::store_ai_selection(provider, model).map_err(|err| err.to_string())
        }),
        _ => Err(MoeError::NotFound),
    }
}

/// Apply: write the picked pair (the store is injected for tests), then confirm with the model's
/// detail card — the key state included, since a missing key is the next step.
pub fn apply_selection(
    item: &Item,
    store: impl FnOnce(&str, &str) -> Result<(), String>,
) -> Result<ActionResult, MoeError> {
    let config = MoeConfig::load();
    let catalog = model_catalog::load();
    let has_key = |id: &str| keychain::ai_api_key_for(id, &env_names(&catalog, id)).is_some();
    apply_selection_with(item, store, &config, &catalog, &has_key)
}

/// Apply with injected catalog/config/key lookup (tests).
pub fn apply_selection_with(
    item: &Item,
    store: impl FnOnce(&str, &str) -> Result<(), String>,
    config: &MoeConfig,
    catalog: &Catalog,
    has_key: &dyn Fn(&str) -> bool,
) -> Result<ActionResult, MoeError> {
    let provider = item
        .payload
        .get("provider")
        .and_then(|value| value.as_str())
        .ok_or(MoeError::NotFound)?;
    let model = item
        .payload
        .get("model")
        .and_then(|value| value.as_str())
        .ok_or(MoeError::NotFound)?;
    store(provider, model).map_err(MoeError::Internal)?;

    let rows = all_rows(config, catalog);
    let found = rows
        .iter()
        .find(|row| row.provider_id == provider && row.model_id == model);
    let mut card = String::from("## AI model set\n\n");
    // The confirmation card is a `detail`-shape page, so its facts ride the metadata column (ADR-0042)
    // instead of only prose: provider and model as labels, in the Extension's order.
    let label = |title: &str, text: &str| Metadata::Label {
        title: title.into(),
        icon: None,
        text: Some(text.into()),
        color: None,
    };
    let metadata = match found {
        Some(row) => {
            card.push_str(&format!(
                "**{} · {}** is now the active model (`[ai] provider` / `[ai] model` written to \
                 config.toml); existing conversations use it on their next message, and new chats \
                 (⌘N) start with it.\n\n---\n\n",
                row.provider_name, row.model_name
            ));
            card.push_str(&model_markdown(row, has_key(row.provider_id)));
            vec![
                label("Provider", row.provider_name),
                label("Model", row.model_name),
            ]
        }
        None => {
            card.push_str(&format!(
                "`{provider}` · `{model}` is now the active model (`[ai] provider` / `[ai] model` \
                 written to config.toml).\n\n{}",
                key_state(provider, has_key(provider))
            ));
            vec![label("Provider", provider), label("Model", model)]
        }
    };
    Ok(ActionResult::detail(vec![card_item(&card, metadata)]))
}

/// The picker's order (MOE-0028), shared by the picker list and the switcher card (MOE-0029):
/// the active provider first, then every other provider by name; newest first within a group.
fn rank_rows(rows: &mut Vec<Row<'_>>) {
    rows.sort_by(|a, b| {
        b.current
            .cmp(&a.current)
            .then_with(|| a.provider_name.cmp(b.provider_name))
            .then_with(|| release_date(b).cmp(release_date(a)))
            .then_with(|| a.model_name.cmp(b.model_name))
    });
}

/// Every selectable row: the catalog's providers plus config-only (custom) providers. Models the
/// settings window hid (`hidden_models`, MOE-0029) stay out of the pickers.
fn all_rows<'a>(config: &'a MoeConfig, catalog: &'a Catalog) -> Vec<Row<'a>> {
    let current_provider = config
        .ai
        .provider
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty());
    let mut rows = Vec::new();
    for (provider_id, provider) in catalog.entries() {
        let block = config.ai.providers.get(provider_id);
        let hidden = block
            .and_then(|block| block.hidden_models.as_deref())
            .unwrap_or_default();
        let protocol = block
            .and_then(|b| b.protocol)
            .or_else(|| providers::default_of(provider_id).map(|table| table.protocol));
        let api_url = block
            .and_then(|b| b.api_url.clone())
            .or_else(|| provider.api.clone());
        let provider_name: &str = if provider.name.trim().is_empty() {
            provider_id
        } else {
            &provider.name
        };
        for (model_id, model) in &provider.models {
            if hidden.contains(model_id) {
                continue;
            }
            rows.push(Row {
                provider_id,
                provider_name,
                model_id,
                model_name: model_name(model),
                haystack: haystack(
                    provider_name,
                    provider_id,
                    model_name(model),
                    model_id,
                    Some(model),
                ),
                meta: Some(model),
                protocol,
                api_url: api_url.clone(),
                doc: provider.doc.as_deref(),
                current: current_provider == Some(provider_id),
            });
        }
    }
    for (id, block) in &config.ai.providers {
        if catalog.get(id).is_some() {
            continue; // official providers are already listed above (with the block's overrides)
        }
        let name = block
            .name
            .as_deref()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(id);
        let hidden = block.hidden_models.as_deref().unwrap_or_default();
        for model_id in block.models.iter().flatten() {
            if hidden.contains(model_id) {
                continue;
            }
            rows.push(Row {
                provider_id: id,
                provider_name: name,
                model_id,
                model_name: model_id,
                haystack: haystack(name, id, model_id, model_id, None),
                meta: None,
                protocol: block.protocol,
                api_url: block.api_url.clone(),
                doc: None,
                current: current_provider == Some(id.as_str()),
            });
        }
    }
    rows
}

/// Keep the rows whose haystack contains every whitespace token of the query (case-insensitive);
/// an empty query keeps everything.
fn filter_rows(rows: &mut Vec<Row<'_>>, query: &str) {
    let tokens: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    if tokens.is_empty() {
        return;
    }
    rows.retain(|row| tokens.iter().all(|token| row.haystack.contains(token)));
}

/// The catalog display name when it exists (and is not just the id again), else the id.
fn model_name(model: &CatalogModel) -> &str {
    let name = model.name.trim();
    if name.is_empty() { &model.id } else { name }
}

/// Names, ids, and the capability words the picker's search understands (`vision tools`, …).
fn haystack(
    provider_name: &str,
    provider_id: &str,
    model_name: &str,
    model_id: &str,
    meta: Option<&CatalogModel>,
) -> String {
    let mut words = String::new();
    if let Some(meta) = meta {
        if meta.tool_call {
            words.push_str(" tools");
        }
        if meta.reasoning {
            words.push_str(" reasoning thinking");
        }
        if is_vision(meta) {
            words.push_str(" vision image");
        }
        if meta.attachment {
            words.push_str(" attachment");
        }
        for modality in &meta.modalities.input {
            words.push(' ');
            words.push_str(modality);
        }
        if let Some(status) = &meta.status {
            words.push(' ');
            words.push_str(status);
        }
    }
    format!("{provider_name} {provider_id} {model_name} {model_id}{words}").to_lowercase()
}

/// Whether the model takes images (the picker's "vision" word; modalities are authoritative,
/// `attachment` is the coarse flag).
fn is_vision(meta: &CatalogModel) -> bool {
    meta.attachment
        || meta
            .modalities
            .input
            .iter()
            .any(|modality| modality == "image")
}

fn release_date<'a>(row: &'a Row<'a>) -> &'a str {
    row.meta
        .and_then(|meta| meta.release_date.as_deref())
        .unwrap_or("")
}

/// One picker row: model name, provider + facts, the apply action, and the detail card.
fn model_item(row: &Row<'_>, has_key: &dyn Fn(&str) -> bool) -> Item {
    let facts = facts_of(row).join(" · ");

    Item {
        id: format!("ai.model.{}.{}", row.provider_id, row.model_id),
        title: row.model_name.to_string(),
        // The picker's filter dimension (ADR-0043): the catalog's capability words. The card already
        // shows them (`ModelDetail.capabilities`), so the tag is the same vocabulary reaching the list.
        tags: row.meta.map(capability_words).unwrap_or_default(),
        accessories: Vec::new(),
        metadata: Vec::new(),
        // The provider rides the group header (ADR-0018 amendment); the row carries the facts.
        group: Some(if row.current {
            format!("{} · Current", row.provider_name)
        } else {
            row.provider_name.to_string()
        }),
        subtitle: Some(facts).filter(|facts| !facts.is_empty()),
        icon: Some("sparkles".into()),
        actions: vec![Action {
            id: "use".into(),
            title: "Use This Model".into(),
            kind: ActionKind::Primary,
            keybinding: None,
        }],
        payload: serde_json::json!({
            "provider": row.provider_id,
            "model": row.model_id,
        }),
        detail: Some(model_markdown(row, has_key(row.provider_id))),
        pending: false,
    }
}

/// The headline facts of a row (picker subtitle, switcher card): context window, the capability
/// flags the catalog carries, and the lifecycle status.
fn facts_of(row: &Row<'_>) -> Vec<String> {
    let mut facts: Vec<String> = Vec::new();
    if let Some(context) = row
        .meta
        .and_then(|meta| meta.limit.as_ref())
        .and_then(|limit| limit.context)
    {
        facts.push(format!("{} ctx", token_count(context)));
    }
    if row.meta.is_some_and(|meta| meta.tool_call) {
        facts.push("tools".into());
    }
    if row.meta.is_some_and(is_vision) {
        facts.push("vision".into());
    }
    if row.meta.is_some_and(|meta| meta.reasoning) {
        facts.push("reasoning".into());
    }
    if let Some(status) = row.meta.and_then(|meta| meta.status.as_deref()) {
        facts.push(status.to_string());
    }
    facts
}

/// The settings row's chips (MOE-0029): the same vocabulary as [`facts_of`], the context window
/// as a bare token count (the column carries the unit).
fn model_facts(meta: &CatalogModel) -> Vec<String> {
    let mut facts: Vec<String> = Vec::new();
    if let Some(context) = meta.limit.as_ref().and_then(|limit| limit.context) {
        facts.push(token_count(context));
    }
    if meta.tool_call {
        facts.push("tools".into());
    }
    if is_vision(meta) {
        facts.push("vision".into());
    }
    if meta.reasoning {
        facts.push("reasoning".into());
    }
    if let Some(status) = &meta.status {
        facts.push(status.clone());
    }
    facts
}

/// The display name of a provider: the config block's name, else the catalog's, else the id.
fn provider_display_name(
    provider_id: &str,
    block: Option<&ProviderConfig>,
    catalog: &Catalog,
) -> String {
    block
        .and_then(|block| block.name.as_deref())
        .filter(|name| !name.trim().is_empty())
        .map(str::to_string)
        .or_else(|| {
            catalog.get(provider_id).map(|provider| {
                if provider.name.trim().is_empty() {
                    provider_id.to_string()
                } else {
                    provider.name.clone()
                }
            })
        })
        .unwrap_or_else(|| provider_id.to_string())
}

/// The catalog release date of a model (the settings order); undated sorts last.
fn release_date_of(meta: &CatalogModel) -> &str {
    meta.release_date.as_deref().unwrap_or("")
}

// ---- The settings window and the chat views' model switcher (MOE-0029) ----

/// The active provider/model pair: the chat views' model chip and the settings window's radios.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivePair {
    pub provider_id: String,
    pub provider_name: String,
    pub model_id: String,
    pub model_name: String,
}

/// The live active pair from config + catalog.
pub fn active_pair() -> Option<ActivePair> {
    let config = MoeConfig::load();
    let catalog = model_catalog::load();
    active_pair_with(&config, &catalog)
}

/// The active pair with injected config/catalog (tests): `[ai] provider` + `[ai] model`, named
/// from the catalog (or the config block, or the bare ids when neither knows them).
pub fn active_pair_with(config: &MoeConfig, catalog: &Catalog) -> Option<ActivePair> {
    let provider_id = config
        .ai
        .provider
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())?;
    let model_id = config
        .ai
        .model
        .as_deref()
        .map(str::trim)
        .filter(|model| !model.is_empty())?;
    let block = config.ai.providers.get(provider_id);
    let provider_name = provider_display_name(provider_id, block, catalog);
    let model_name = catalog
        .get(provider_id)
        .and_then(|provider| provider.models.get(model_id))
        .map(model_name)
        .unwrap_or(model_id)
        .to_string();
    Some(ActivePair {
        provider_id: provider_id.to_string(),
        provider_name,
        model_id: model_id.to_string(),
        model_name,
    })
}

/// One row of the chat views' model switcher card (MOE-0029/0030): the rows are bare names —
/// each provider's rows carry a resolvable key (the switcher only lists what can run), and the
/// focused row's facts live in the companion detail card (`ModelDetail`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelOption {
    pub provider_id: String,
    pub provider_name: String,
    pub model_id: String,
    pub model_name: String,
    /// The row is the active pair.
    pub current: bool,
    /// The companion card's structured facts.
    pub detail: ModelDetail,
}

/// The companion detail card's fields (MOE-0030 revisions): one field per rendered line — the card
/// pairs a muted label with a right-aligned value, so nothing has to be parsed or wrapped out of a
/// joined prose string.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDetail {
    /// Bare counts (`204k`, `1M`): the card owns the labels and units.
    pub context: Option<String>,
    pub max_output: Option<String>,
    /// The input modalities (`text`, `image`, `pdf`, …) and the output ones: the card draws them
    /// as the rounded tile row, `in → out`.
    pub modalities_in: Vec<String>,
    pub modalities_out: Vec<String>,
    /// Short tags: tool calls / reasoning / structured output / attachments / open weights / vision.
    pub capabilities: Vec<String>,
    /// The lifecycle status (`beta`, `deprecated`): a chip, never buried in a dates line.
    pub status: Option<String>,
    /// Per-million pricing, one row each: `$0.28/M tokens`.
    pub price_input: Option<String>,
    pub price_output: Option<String>,
    pub price_cache_read: Option<String>,
    pub price_cache_write: Option<String>,
    pub released: Option<String>,
    pub knowledge: Option<String>,
    pub doc: Option<String>,
}

/// The switcher card's rows for a query (empty = the ranked head, like the picker).
pub fn model_options(query: &str) -> Vec<ModelOption> {
    let config = MoeConfig::load();
    let catalog = model_catalog::load();
    let has_key = |provider_id: &str| {
        keychain::ai_api_key_for(provider_id, &env_names(&catalog, provider_id)).is_some()
    };
    model_options_with(query, &config, &catalog, &has_key)
}

/// The switcher rows with injected config/catalog/key lookup (tests). Only providers with a
/// resolvable key are listed (MOE-0030 revision: the switcher shows what can run; the palette
/// picker remains the catalog browser for unkeyed providers). An empty query returns a
/// provider-balanced head (active provider first, then each provider's newest); a query filters
/// the whole ranked list, so search reaches models the head cap left out.
pub fn model_options_with(
    query: &str,
    config: &MoeConfig,
    catalog: &Catalog,
    has_key: &dyn Fn(&str) -> bool,
) -> Vec<ModelOption> {
    let mut rows = all_rows(config, catalog);
    rows.retain(|row| has_key(row.provider_id));
    filter_rows(&mut rows, query);
    rank_rows(&mut rows);
    if query.trim().is_empty() {
        let mut seen: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
        rows.retain(|row| {
            let count = seen.entry(row.provider_id).or_insert(0);
            *count += 1;
            *count <= HEAD_PER_PROVIDER
        });
    } else {
        rows.truncate(ROW_LIMIT);
    }
    // The row check means the active pair, not the active provider (MOE-0030 revision): the
    // picker's provider-level "Current" belongs to its group headers, never to every row here.
    let active_model = config
        .ai
        .model
        .as_deref()
        .map(str::trim)
        .filter(|model| !model.is_empty());
    rows.iter()
        .map(|row| ModelOption {
            provider_id: row.provider_id.to_string(),
            provider_name: row.provider_name.to_string(),
            model_id: row.model_id.to_string(),
            model_name: row.model_name.to_string(),
            current: row.current && active_model == Some(row.model_id),
            detail: model_detail(row.meta, row.doc),
        })
        .collect()
}

/// The catalogued facts of one row as the detail card's fields; a custom provider's model has no
/// catalog metadata, so only the docs link lands.
fn model_detail(meta: Option<&CatalogModel>, doc: Option<&str>) -> ModelDetail {
    let limit = meta.and_then(|meta| meta.limit.as_ref());
    let cost = meta.and_then(|meta| meta.cost.as_ref());
    ModelDetail {
        context: limit.and_then(|limit| limit.context).map(token_count),
        max_output: limit.and_then(|limit| limit.output).map(token_count),
        modalities_in: meta
            .map(|meta| meta.modalities.input.clone())
            .unwrap_or_default(),
        modalities_out: meta
            .map(|meta| meta.modalities.output.clone())
            .unwrap_or_default(),
        capabilities: meta.map(capability_words).unwrap_or_default(),
        status: meta.and_then(|meta| meta.status.clone()),
        price_input: cost.and_then(|cost| cost.input).map(per_million),
        price_output: cost.and_then(|cost| cost.output).map(per_million),
        price_cache_read: cost.and_then(|cost| cost.cache_read).map(per_million),
        price_cache_write: cost.and_then(|cost| cost.cache_write).map(per_million),
        released: meta.and_then(|meta| meta.release_date.clone()),
        knowledge: meta.and_then(|meta| meta.knowledge.clone()),
        doc: doc.map(str::to_string),
    }
}

/// The capability labels the catalog carries (the picker's detail vocabulary); `vision` means the
/// model takes image input (the modality flag, not the coarser attachment flag).
fn capability_words(meta: &CatalogModel) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    if meta.tool_call {
        words.push("tool calls".into());
    }
    if meta.reasoning {
        words.push("reasoning".into());
    }
    if meta.structured_output {
        words.push("structured output".into());
    }
    if meta.attachment {
        words.push("attachments".into());
    }
    if meta.open_weights {
        words.push("open weights".into());
    }
    if meta
        .modalities
        .input
        .iter()
        .any(|modality| modality == "image")
    {
        words.push("vision".into());
    }
    words
}

/// `$0.28/M tokens` — one priced unit for a detail-card fact row (the picker's markdown keeps the
/// long form from [`pricing_of`]).
fn per_million(price_per_million: f64) -> String {
    format!("{}/M tokens", price(price_per_million))
}

/// `$0.28 in / $0.42 out per M tokens · cache read $0.028`; None when the catalog carries no cost.
fn pricing_of(cost: &Cost) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    if let (Some(input), Some(output)) = (cost.input, cost.output) {
        parts.push(format!(
            "{} in / {} out per M tokens",
            price(input),
            price(output)
        ));
    }
    if let Some(read) = cost.cache_read {
        parts.push(format!("cache read {}", price(read)));
    }
    if let Some(write) = cost.cache_write {
        parts.push(format!("cache write {}", price(write)));
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// `released 2026-08-21 · knowledge cutoff 2025-09 · status beta`; None when nothing is catalogued.
fn dates_of(meta: &CatalogModel) -> Option<String> {
    let mut dates: Vec<String> = Vec::new();
    if let Some(released) = &meta.release_date {
        dates.push(format!("released {released}"));
    }
    if let Some(knowledge) = &meta.knowledge {
        dates.push(format!("knowledge cutoff {knowledge}"));
    }
    if let Some(status) = &meta.status {
        dates.push(format!("status {status}"));
    }
    (!dates.is_empty()).then(|| dates.join(" · "))
}

/// One model row of the settings window (MOE-0029): the catalog's facts as chips, whether the
/// model shows in the pickers, and whether it is half of the active pair.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSettings {
    pub id: String,
    pub name: String,
    pub facts: Vec<String>,
    /// The model shows in the pickers (the `hidden_models` checkbox).
    pub enabled: bool,
    /// The model is half of the active pair.
    pub active: bool,
}

/// One provider group of the settings window (MOE-0029): the providers this install actually
/// uses — the configured blocks, the catalog providers with a resolvable key, the active one,
/// and the legacy implicit `default` endpoint (`base_url` without a provider).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSettings {
    pub id: String,
    pub name: String,
    /// The id comes from the model catalog (custom blocks are false).
    pub official: bool,
    pub protocol: Option<String>,
    pub api_url: Option<String>,
    pub doc: Option<String>,
    pub has_key: bool,
    /// A `[ai.providers.<id>]` block exists (the remove action edits config only then).
    pub has_block: bool,
    pub key_path: String,
    /// The catalog's env var names (official providers): how a key resolves without a file.
    pub env_names: Vec<String>,
    /// The provider is the active one (`[ai] provider`).
    pub active: bool,
    pub models: Vec<ModelSettings>,
}

/// The settings window's provider groups (key / visibility / active-pair management, MOE-0029).
pub fn settings_providers() -> Vec<ProviderSettings> {
    let config = MoeConfig::load();
    let catalog = model_catalog::load();
    let has_key = |provider_id: &str| {
        keychain::ai_api_key_for(provider_id, &env_names(&catalog, provider_id)).is_some()
    };
    settings_providers_with(&config, &catalog, &has_key)
}

/// The provider groups with injected config/catalog/key lookup (tests).
pub fn settings_providers_with(
    config: &MoeConfig,
    catalog: &Catalog,
    has_key: &dyn Fn(&str) -> bool,
) -> Vec<ProviderSettings> {
    let active_id = config
        .ai
        .provider
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty());
    let active_model = config
        .ai
        .model
        .as_deref()
        .map(str::trim)
        .filter(|model| !model.is_empty());

    let mut ids: Vec<String> = config.ai.providers.keys().cloned().collect();
    for (provider_id, _) in catalog.entries() {
        if !ids.iter().any(|id| id == provider_id) && has_key(provider_id) {
            ids.push(provider_id.to_string());
        }
    }
    if let Some(active_id) = active_id
        && !ids.iter().any(|id| id == active_id)
    {
        ids.push(active_id.to_string());
    }

    let mut providers: Vec<ProviderSettings> = ids
        .into_iter()
        .map(|id| {
            let block = config.ai.providers.get(&id);
            let protocol = block
                .and_then(|block| block.protocol)
                .or_else(|| providers::default_of(&id).map(|table| table.protocol));
            let api_url = block
                .and_then(|block| block.api_url.clone())
                .or_else(|| catalog.get(&id).and_then(|provider| provider.api.clone()));
            let hidden = block
                .and_then(|block| block.hidden_models.as_deref())
                .unwrap_or_default();
            let models: Vec<ModelSettings> = match catalog.get(&id) {
                Some(provider) => {
                    let mut catalogued: Vec<(&String, &CatalogModel)> =
                        provider.models.iter().collect();
                    catalogued.sort_by(|a, b| {
                        release_date_of(b.1)
                            .cmp(release_date_of(a.1))
                            .then_with(|| model_name(a.1).cmp(model_name(b.1)))
                    });
                    catalogued
                        .into_iter()
                        .map(|(model_id, meta)| ModelSettings {
                            id: model_id.clone(),
                            name: model_name(meta).to_string(),
                            facts: model_facts(meta),
                            enabled: !hidden.contains(model_id),
                            active: active_id == Some(id.as_str())
                                && active_model == Some(model_id.as_str()),
                        })
                        .collect()
                }
                None => block
                    .and_then(|block| block.models.clone())
                    .unwrap_or_default()
                    .into_iter()
                    .map(|model_id| ModelSettings {
                        active: active_id == Some(id.as_str())
                            && active_model == Some(model_id.as_str()),
                        enabled: !hidden.contains(&model_id),
                        facts: Vec::new(),
                        name: model_id.clone(),
                        id: model_id,
                    })
                    .collect(),
            };
            ProviderSettings {
                name: provider_display_name(&id, block, catalog),
                official: catalog.get(&id).is_some(),
                protocol: protocol.map(|protocol| protocol.as_str().to_string()),
                api_url,
                doc: catalog.get(&id).and_then(|provider| provider.doc.clone()),
                has_key: has_key(&id),
                has_block: block.is_some(),
                key_path: keychain::provider_key_path(&id)
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| format!("keys/{id}")),
                env_names: env_names(catalog, &id),
                active: active_id == Some(id.as_str()),
                models,
                id,
            }
        })
        .collect();

    // The legacy endpoint (ADR-0040's implicit `default` provider): one model, the configured one.
    if active_id.is_none()
        && config
            .ai
            .base_url
            .as_deref()
            .is_some_and(|url| !url.trim().is_empty())
    {
        let model = config.ai.model_or_default().to_string();
        providers.push(ProviderSettings {
            id: keychain::DEFAULT_PROVIDER_ID.to_string(),
            name: "Default endpoint".to_string(),
            official: false,
            protocol: Some(Protocol::OpenAiChat.as_str().to_string()),
            api_url: config.ai.base_url.clone(),
            doc: None,
            has_key: has_key(keychain::DEFAULT_PROVIDER_ID),
            has_block: config
                .ai
                .providers
                .contains_key(keychain::DEFAULT_PROVIDER_ID),
            key_path: keychain::provider_key_path(keychain::DEFAULT_PROVIDER_ID)
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| format!("keys/{}", keychain::DEFAULT_PROVIDER_ID)),
            env_names: Vec::new(),
            active: true,
            models: vec![ModelSettings {
                id: model.clone(),
                name: model.clone(),
                facts: Vec::new(),
                enabled: true,
                active: active_model.is_none_or(|active| active == model.as_str()),
            }],
        });
    }

    providers.sort_by(|a, b| b.active.cmp(&a.active).then_with(|| a.name.cmp(&b.name)));
    providers
}

/// A catalog provider the settings window does not manage yet (its "Connect a provider" search).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogProvider {
    pub id: String,
    pub name: String,
    pub protocol: Option<String>,
    pub doc: Option<String>,
    pub env_names: Vec<String>,
    pub model_count: usize,
}

/// The connectable catalog providers for a query (a name/id substring), excluding the groups the
/// settings window already lists; capped so the search card stays small.
pub fn catalog_providers(query: &str) -> Vec<CatalogProvider> {
    let config = MoeConfig::load();
    let catalog = model_catalog::load();
    let has_key = |provider_id: &str| {
        keychain::ai_api_key_for(provider_id, &env_names(&catalog, provider_id)).is_some()
    };
    catalog_providers_with(query, &config, &catalog, &has_key)
}

/// The connectable providers with injected config/catalog/key lookup (tests).
pub fn catalog_providers_with(
    query: &str,
    config: &MoeConfig,
    catalog: &Catalog,
    has_key: &dyn Fn(&str) -> bool,
) -> Vec<CatalogProvider> {
    let managed = settings_providers_with(config, catalog, has_key);
    let query = query.trim().to_lowercase();
    let mut out: Vec<CatalogProvider> = catalog
        .entries()
        .filter(|(id, _)| !managed.iter().any(|provider| provider.id == *id))
        .filter(|(id, provider)| {
            query.is_empty() || provider.name.to_lowercase().contains(&query) || id.contains(&query)
        })
        .map(|(id, provider)| CatalogProvider {
            id: id.to_string(),
            name: if provider.name.trim().is_empty() {
                id.to_string()
            } else {
                provider.name.clone()
            },
            protocol: providers::default_of(id).map(|table| table.protocol.as_str().to_string()),
            doc: provider.doc.clone(),
            env_names: provider.env.clone(),
            model_count: provider.models.len(),
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out.truncate(30);
    out
}

/// Toggle one model's picker visibility (the settings window's checkbox, MOE-0029): the
/// provider's `hidden_models` list gains or loses the model; the store is injected for tests.
pub fn set_model_visibility(
    provider: &str,
    model: &str,
    enabled: bool,
    store: impl FnOnce(&str, &[String]) -> Result<(), String>,
) -> Result<(), MoeError> {
    let config = MoeConfig::load();
    set_model_visibility_with(provider, model, enabled, store, &config)
}

/// Remove a provider from this install (the settings window's Remove, MOE-0032): the key file and
/// the config block go, and the active pair is cleared when it pointed at the provider — nothing
/// about it remains half-removed. A provider that was only the active pair (an official catalog id
/// without key or block) is removed by clearing that pair.
pub fn remove_provider(id: &str) -> Result<(), MoeError> {
    let config = MoeConfig::load();
    remove_provider_with(
        id,
        &config,
        keychain::remove_provider_key,
        |id, clear_selection| {
            moe_platform::config::store_provider_removal(id, clear_selection)
                .map_err(|err| err.to_string())
        },
    )
}

/// The removal with injected config and stores (tests).
pub fn remove_provider_with(
    id: &str,
    config: &MoeConfig,
    remove_key: impl FnOnce(&str) -> Result<(), String>,
    store: impl FnOnce(&str, bool) -> Result<(), String>,
) -> Result<(), MoeError> {
    let active = config.ai.provider.as_deref().map(str::trim) == Some(id);
    let has_block = config.ai.providers.contains_key(id);
    remove_key(id).map_err(MoeError::Internal)?;
    // A keyed-only official provider that is not the active pair needs no config edit.
    if has_block || active {
        store(id, active).map_err(MoeError::Internal)?;
    }
    Ok(())
}

/// The visibility toggle with an injected config (tests).
pub fn set_model_visibility_with(
    provider: &str,
    model: &str,
    enabled: bool,
    store: impl FnOnce(&str, &[String]) -> Result<(), String>,
    config: &MoeConfig,
) -> Result<(), MoeError> {
    let mut hidden = config
        .ai
        .providers
        .get(provider)
        .and_then(|block| block.hidden_models.clone())
        .unwrap_or_default();
    if enabled {
        hidden.retain(|hidden| hidden != model);
    } else if !hidden.iter().any(|hidden| hidden == model) {
        hidden.push(model.to_string());
    }
    store(provider, &hidden).map_err(MoeError::Internal)
}

/// The detail pane: the catalogued properties, the provider facts, and the key state.
fn model_markdown(row: &Row<'_>, has_key: bool) -> String {
    let mut md = format!("## {}\n\n", row.model_name);
    if row.model_name != row.model_id {
        md.push_str(&format!("`{}`\n\n", row.model_id));
    }
    let mut provider = format!(
        "**Provider**: {} (`{}`)",
        row.provider_name, row.provider_id
    );
    if let Some(protocol) = row.protocol {
        provider.push_str(&format!(" · **Protocol**: `{}`", protocol.as_str()));
    }
    md.push_str(&provider);
    md.push('\n');
    if let Some(api_url) = &row.api_url {
        md.push_str(&format!("**API URL**: `{api_url}`\n"));
    }
    match row.meta {
        Some(meta) => md.push_str(&catalog_facts(meta)),
        None => md
            .push_str("\nThis is a custom provider's model: the catalog has no metadata for it.\n"),
    }
    if let Some(doc) = row.doc {
        md.push_str(&format!("\n**Docs**: {doc}\n"));
    }
    md.push('\n');
    md.push_str(&key_state(row.provider_id, has_key));
    md
}

/// The catalogued facts, one bold label per line (absent fields are skipped).
fn catalog_facts(meta: &CatalogModel) -> String {
    let mut md = String::new();
    if let Some(limit) = &meta.limit {
        let mut parts: Vec<String> = Vec::new();
        if let Some(context) = limit.context {
            parts.push(format!("context {} tokens", token_count(context)));
        }
        if let Some(output) = limit.output {
            parts.push(format!("max output {} tokens", token_count(output)));
        }
        if !parts.is_empty() {
            md.push_str(&format!("\n**Limits**: {}\n", parts.join(" · ")));
        }
    }
    let input = list(&meta.modalities.input);
    let output = list(&meta.modalities.output);
    if !input.is_empty() || !output.is_empty() {
        md.push_str(&format!(
            "**Modalities**: input: {} · output: {}\n",
            if input.is_empty() { "—" } else { &input },
            if output.is_empty() { "—" } else { &output },
        ));
    }
    let mut capabilities: Vec<&str> = Vec::new();
    if meta.tool_call {
        capabilities.push("tool calls");
    }
    if meta.reasoning {
        capabilities.push("reasoning");
    }
    if meta.structured_output {
        capabilities.push("structured output");
    }
    if meta.attachment {
        capabilities.push("attachments");
    }
    if meta.open_weights {
        capabilities.push("open weights");
    }
    if !capabilities.is_empty() {
        md.push_str(&format!("**Capabilities**: {}\n", capabilities.join(" · ")));
    }
    if let Some(pricing) = meta.cost.as_ref().and_then(pricing_of) {
        md.push_str(&format!("**Pricing**: {pricing}\n"));
    }
    if let Some(dates) = dates_of(meta) {
        md.push_str(&format!("**Dates**: {dates}\n"));
    }
    md
}

fn list(values: &[String]) -> String {
    values.join(", ")
}

/// `0.28` → `$0.28`; `0.0` → `$0`; `3.0` → `$3` (f64's Display trims trailing zeros).
fn price(per_million: f64) -> String {
    format!("${per_million}")
}

/// `1_000_000` → `1M`, `393_216` → `393k`, `8_192` → `8k`, `512` → `512`; over a million keeps
/// one decimal when it is significant (`1_500_000` → `1.5M`).
pub fn token_count(tokens: u64) -> String {
    const K: u64 = 1_000;
    const M: u64 = 1_000_000;
    if tokens >= M {
        let whole = tokens / M;
        let tenth = (tokens % M) / (M / 10);
        return if tenth == 0 {
            format!("{whole}M")
        } else {
            format!("{whole}.{tenth}M")
        };
    }
    if tokens >= K {
        return format!("{}k", tokens / K);
    }
    tokens.to_string()
}

/// The key line: the provider's file, or how to save one (the picker never takes key input).
fn key_state(provider_id: &str, has_key: bool) -> String {
    let path = keychain::provider_key_path(provider_id)
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| format!("keys/{provider_id}"));
    if has_key {
        format!("**Key**: saved (`{path}`)\n")
    } else {
        format!(
            "**Key**: **missing** — type `key <your-key>` to save one (stored at `{path}`; \
             `MOE_AI_API_KEY` also works)\n"
        )
    }
}

fn env_names(catalog: &Catalog, provider_id: &str) -> Vec<String> {
    catalog
        .get(provider_id)
        .map(|provider| provider.env.clone())
        .unwrap_or_default()
}

/// The full-screen card a `detail` result renders (the setup cards' shape).
fn card_item(markdown: &str, metadata: Vec<Metadata>) -> Item {
    Item {
        id: "ai.model-card".into(),
        title: "Set AI Model".into(),
        subtitle: None,
        group: None,
        tags: Vec::new(),
        accessories: Vec::new(),
        metadata,
        icon: Some("sparkles".into()),
        actions: vec![],
        payload: serde_json::Value::Null,
        detail: Some(markdown.to_string()),
        pending: false,
    }
}

/// Nothing matched: an Empty View, not a notice row (ADR-0043) — the notice was focusable, so ⏎
/// could Apply "No model matches".
#[cfg(test)]
mod tests {
    use super::*;

    /// A small catalog: one official (table) provider with vision/tools/reasoning metadata, one
    /// drifted id the table does not know, and the config's custom provider below.
    fn catalog() -> Catalog {
        serde_json::from_str(
            r#"{
              "deepseek": {
                "name": "DeepSeek",
                "api": "https://api.deepseek.example/v1",
                "env": ["DEEPSEEK_API_KEY"],
                "doc": "https://api-docs.deepseek.example",
                "models": {
                  "deepseek-new": {
                    "name": "DeepSeek New",
                    "attachment": true,
                    "reasoning": true,
                    "tool_call": true,
                    "structured_output": true,
                    "modalities": { "input": ["text", "image"], "output": ["text"] },
                    "limit": { "context": 1000000, "output": 393216 },
                    "cost": { "input": 0.28, "output": 0.42, "cache_read": 0.028 },
                    "release_date": "2026-08-21",
                    "knowledge": "2025-09",
                    "status": "beta"
                  },
                  "deepseek-old": { "name": "DeepSeek Old", "release_date": "2024-01-01" }
                }
              },
              "beta": {
                "name": "Beta",
                "models": {
                  "beta-vision": {
                    "name": "Beta Vision",
                    "attachment": true,
                    "modalities": { "input": ["text", "image", "pdf"], "output": ["text"] },
                    "release_date": "2025-05-05"
                  }
                }
              }
            }"#,
        )
        .expect("fixture catalog parses")
    }

    fn config() -> MoeConfig {
        MoeConfig::from_toml(
            "[ai]\nprovider = \"deepseek\"\nmodel = \"deepseek-new\"\n\
             [ai.providers.custom]\nname = \"My Gateway\"\nprotocol = \"anthropic\"\n\
             api_url = \"https://gw.example\"\nmodels = [\"c1\"]\n",
        )
        .expect("fixture config parses")
    }

    fn key_ok(_provider_id: &str) -> bool {
        true
    }

    fn rows_of(query: &str) -> Vec<Item> {
        match choose_model_with(query, &config(), &catalog(), &key_ok) {
            ActionResult::List { items, .. } => items,
            other => panic!("expected a list, got {other:?}"),
        }
    }

    /// Ranking: the active provider's models first (newest first), then the rest by release date,
    /// then undated custom models; the row subtitle carries the headline facts and the Current mark.
    #[test]
    fn rows_rank_the_active_provider_first_and_carry_the_facts() {
        let items = rows_of("");
        let ids: Vec<&str> = items.iter().map(|item| item.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![
                "ai.model.deepseek.deepseek-new",
                "ai.model.deepseek.deepseek-old",
                "ai.model.beta.beta-vision",
                "ai.model.custom.c1",
            ]
        );
        let first = &items[0];
        assert_eq!(first.title, "DeepSeek New");
        assert_eq!(first.group.as_deref(), Some("DeepSeek · Current"));
        assert_eq!(
            first.subtitle.as_deref(),
            Some("1M ctx · tools · vision · reasoning · beta")
        );
        assert_eq!(first.actions[0].id, "use");
        assert_eq!(first.actions[0].kind, ActionKind::Primary);
        assert_eq!(first.payload["provider"], "deepseek");
        assert_eq!(first.payload["model"], "deepseek-new");
        // Providers are contiguous groups (headers never repeat in the render)
        let groups: Vec<Option<&str>> = items.iter().map(|item| item.group.as_deref()).collect();
        assert_eq!(
            groups,
            vec![
                Some("DeepSeek · Current"),
                Some("DeepSeek · Current"),
                Some("Beta"),
                Some("My Gateway"),
            ]
        );
        // A custom row: its provider rides the group header, and it has no catalog facts
        let custom = items.last().unwrap();
        assert_eq!(custom.title, "c1");
        assert_eq!(custom.subtitle, None);
    }

    /// Search: every token must hit the haystack — names, ids, and capability words.
    /// The picker's capability dimension (ADR-0043's named follow-up): the words that were only a
    /// search haystack become `Item.tags` behind one derived dimension, so the Filter Control appears
    /// here with no new filtering code — the platform derives the values from the tags.
    #[test]
    fn picker_tags_capabilities_behind_one_derived_dimension() {
        let ActionResult::List { filter, .. } =
            choose_model_with("", &config(), &catalog(), &key_ok)
        else {
            panic!("expected a list");
        };
        let filter = filter.expect("the picker declares its dimension");
        assert_eq!(filter.id, "capability");
        assert_eq!(filter.label, "Capability");
        assert!(
            filter.values.is_empty(),
            "derived from the tags, not declared"
        );

        // The tags are the card's own capability vocabulary, so the list and the card agree.
        let row = rows_of("deepseek-new");
        assert_eq!(row.len(), 1);
        for word in ["tool calls", "reasoning", "vision"] {
            assert!(
                row[0].tags.iter().any(|tag| tag == word),
                "missing {word}: {:?}",
                row[0].tags
            );
        }

        // A model the catalog knows nothing about carries none, so All still reaches it.
        let custom = rows_of("gateway");
        assert_eq!(custom.len(), 1);
        assert!(custom[0].tags.is_empty(), "{:?}", custom[0].tags);
    }

    #[test]
    fn query_matches_names_and_capability_words() {
        let ids: Vec<String> = rows_of("vision")
            .iter()
            .map(|item| item.id.clone())
            .collect();
        assert_eq!(
            ids,
            vec![
                "ai.model.deepseek.deepseek-new",
                "ai.model.beta.beta-vision"
            ]
        );
        let ids: Vec<String> = rows_of("gateway")
            .iter()
            .map(|item| item.id.clone())
            .collect();
        assert_eq!(ids, vec!["ai.model.custom.c1"]);
        // Two tokens: both must be present (DeepSeek New has tools + reasoning)
        let ids: Vec<String> = rows_of("tools reasoning")
            .iter()
            .map(|item| item.id.clone())
            .collect();
        assert_eq!(ids, vec!["ai.model.deepseek.deepseek-new"]);
        // No hit: an empty list that declares an Empty View (ADR-0043) — the notice row is gone, so
        // nothing is focusable to Apply.
        let empty = match choose_model_with("nope", &config(), &catalog(), &key_ok) {
            ActionResult::List { items, empty, .. } => {
                assert!(items.is_empty(), "the zero-row case is an empty list");
                empty.expect("a no-hit picker declares its Empty View")
            }
            other => panic!("expected a list, got {other:?}"),
        };
        assert_eq!(empty.title, "No model matches");
        assert!(empty.description.as_deref().unwrap().contains("nope"));
    }

    /// The cap keeps the live IPC payload small.
    #[test]
    fn rows_are_capped() {
        let mut providers_json = String::from("{\"big\": {\"name\": \"Big\", \"models\": {");
        for index in 0..(ROW_LIMIT + 10) {
            if index > 0 {
                providers_json.push(',');
            }
            providers_json.push_str(&format!("\"m{index:03}\": {{\"name\": \"M {index}\"}}"));
        }
        providers_json.push_str("}}}");
        let catalog: Catalog = serde_json::from_str(&providers_json).expect("big catalog");
        let config = MoeConfig::default();
        let rows = match choose_model_with("", &config, &catalog, &key_ok) {
            ActionResult::List { items, .. } => items,
            other => panic!("expected a list, got {other:?}"),
        };
        assert_eq!(rows.len(), ROW_LIMIT);
    }

    /// The detail pane carries the catalogued properties, the docs link, and the key path.
    #[test]
    fn detail_lists_the_catalog_properties() {
        let items = rows_of("deepseek-new");
        let detail = items[0].detail.as_deref().unwrap();
        assert!(detail.contains("## DeepSeek New"), "{detail}");
        assert!(
            detail.contains("**Provider**: DeepSeek (`deepseek`) · **Protocol**: `openai-chat`"),
            "{detail}"
        );
        assert!(
            detail.contains("**API URL**: `https://api.deepseek.example/v1`"),
            "{detail}"
        );
        assert!(
            detail.contains("context 1M tokens · max output 393k tokens"),
            "{detail}"
        );
        assert!(
            detail.contains("input: text, image · output: text"),
            "{detail}"
        );
        assert!(
            detail.contains("tool calls · reasoning · structured output · attachments"),
            "{detail}"
        );
        assert!(
            detail.contains("$0.28 in / $0.42 out per M tokens · cache read $0.028"),
            "{detail}"
        );
        assert!(
            detail.contains("released 2026-08-21 · knowledge cutoff 2025-09 · status beta"),
            "{detail}"
        );
        assert!(
            detail.contains("**Docs**: https://api-docs.deepseek.example"),
            "{detail}"
        );
        assert!(detail.contains("**Key**: saved ("), "{detail}");
    }

    /// A custom provider's model: the configured facts, no catalog metadata, the provider's key
    /// state (env names come from the catalog, so custom providers only have their file).
    #[test]
    fn custom_models_show_configured_facts_and_the_missing_key_hint() {
        let has_no_key = |_provider_id: &str| false;
        let items = match choose_model_with("c1", &config(), &catalog(), &has_no_key) {
            ActionResult::List { items, .. } => items,
            other => panic!("expected a list, got {other:?}"),
        };
        let detail = items[0].detail.as_deref().unwrap();
        assert!(detail.contains("## c1"), "{detail}");
        assert!(
            detail.contains("**Provider**: My Gateway (`custom`) · **Protocol**: `anthropic`"),
            "{detail}"
        );
        assert!(
            detail.contains("**API URL**: `https://gw.example`"),
            "{detail}"
        );
        assert!(detail.contains("the catalog has no metadata"), "{detail}");
        assert!(
            detail.contains("**Key**: **missing** — type `key <your-key>`"),
            "{detail}"
        );
        assert!(detail.contains("keys/custom"), "{detail}");
    }

    /// Apply: the store receives the payload's pair, and the card repeats the model's facts.
    #[test]
    fn apply_writes_the_pair_and_confirms_with_the_detail() {
        let items = rows_of("deepseek-new");
        let seen = std::sync::Mutex::new(Vec::new());
        let result = apply_selection_with(
            &items[0],
            |provider, model| {
                seen.lock()
                    .unwrap()
                    .push((provider.to_string(), model.to_string()));
                Ok(())
            },
            &config(),
            &catalog(),
            &key_ok,
        )
        .expect("applies");
        assert_eq!(
            seen.lock().unwrap().as_slice(),
            [("deepseek".to_string(), "deepseek-new".to_string())]
        );
        match result {
            ActionResult::List {
                items, detail_full, ..
            } => {
                assert!(detail_full, "the confirmation is a full-screen card");
                let card = items[0].detail.as_deref().unwrap();
                assert!(card.contains("## AI model set"), "{card}");
                assert!(
                    card.contains("**DeepSeek · DeepSeek New** is now the active model"),
                    "{card}"
                );
                assert!(
                    card.contains("context 1M tokens"),
                    "the detail rides along: {card}"
                );
            }
            other => panic!("expected a detail card, got {other:?}"),
        }

        // A store failure surfaces as an error, and a payload-less item is not found
        let failed = apply_selection_with(
            &items[0],
            |_, _| Err("disk full".into()),
            &config(),
            &catalog(),
            &key_ok,
        );
        assert!(failed.is_err());
        let bare = Item {
            id: "x".into(),
            title: "x".into(),
            subtitle: None,
            group: None,
            tags: Vec::new(),
            accessories: Vec::new(),
            metadata: Vec::new(),
            icon: None,
            actions: vec![],
            payload: serde_json::Value::Null,
            detail: None,
            pending: false,
        };
        assert!(
            apply_selection_with(&bare, |_, _| Ok(()), &config(), &catalog(), &key_ok).is_err()
        );
    }

    /// The settings window (MOE-0029): one group per provider in use — the active one first, then
    /// by name — carrying the key state and per-model visibility / active flags.
    #[test]
    fn settings_lists_the_providers_in_use_with_visibility_and_active_flags() {
        let providers = settings_providers_with(&config(), &catalog(), &key_ok);
        let ids: Vec<&str> = providers.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, vec!["deepseek", "beta", "custom"]);

        let deepseek = &providers[0];
        assert!(deepseek.active);
        assert!(deepseek.official);
        assert_eq!(deepseek.protocol.as_deref(), Some("openai-chat"));
        assert!(deepseek.has_key);
        assert!(!deepseek.env_names.is_empty());
        let models: Vec<(&str, bool, bool)> = deepseek
            .models
            .iter()
            .map(|m| (m.id.as_str(), m.enabled, m.active))
            .collect();
        assert_eq!(
            models,
            vec![("deepseek-new", true, true), ("deepseek-old", true, false)]
        );
        assert_eq!(
            deepseek.models[0].facts,
            vec!["1M", "tools", "vision", "reasoning", "beta"]
        );

        let custom = providers.iter().find(|p| p.id == "custom").unwrap();
        assert!(!custom.official);
        assert_eq!(custom.protocol.as_deref(), Some("anthropic"));
        assert_eq!(custom.models.len(), 1);
        assert!(custom.models[0].enabled);
        assert!(!custom.models[0].active);
    }

    /// Hidden models (MOE-0029): they leave the picker and the switcher but stay in the settings
    /// list (unchecked), and the visibility toggle edits `hidden_models` both ways.
    #[test]
    fn hidden_models_leave_the_pickers_and_the_toggle_round_trips() {
        let mut config = config();
        config.ai.providers.insert(
            "deepseek".into(),
            ProviderConfig {
                hidden_models: Some(vec!["deepseek-old".to_string()]),
                ..Default::default()
            },
        );
        let ids: Vec<String> = match choose_model_with("", &config, &catalog(), &key_ok) {
            ActionResult::List { items, .. } => items.into_iter().map(|item| item.id).collect(),
            other => panic!("expected a list, got {other:?}"),
        };
        assert!(!ids.contains(&"ai.model.deepseek.deepseek-old".to_string()));
        let options = model_options_with("", &config, &catalog(), &key_ok);
        assert!(
            !options
                .iter()
                .any(|option| option.model_id == "deepseek-old")
        );

        let providers = settings_providers_with(&config, &catalog(), &key_ok);
        let old = providers[0]
            .models
            .iter()
            .find(|m| m.id == "deepseek-old")
            .unwrap();
        assert!(!old.enabled);

        // Hide one more, then show the first again: the stored list follows both ways
        let mut stored: Vec<(String, Vec<String>)> = Vec::new();
        set_model_visibility_with(
            "deepseek",
            "deepseek-new",
            false,
            |provider, hidden| {
                stored.push((provider.to_string(), hidden.to_vec()));
                Ok(())
            },
            &config,
        )
        .expect("hides");
        assert_eq!(
            stored.last().unwrap(),
            &(
                "deepseek".to_string(),
                vec!["deepseek-old".into(), "deepseek-new".into()]
            )
        );
        let mut hidden_config = config.clone();
        hidden_config
            .ai
            .providers
            .get_mut("deepseek")
            .unwrap()
            .hidden_models = Some(stored.last().unwrap().1.clone());
        set_model_visibility_with(
            "deepseek",
            "deepseek-old",
            true,
            |provider, hidden| {
                stored.push((provider.to_string(), hidden.to_vec()));
                Ok(())
            },
            &hidden_config,
        )
        .expect("shows");
        assert_eq!(stored.last().unwrap().1, vec!["deepseek-new".to_string()]);
    }

    /// The chip and the switcher card (MOE-0029): the active pair is named from the catalog, the
    /// card ranks like the picker, and its query filters the same way.
    #[test]
    fn active_pair_and_switcher_rows_carry_the_current_pair() {
        let pair = active_pair_with(&config(), &catalog()).unwrap();
        assert_eq!(pair.provider_id, "deepseek");
        assert_eq!(pair.provider_name, "DeepSeek");
        assert_eq!(pair.model_id, "deepseek-new");
        assert_eq!(pair.model_name, "DeepSeek New");

        let options = model_options_with("", &config(), &catalog(), &key_ok);
        assert!(options[0].current);
        assert_eq!(
            options.iter().filter(|option| option.current).count(),
            1,
            "exactly the active pair carries the check, never the whole provider"
        );
        assert_eq!(options[0].detail.context.as_deref(), Some("1M"));
        assert_eq!(options[0].detail.max_output.as_deref(), Some("393k"));
        assert_eq!(options[0].detail.modalities_in, vec!["text", "image"]);
        assert_eq!(options[0].detail.modalities_out, vec!["text"]);
        assert_eq!(
            options[0].detail.capabilities,
            vec![
                "tool calls",
                "reasoning",
                "structured output",
                "attachments",
                "vision"
            ]
        );
        assert_eq!(options[0].detail.status.as_deref(), Some("beta"));
        assert_eq!(
            options[0].detail.price_input.as_deref(),
            Some("$0.28/M tokens")
        );
        assert_eq!(
            options[0].detail.price_output.as_deref(),
            Some("$0.42/M tokens")
        );
        assert_eq!(
            options[0].detail.price_cache_read.as_deref(),
            Some("$0.028/M tokens")
        );
        assert_eq!(options[0].detail.price_cache_write, None);
        assert_eq!(options[0].detail.released.as_deref(), Some("2026-08-21"));
        assert_eq!(options[0].detail.knowledge.as_deref(), Some("2025-09"));
        let vision = model_options_with("vision", &config(), &catalog(), &key_ok);
        assert_eq!(vision.len(), 2);
    }

    /// The switcher only lists what can run (MOE-0030 revision): providers without a resolvable
    /// key leave the card entirely — the active provider included, search included; the palette
    /// picker keeps browsing them for discovery.
    #[test]
    fn switcher_lists_only_keyed_providers() {
        let only_beta = |provider_id: &str| provider_id == "beta";
        let options = model_options_with("", &config(), &catalog(), &only_beta);
        assert!(!options.is_empty());
        assert!(options.iter().all(|option| option.provider_id == "beta"));

        // An unkeyed provider never surfaces, not even through a search
        let found = model_options_with("deepseek", &config(), &catalog(), &only_beta);
        assert!(found.is_empty());
    }

    /// The switcher's unfiltered head is provider-balanced (MOE-0030): a catalog-rich provider
    /// cannot push every other provider out of the card's first screen — but a query reaches the
    /// models the head cap left out.
    #[test]
    fn switcher_head_caps_per_provider_and_search_reaches_beyond() {
        let catalog: Catalog = serde_json::from_str(
            r#"{
              "rich": {
                "name": "Rich",
                "models": {
                  "m01": { "name": "M 01" }, "m02": { "name": "M 02" },
                  "m03": { "name": "M 03" }, "m04": { "name": "M 04" },
                  "m05": { "name": "M 05" }, "m06": { "name": "M 06" },
                  "m07": { "name": "M 07" }, "m08": { "name": "M 08" },
                  "m09": { "name": "M 09" }, "m10": { "name": "M 10" },
                  "m11": { "name": "M 11" }, "m12": { "name": "M 12" }
                }
              },
              "small": {
                "name": "Small",
                "models": { "s1": { "name": "S 1" } }
              }
            }"#,
        )
        .expect("fixture catalog parses");
        let config = MoeConfig::from_toml("[ai]\nprovider = \"rich\"\nmodel = \"m01\"\n").unwrap();

        let head = model_options_with("", &config, &catalog, &key_ok);
        let rich = head.iter().filter(|o| o.provider_id == "rich").count();
        let small = head.iter().filter(|o| o.provider_id == "small").count();
        assert_eq!(rich, 10, "the active provider's head is capped too");
        assert_eq!(small, 1, "the second provider stays reachable in the head");

        // Search lifts the cap: the eleventh model is findable
        let found = model_options_with("m11", &config, &catalog, &key_ok);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].model_id, "m11");
    }

    /// The settings window's Remove (MOE-0032): the key file always goes; the config is edited
    /// only for a block or the active pair — with the selection cleared exactly then.
    #[test]
    fn remove_provider_clears_the_key_block_and_active_selection() {
        let config = config();
        let mut removed_keys: Vec<String> = Vec::new();
        let mut stored: Vec<(String, bool)> = Vec::new();

        // The active official provider without a block: the pair is cleared
        remove_provider_with(
            "deepseek",
            &config,
            |id| {
                removed_keys.push(id.to_string());
                Ok(())
            },
            |id, clear| {
                stored.push((id.to_string(), clear));
                Ok(())
            },
        )
        .expect("removes");
        assert_eq!(removed_keys, vec!["deepseek"]);
        assert_eq!(stored, vec![("deepseek".to_string(), true)]);

        // A custom block that is not the active pair: the block goes, the selection stays
        stored.clear();
        remove_provider_with(
            "custom",
            &config,
            |_| Ok(()),
            |id, clear| {
                stored.push((id.to_string(), clear));
                Ok(())
            },
        )
        .expect("removes");
        assert_eq!(stored, vec![("custom".to_string(), false)]);

        // Neither block nor active pair (a keyed-only official provider): no config edit at all
        stored.clear();
        remove_provider_with(
            "beta",
            &config,
            |_| Ok(()),
            |id, clear| {
                stored.push((id.to_string(), clear));
                Ok(())
            },
        )
        .expect("removes");
        assert!(stored.is_empty());
    }

    /// Legacy configs (`base_url`, no provider): the settings window shows the implicit `default`
    /// provider with its one configured model, and no pair is active until one is picked.
    #[test]
    fn legacy_base_url_configs_show_as_the_default_provider() {
        let config =
            MoeConfig::from_toml("[ai]\nbase_url = \"https://x/v1\"\nmodel = \"m-1\"\n").unwrap();
        let providers = settings_providers_with(&config, &catalog(), &|_: &str| false);
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].id, "default");
        assert!(providers[0].active);
        assert_eq!(providers[0].api_url.as_deref(), Some("https://x/v1"));
        assert_eq!(providers[0].models.len(), 1);
        assert!(providers[0].models[0].active);
        assert_eq!(active_pair_with(&config, &catalog()), None);
    }

    /// The settings window's "Connect a provider" search: unmanaged catalog providers only.
    #[test]
    fn catalog_providers_excludes_the_managed_groups() {
        let all = catalog_providers_with("", &config(), &catalog(), &|_: &str| false);
        let ids: Vec<&str> = all.iter().map(|p| p.id.as_str()).collect();
        // deepseek (active) and custom (block) are managed; beta is merely catalogued
        assert_eq!(ids, vec!["beta"]);
        assert!(catalog_providers_with("deep", &config(), &catalog(), &|_: &str| false).is_empty());
        // With a key, beta becomes managed and drops out of the search
        let keyed = catalog_providers_with("", &config(), &catalog(), &|id: &str| id == "beta");
        assert!(keyed.is_empty());
    }

    /// Token formatting: plain below a thousand, `k` truncated, `M` with one significant decimal.
    #[test]
    fn token_counts_format_compactly() {
        assert_eq!(token_count(512), "512");
        assert_eq!(token_count(8_192), "8k");
        assert_eq!(token_count(128_000), "128k");
        assert_eq!(token_count(393_216), "393k");
        assert_eq!(token_count(1_000_000), "1M");
        assert_eq!(token_count(1_500_000), "1.5M");
    }
}
