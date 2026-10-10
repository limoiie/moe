//! The model picker (MOE-0019): the model catalog (MOE-0016) reaches the palette — every model
//! with its properties, searchable, and one Enter switches the active provider/model
//! (`[ai] provider` + `model` in config.toml, ADR-0040).
//!
//! The list is search-driven and capped: the rows are built from the catalog plus the config's
//! custom providers, ranked with the active provider first (newest first, undated last), filtered
//! by whitespace tokens against a per-row haystack that includes capability words — so
//! `vision tools` finds the models that have both.

use moe_core::contract::{Action, ActionKind, ActionResult, Item, MoeError};
use moe_platform::config::{MoeConfig, Protocol};
use moe_platform::keychain;
use moe_platform::model_catalog::{self, Catalog, Model as CatalogModel};

use crate::ai_client::providers;

/// Rows per query: the list is search-driven, and the cap keeps the live IPC payload small.
const ROW_LIMIT: usize = 50;

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
    rows.sort_by(|a, b| {
        b.current
            .cmp(&a.current)
            .then_with(|| release_date(b).cmp(release_date(a)))
            .then_with(|| a.provider_name.cmp(b.provider_name))
            .then_with(|| a.model_name.cmp(b.model_name))
    });
    rows.truncate(ROW_LIMIT);
    if rows.is_empty() {
        return ActionResult::list(vec![no_match_item(query)]);
    }
    ActionResult::list(rows.iter().map(|row| model_item(row, has_key)).collect())
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
    match found {
        Some(row) => {
            card.push_str(&format!(
                "**{} · {}** is now the active model (`[ai] provider` / `[ai] model` written to \
                 config.toml); existing conversations use it on their next message, and new chats \
                 (⌘N) start with it.\n\n---\n\n",
                row.provider_name, row.model_name
            ));
            card.push_str(&model_markdown(row, has_key(row.provider_id)));
        }
        None => {
            card.push_str(&format!(
                "`{provider}` · `{model}` is now the active model (`[ai] provider` / `[ai] model` \
                 written to config.toml).\n\n{}",
                key_state(provider, has_key(provider))
            ));
        }
    }
    Ok(ActionResult::detail(vec![card_item(&card)]))
}

/// Every selectable row: the catalog's providers plus config-only (custom) providers.
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
        for model_id in block.models.iter().flatten() {
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
    let mut subtitle = String::new();
    if row.current {
        subtitle.push_str("Current · ");
    }
    subtitle.push_str(row.provider_name);
    for fact in facts {
        subtitle.push_str(" · ");
        subtitle.push_str(&fact);
    }

    Item {
        id: format!("ai.model.{}.{}", row.provider_id, row.model_id),
        title: row.model_name.to_string(),
        subtitle: Some(subtitle),
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
    if let Some(cost) = &meta.cost {
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
        if !parts.is_empty() {
            md.push_str(&format!("**Pricing**: {}\n", parts.join(" · ")));
        }
    }
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
    if !dates.is_empty() {
        md.push_str(&format!("**Dates**: {}\n", dates.join(" · ")));
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
fn card_item(markdown: &str) -> Item {
    Item {
        id: "ai.model-card".into(),
        title: "Set AI Model".into(),
        subtitle: None,
        icon: Some("sparkles".into()),
        actions: vec![],
        payload: serde_json::Value::Null,
        detail: Some(markdown.to_string()),
        pending: false,
    }
}

/// Nothing matched: keep the user in the flow (fewer words, or a custom provider in config.toml).
fn no_match_item(query: &str) -> Item {
    Item {
        id: "ai.model-none".into(),
        title: "No model matches".into(),
        subtitle: None,
        icon: Some("search".into()),
        actions: vec![],
        payload: serde_json::Value::Null,
        detail: Some(format!(
            "No catalogued model matches \"{query}\". Try fewer words (a model name, a provider \
             name, or a capability such as `vision`), or add a custom provider in config.toml."
        )),
        pending: false,
    }
}

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
        let subtitle = first.subtitle.as_deref().unwrap();
        assert!(
            subtitle.contains("Current · DeepSeek · 1M ctx · tools · vision · reasoning · beta"),
            "{subtitle}"
        );
        assert_eq!(first.actions[0].id, "use");
        assert_eq!(first.actions[0].kind, ActionKind::Primary);
        assert_eq!(first.payload["provider"], "deepseek");
        assert_eq!(first.payload["model"], "deepseek-new");
        // A custom row: the provider's display name, no catalog facts
        let custom = items.last().unwrap();
        assert_eq!(custom.title, "c1");
        assert_eq!(custom.subtitle.as_deref(), Some("My Gateway"));
    }

    /// Search: every token must hit the haystack — names, ids, and capability words.
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
        // No hit: the notice keeps the flow
        let items = rows_of("nope");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "ai.model-none");
        assert!(items[0].detail.as_deref().unwrap().contains("nope"));
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
            ActionResult::List { items, detail_full } => {
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
