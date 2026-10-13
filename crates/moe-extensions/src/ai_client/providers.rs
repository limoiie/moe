//! Provider resolution (ADR-0040): the config's provider/model selection × the model catalog ×
//! the checked-in protocol table → everything one request needs.
//!
//! Official providers need no config block: the base URL comes from the catalog (`api`) or the
//! table below, the model list and its properties from the catalog; the protocol comes from the
//! table — hand-checked per provider, with the citation the choice rests on (the catalog's `npm`
//! field is the hint). Custom providers state `protocol` + `api_url` + `models` themselves.

use moe_platform::config::{MoeConfig, Protocol};
use moe_platform::keychain;
use moe_platform::model_catalog::{Catalog, Provider as CatalogProvider};

use super::Reasoning;

/// The `max_tokens` Moe asks for when the protocol requires one (Anthropic); clamped to the
/// catalogued output limit when the model carries one.
const DEFAULT_MAX_TOKENS: u64 = 8192;

/// One provider's checked-in default: the protocol it speaks, the base URL to use when the
/// catalog has none, and where the choice was verified.
pub struct ProviderDefault {
    pub protocol: Protocol,
    pub api_url: &'static str,
    pub citation: &'static str,
}

/// The table (ADR-0040): official provider id → protocol, fallback base URL, citation.
/// Reviewed like the catalog snapshot; refresh it when a vendor changes its surface.
const OFFICIAL: &[(&str, Protocol, &str, &str)] = &[
    (
        "anthropic",
        Protocol::Anthropic,
        "https://api.anthropic.com",
        "https://docs.anthropic.com/en/api/messages",
    ),
    (
        "openai",
        Protocol::OpenAiChat,
        "https://api.openai.com/v1",
        "https://platform.openai.com/docs/api-reference/chat",
    ),
    (
        "google",
        Protocol::OpenAiChat,
        "https://generativelanguage.googleapis.com/v1beta/openai",
        "https://ai.google.dev/gemini-api/docs/openai",
    ),
    (
        "deepseek",
        Protocol::OpenAiChat,
        "https://api.deepseek.com",
        "https://api-docs.deepseek.com",
    ),
    (
        "xai",
        Protocol::OpenAiChat,
        "https://api.x.ai/v1",
        "https://docs.x.ai/docs/overview",
    ),
    (
        "mistral",
        Protocol::OpenAiChat,
        "https://api.mistral.ai/v1",
        "https://docs.mistral.ai/getting-started/models/",
    ),
    (
        "groq",
        Protocol::OpenAiChat,
        "https://api.groq.com/openai/v1",
        "https://console.groq.com/docs/openai",
    ),
    (
        "openrouter",
        Protocol::OpenAiChat,
        "https://openrouter.ai/api/v1",
        "https://openrouter.ai/docs/api-reference/overview",
    ),
    (
        "perplexity",
        Protocol::OpenAiChat,
        "https://api.perplexity.ai",
        "https://docs.perplexity.ai",
    ),
    (
        "togetherai",
        Protocol::OpenAiChat,
        "https://api.together.xyz/v1",
        "https://docs.together.ai/docs/openai-api-compatibility",
    ),
    (
        "cohere",
        Protocol::OpenAiChat,
        "https://api.cohere.ai/compatibility/v1",
        "https://docs.cohere.com/docs/compatibility-api",
    ),
    (
        "alibaba",
        Protocol::OpenAiChat,
        "https://dashscope-intl.aliyuncs.com/compatible-mode/v1",
        "https://www.alibabacloud.com/help/en/model-studio/compatibility-of-openai-with-dashscope",
    ),
    (
        "meta",
        Protocol::OpenAiChat,
        "https://api.meta.ai/v1",
        "https://dev.meta.ai/docs",
    ),
    (
        "moonshotai",
        Protocol::OpenAiChat,
        "https://api.moonshot.ai/v1",
        "https://platform.moonshot.ai/docs/api/chat",
    ),
    (
        "zai",
        Protocol::OpenAiChat,
        "https://api.z.ai/api/paas/v4",
        "https://docs.z.ai",
    ),
    (
        "zhipuai",
        Protocol::OpenAiChat,
        "https://open.bigmodel.cn/api/paas/v4",
        "https://docs.z.ai",
    ),
    (
        "lmstudio",
        Protocol::OpenAiChat,
        "http://127.0.0.1:1234/v1",
        "https://lmstudio.ai/docs/app/api/endpoints/openai",
    ),
    (
        // Local, keyless, and not in the catalog snapshot: a table entry so `provider = "ollama"`
        // works with zero config; discovery of its models is out of scope (ADR-0040).
        "ollama",
        Protocol::OpenAiChat,
        "http://localhost:11434/v1",
        "https://docs.ollama.com/api/openai-compatibility",
    ),
    (
        // MiniMax's Anthropic-compatible surface (models.dev's npm hint @ai-sdk/anthropic): the
        // base already carries the version root, so the endpoint join adds only `/messages`.
        "minimax",
        Protocol::Anthropic,
        "https://api.minimax.io/anthropic/v1",
        "https://platform.minimax.io/docs/api-reference/text-anthropic-api",
    ),
    (
        "minimax-cn",
        Protocol::Anthropic,
        "https://api.minimax.cn/anthropic/v1",
        "https://platform.minimaxi.com/document/Anthropic",
    ),
];

/// The official table's entry for a provider id, if any.
pub fn default_of(id: &str) -> Option<ProviderDefault> {
    OFFICIAL.iter().find(|(provider, ..)| *provider == id).map(
        |(_, protocol, api_url, citation)| ProviderDefault {
            protocol: *protocol,
            api_url,
            citation,
        },
    )
}

/// The official ids, for "unknown provider" errors.
pub fn official_ids() -> String {
    OFFICIAL
        .iter()
        .map(|(id, ..)| *id)
        .collect::<Vec<_>>()
        .join(", ")
}

/// The active provider, resolved: everything a request needs (config + catalog defaults + key).
#[derive(Debug)]
pub struct ActiveProvider {
    pub id: String,
    pub name: String,
    pub protocol: Protocol,
    /// Base URL; adapters append their canonical path (`/chat/completions`, `/responses`,
    /// `/v1/messages`).
    pub api_url: String,
    pub model: String,
    pub key: String,
    /// The reasoning opt-in (Anthropic `thinking`, Responses summaries): the provider block's
    /// override, else the catalogued model's flag, else off.
    pub reasoning: bool,
    /// Anthropic-required; clamped to the catalogued output limit.
    pub max_tokens: u64,
    /// The provider's model ids (block override, else the catalog's) — what an error can list.
    pub models: Vec<String>,
}

impl ActiveProvider {
    /// One line naming the provider's known models (appended to 4xx cards), when any are known.
    pub fn model_hint(&self) -> Option<String> {
        if self.models.is_empty() {
            return None;
        }
        Some(format!(
            "Models that provider \"{}\" lists: {}. To use an unlisted model, add it to \
             [ai.providers.{}] models.",
            self.id,
            self.models.join(", "),
            self.id
        ))
    }
}

/// Resolve the active provider from the loaded config (catalog + on-disk/env keys).
/// The Anthropic adapter's thinking dialect for one provider (MOE-0035): MiniMax's surface
/// whitelists only `disabled|adaptive` — and M3.1 rejects even `disabled`, so reasoning-off must
/// *omit* the key — while legacy Anthropic models want `enabled` + `budget_tokens`.
pub fn anthropic_thinking(
    provider: &ActiveProvider,
    reasoning: Reasoning,
) -> super::AnthropicThinking {
    use super::AnthropicThinking;
    use moe_platform::config::ReasoningSetting;
    if !reasoning.is_on() {
        return AnthropicThinking::Omit;
    }
    if provider.id.starts_with("minimax") {
        // MiniMax's surface has exactly one on-state; an effort cannot be expressed there.
        return AnthropicThinking::Adaptive;
    }
    let budget = match reasoning.effort() {
        Some(ReasoningSetting::Low) => 1024,
        Some(ReasoningSetting::Medium) => 2048,
        Some(ReasoningSetting::High) => 4096,
        // The provider's own default: half the output budget, clamped as before (MOE-0035).
        _ => (provider.max_tokens / 2).clamp(1024, 4096),
    };
    AnthropicThinking::EnabledBudget(budget)
}

pub fn resolve(config: &MoeConfig) -> Result<ActiveProvider, String> {
    resolve_with(config, &moe_platform::model_catalog::load(), |id, env| {
        keychain::ai_api_key_for(id, env)
    })
}

/// Resolve with injected catalog and key lookup (tests use the embedded snapshot and a fake key).
pub fn resolve_with(
    config: &MoeConfig,
    catalog: &Catalog,
    key: impl Fn(&str, &[String]) -> Option<String>,
) -> Result<ActiveProvider, String> {
    resolve_pair_with(config, catalog, key, None, None)
}

/// Resolve one provider/model pair, optionally overriding the configured active pair (MOE-0062: an
/// AI Command's own model). The overrides travel together: a lone provider without a model falls
/// back to the active pair rather than guessing which of its models was meant.
pub fn resolve_pair_with(
    config: &MoeConfig,
    catalog: &Catalog,
    key: impl Fn(&str, &[String]) -> Option<String>,
    provider_override: Option<&str>,
    model_override: Option<&str>,
) -> Result<ActiveProvider, String> {
    let ai = &config.ai;
    // A pair is both halves or neither; half an override is no override.
    let override_pair = provider_override
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .zip(
            model_override
                .map(str::trim)
                .filter(|model| !model.is_empty()),
        );
    let selected = override_pair.map(|(provider, _)| provider).or_else(|| {
        ai.provider
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())
    });
    // Legacy mode (ADR-0005): no provider selected — the implicit `default` provider on its
    // `base_url`; a `[ai.providers.default]` block may still override it like any other provider.
    let legacy = selected.is_none();
    let id = selected.unwrap_or(keychain::DEFAULT_PROVIDER_ID);
    let block = ai.providers.get(id);
    let catalog_provider = catalog.get(id);
    let table = default_of(id);
    let legacy_base_url = ai
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|url| !url.is_empty());

    if legacy
        && block.is_none()
        && catalog_provider.is_none()
        && table.is_none()
        && legacy_base_url.is_none()
    {
        return Err(
            "AI is not configured yet: set [ai] provider + model in Open Config File (⌘,), \
                    or keep using the legacy [ai] base_url"
                .to_string(),
        );
    }
    if !legacy && block.is_none() && catalog_provider.is_none() && table.is_none() {
        return Err(format!(
            "unknown provider \"{id}\": official ids are {} (a model catalog id from the embedded \
             list); custom providers need an [ai.providers.{id}] block",
            official_ids()
        ));
    }

    let protocol = block
        .and_then(|b| b.protocol)
        .or(table.as_ref().map(|t| t.protocol))
        .or(legacy.then_some(Protocol::OpenAiChat))
        .ok_or_else(|| {
            format!(
                "provider \"{id}\" does not say which protocol it speaks: add \
                 protocol = \"openai-chat\" | \"openai-responses\" | \"anthropic\" to \
                 [ai.providers.{id}]"
            )
        })?;
    let api_url = block
        .and_then(|b| b.api_url.as_deref())
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .or_else(|| {
            catalog_provider
                .and_then(|p| p.api.as_deref())
                .map(str::trim)
                .filter(|url| !url.is_empty())
        })
        .or(legacy_base_url)
        .or_else(|| table.as_ref().map(|t| t.api_url))
        .ok_or_else(|| {
            format!("provider \"{id}\" needs an API URL: add api_url to [ai.providers.{id}]")
        })?;

    let models: Vec<String> = block
        .and_then(|b| b.models.clone())
        .unwrap_or_else(|| model_ids(catalog_provider));
    let model = override_pair
        .map(|(_, model)| model.to_string())
        .or_else(|| {
            ai.model
                .as_deref()
                .map(str::trim)
                .filter(|model| !model.is_empty())
                .map(str::to_string)
        })
        .or_else(|| legacy.then(|| ai.model_or_default().to_string()))
        .ok_or_else(|| {
            format!(
                "[ai] model is required with provider \"{id}\"{}",
                model_suffix(&models)
            )
        })?;

    let model_info = catalog_provider.and_then(|p| p.models.get(&model));
    let reasoning = block
        .and_then(|b| b.reasoning)
        .or_else(|| model_info.map(|m| m.reasoning))
        .unwrap_or(false);
    let max_tokens = model_info
        .and_then(|m| m.limit.as_ref())
        .and_then(|limit| limit.output)
        .map(|output| output.clamp(1024, DEFAULT_MAX_TOKENS))
        .unwrap_or(DEFAULT_MAX_TOKENS);

    let env_names = catalog_provider.map(|p| p.env.clone()).unwrap_or_default();
    let key = key(id, &env_names).ok_or_else(|| key_missing_message(id, &env_names))?;

    let name = block
        .and_then(|b| b.name.clone())
        .filter(|name| !name.trim().is_empty())
        .or_else(|| {
            catalog_provider
                .map(|p| p.name.clone())
                .filter(|name| !name.is_empty())
        })
        .unwrap_or_else(|| id.to_string());

    Ok(ActiveProvider {
        id: id.to_string(),
        name,
        protocol,
        api_url: api_url.to_string(),
        model,
        key,
        reasoning,
        max_tokens,
        models,
    })
}

/// One AI Command's resolved run (MOE-0062): the provider it runs on — its own configured pair when
/// the command overrides one, else the active pair — plus the reasoning and temperature it asks for.
pub struct CommandRun {
    pub provider: ActiveProvider,
    pub reasoning: Reasoning,
    pub temperature: Option<f64>,
}

/// Resolve an AI Command's run from `[ai.commands."<id>"]` (MOE-0062). A command with no block runs
/// exactly like the chat does: the active pair, the provider's own reasoning default, and no
/// temperature key on the wire.
pub fn resolve_command_run(config: &MoeConfig, command_id: &str) -> Result<CommandRun, String> {
    resolve_command_run_with(
        config,
        &moe_platform::model_catalog::load(),
        keychain::ai_api_key_for,
        command_id,
    )
}

/// Resolve with injected catalog and key lookup (tests use the embedded snapshot and a fake key).
pub fn resolve_command_run_with(
    config: &MoeConfig,
    catalog: &Catalog,
    key: impl Fn(&str, &[String]) -> Option<String>,
    command_id: &str,
) -> Result<CommandRun, String> {
    let command = config.ai.commands.get(command_id);
    let provider = resolve_pair_with(
        config,
        catalog,
        key,
        command.and_then(|c| c.provider.as_deref()),
        command.and_then(|c| c.model.as_deref()),
    )?;
    Ok(CommandRun {
        reasoning: Reasoning::resolve(provider.reasoning, command.and_then(|c| c.reasoning)),
        temperature: command.and_then(|c| c.temperature),
        provider,
    })
}

/// The protocol a provider speaks, in the same order the resolver reads it (the block's override,
/// then the checked-in table). `None` = an unknown provider: the caller must not promise anything
/// about it (MOE-0062's reasoning-support check reads this).
pub fn protocol_of(config: &MoeConfig, provider_id: &str) -> Option<Protocol> {
    let id = provider_id.trim();
    config
        .ai
        .providers
        .get(id)
        .and_then(|block| block.protocol)
        .or_else(|| default_of(id).map(|table| table.protocol))
}

fn model_ids(provider: Option<&CatalogProvider>) -> Vec<String> {
    provider
        .map(|p| p.models.keys().cloned().collect())
        .unwrap_or_default()
}

fn model_suffix(models: &[String]) -> String {
    if models.is_empty() {
        String::new()
    } else {
        format!(" — available models: {}", models.join(", "))
    }
}

fn key_missing_message(id: &str, env_names: &[String]) -> String {
    let path = keychain::provider_key_path(id)
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| format!("keys/{id}"));
    let env = env_names
        .first()
        .map(|name| format!(", or export {name}"))
        .unwrap_or_default();
    format!(
        "no API key for provider \"{id}\": save one with `key <your-key>` (stored at {path}{env}; \
         MOE_AI_API_KEY also works)"
    )
}

/// A minimal provider for tests in this crate (e.g. a mock server's URL as the API base).
#[cfg(test)]
pub(crate) fn test_provider(protocol: Protocol, api_url: &str) -> ActiveProvider {
    ActiveProvider {
        id: "test".into(),
        name: "Test".into(),
        protocol,
        api_url: api_url.to_string(),
        model: "test-model".into(),
        key: "test-key".into(),
        reasoning: false,
        max_tokens: DEFAULT_MAX_TOKENS,
        models: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_client::AnthropicThinking;
    use moe_platform::config::ReasoningSetting;
    use moe_platform::model_catalog;

    fn key_ok(_id: &str, _env: &[String]) -> Option<String> {
        Some("sk-test".to_string())
    }

    fn resolve_toml(toml: &str) -> Result<ActiveProvider, String> {
        let config = MoeConfig::from_toml(toml).expect("config parses");
        resolve_with(&config, model_catalog::embedded(), key_ok)
    }

    fn run_toml(toml: &str, command: &str) -> Result<CommandRun, String> {
        let config = MoeConfig::from_toml(toml).expect("config parses");
        resolve_command_run_with(&config, model_catalog::embedded(), key_ok, command)
    }

    /// One AI Command's own pair overrides the active one (MOE-0062); with no block it resolves to
    /// exactly the active pair, the provider's reasoning default and no temperature key.
    #[test]
    fn command_runs_resolve_their_own_pair_or_the_active_one() {
        let toml = "[ai]\nprovider = \"deepseek\"\nmodel = \"deepseek-flash\"\n\
                    [ai.commands.\"aicmd.translate\"]\nprovider = \"openai\"\nmodel = \"gpt-5.2\"\n";
        let own = run_toml(toml, "aicmd.translate").expect("resolves");
        assert_eq!(own.provider.id, "openai");
        assert_eq!(own.provider.model, "gpt-5.2");
        // No override: the active pair, and the provider's own reasoning default
        let inherited = run_toml(toml, "aicmd.improve").expect("resolves");
        assert_eq!(inherited.provider.id, "deepseek");
        assert_eq!(inherited.provider.model, "deepseek-flash");
        assert_eq!(inherited.reasoning, Reasoning::Default);
        assert_eq!(inherited.temperature, None);

        // Half a pair is no pair: a lone provider falls back to the active pair rather than
        // guessing which of its models was meant.
        let lone = run_toml(
            "[ai]\nprovider = \"deepseek\"\nmodel = \"deepseek-flash\"\n\
             [ai.commands.\"aicmd.translate\"]\nprovider = \"openai\"\n",
            "aicmd.translate",
        )
        .expect("resolves");
        assert_eq!(lone.provider.id, "deepseek");
    }

    /// The reasoning setting (MOE-0062): off and an effort both beat the provider's default, and
    /// the temperature rides through as written (or stays absent).
    #[test]
    fn command_runs_resolve_reasoning_and_temperature() {
        let toml = "[ai]\nprovider = \"deepseek\"\nmodel = \"deepseek-flash\"\n\
                    [ai.commands.\"aicmd.translate\"]\nreasoning = \"off\"\ntemperature = 0.2\n";
        let off = run_toml(toml, "aicmd.translate").expect("resolves");
        assert_eq!(off.reasoning, Reasoning::Off);
        assert!(!off.reasoning.is_on());
        assert_eq!(off.temperature, Some(0.2));

        let high = run_toml(
            "[ai]\nprovider = \"deepseek\"\nmodel = \"deepseek-flash\"\n\
             [ai.commands.\"aicmd.translate\"]\nreasoning = \"high\"\n",
            "aicmd.translate",
        )
        .expect("resolves");
        assert_eq!(high.reasoning, Reasoning::Effort(ReasoningSetting::High));
        assert_eq!(high.reasoning.effort(), Some(ReasoningSetting::High));

        // A command that declares nothing keeps the provider's default: this one reasons in the
        // snapshot, so the run asks for the dialect's own default rather than an effort.
        let inherited = run_toml(
            "[ai]\nprovider = \"deepseek\"\nmodel = \"deepseek-flash\"\n",
            "aicmd.translate",
        )
        .expect("resolves");
        assert_eq!(inherited.reasoning, Reasoning::Default);
    }

    /// The Anthropic dialect the resolver picks (MOE-0035/0062): the default keeps the old
    /// half-of-`max_tokens` formula, an effort names its own budget, MiniMax has one on-state, and
    /// off omits the key entirely.
    #[test]
    fn anthropic_thinking_follows_the_effort() {
        let provider =
            resolve_toml("[ai]\nprovider = \"anthropic\"\nmodel = \"claude-sonnet-4-5\"\n")
                .expect("resolves");
        assert!(provider.reasoning, "Claude models reason in the snapshot");
        assert_eq!(
            anthropic_thinking(&provider, Reasoning::Default),
            AnthropicThinking::EnabledBudget(provider.max_tokens / 2)
        );
        for (effort, budget) in [
            (ReasoningSetting::Low, 1024),
            (ReasoningSetting::Medium, 2048),
            (ReasoningSetting::High, 4096),
        ] {
            assert_eq!(
                anthropic_thinking(&provider, Reasoning::Effort(effort)),
                AnthropicThinking::EnabledBudget(budget),
                "{effort:?}"
            );
        }
        assert_eq!(
            anthropic_thinking(&provider, Reasoning::Off),
            AnthropicThinking::Omit
        );

        let minimax = resolve_toml("[ai]\nprovider = \"minimax\"\nmodel = \"MiniMax-M2.5\"\n")
            .expect("resolves");
        // MiniMax's surface has one on-state; an effort cannot be expressed there
        assert_eq!(
            anthropic_thinking(&minimax, Reasoning::Effort(ReasoningSetting::High)),
            AnthropicThinking::Adaptive
        );
        assert_eq!(
            anthropic_thinking(&minimax, Reasoning::Off),
            AnthropicThinking::Omit
        );
    }

    /// Every table entry is a real catalog id or a known local provider (the table must not drift
    /// into typos); `ollama` is the documented local exception.
    #[test]
    fn table_ids_exist_in_the_catalog() {
        let catalog = model_catalog::embedded();
        for (id, ..) in OFFICIAL {
            if *id == "ollama" {
                continue;
            }
            assert!(
                catalog.get(id).is_some(),
                "provider table id {id} is not in the catalog snapshot"
            );
        }
    }

    /// The legacy path is byte-for-byte the old behavior: base_url + model, openai-chat, the
    /// legacy key file, no reasoning opt-in, no tools gating here.
    #[test]
    fn legacy_base_url_resolves_to_the_implicit_default_provider() {
        let provider =
            resolve_toml("[ai]\nbase_url = \"https://x/v1\"\nmodel = \"local-llama\"\n").unwrap();
        assert_eq!(provider.id, keychain::DEFAULT_PROVIDER_ID);
        assert_eq!(provider.protocol, Protocol::OpenAiChat);
        assert_eq!(provider.api_url, "https://x/v1");
        assert_eq!(provider.model, "local-llama");
        assert!(!provider.reasoning);

        // Endpoint only: the legacy model default stays
        let provider = resolve_toml("[ai]\nbase_url = \"https://x/v1\"\n").unwrap();
        assert_eq!(provider.model, "gpt-4o-mini");
    }

    /// An official provider with no block: catalog/table defaults, catalog model list, catalog
    /// reasoning flag, branded name.
    #[test]
    fn official_provider_resolves_from_catalog_defaults() {
        let provider = resolve_toml("[ai]\nprovider = \"deepseek\"\nmodel = \"deepseek-flash\"\n")
            .expect("resolves");
        assert_eq!(provider.id, "deepseek");
        assert_eq!(provider.name, "DeepSeek");
        assert_eq!(provider.protocol, Protocol::OpenAiChat);
        assert_eq!(provider.api_url, "https://api.deepseek.com");
        assert!(provider.models.contains(&"deepseek-flash".to_string()));
        assert!(provider.reasoning, "deepseek-flash reasons in the snapshot");
        assert_eq!(
            provider.max_tokens, DEFAULT_MAX_TOKENS,
            "clamped to the default"
        );

        // The table supplies Anthropic's base URL (the catalog has none) and its protocol
        let provider =
            resolve_toml("[ai]\nprovider = \"anthropic\"\nmodel = \"claude-sonnet-5-5\"\n")
                .expect("resolves");
        assert_eq!(provider.protocol, Protocol::Anthropic);
        assert_eq!(provider.api_url, "https://api.anthropic.com");
        assert!(provider.reasoning, "Claude models reason in the snapshot");
    }

    /// MiniMax (MOE-0031): both regions resolve from the snapshot + table with the Anthropic
    /// protocol, the catalogued base (version root included), and a working endpoint join — the
    /// catalogued base is never doubled into `…/v1/v1/messages`.
    #[test]
    fn minimax_providers_resolve_over_their_anthropic_surface() {
        for (id, api) in [
            ("minimax", "https://api.minimax.io/anthropic/v1"),
            ("minimax-cn", "https://api.minimax.cn/anthropic/v1"),
        ] {
            let provider = resolve_toml(&format!(
                "[ai]\nprovider = \"{id}\"\nmodel = \"MiniMax-M2.5\"\n"
            ))
            .expect("resolves");
            assert_eq!(provider.id, id);
            assert_eq!(provider.protocol, Protocol::Anthropic);
            assert_eq!(provider.api_url, api);
            assert!(provider.models.contains(&"MiniMax-M2.5".to_string()));
            assert!(provider.reasoning, "MiniMax models reason in the snapshot");
            assert_eq!(
                crate::ai_client::endpoint_url(provider.protocol, &provider.api_url),
                format!("{api}/messages")
            );
        }
    }

    /// Blocks override official defaults (protocol, api_url, models, reasoning).
    #[test]
    fn provider_blocks_override_defaults() {
        let provider = resolve_toml(
            "[ai]\nprovider = \"openai\"\nmodel = \"gpt-5.2\"\n\
             [ai.providers.openai]\nprotocol = \"openai-responses\"\napi_url = \"https://proxy/v1\"\n\
             models = [\"gpt-5.2\"]\nreasoning = false\n",
        )
        .unwrap();
        assert_eq!(provider.protocol, Protocol::OpenAiResponses);
        assert_eq!(provider.api_url, "https://proxy/v1");
        assert_eq!(provider.models, vec!["gpt-5.2".to_string()]);
        assert!(!provider.reasoning, "the block's override wins");
    }

    /// Custom providers state protocol + api_url + models; a missing piece names it.
    #[test]
    fn custom_provider_requires_protocol_and_api_url() {
        let provider = resolve_toml(
            "[ai]\nprovider = \"my-gateway\"\nmodel = \"claude-sonnet-5-5\"\n\
             [ai.providers.my-gateway]\nname = \"My Gateway\"\nprotocol = \"anthropic\"\n\
             api_url = \"https://gateway.example.com/anthropic\"\nmodels = [\"claude-sonnet-5-5\"]\n",
        )
        .unwrap();
        assert_eq!(provider.name, "My Gateway");
        assert_eq!(provider.protocol, Protocol::Anthropic);
        assert!(
            !provider.reasoning,
            "custom providers default the opt-in off"
        );
        assert_eq!(provider.max_tokens, DEFAULT_MAX_TOKENS);

        let err = resolve_toml(
            "[ai]\nprovider = \"my-gateway\"\nmodel = \"x\"\n\
             [ai.providers.my-gateway]\napi_url = \"https://gateway\"\nmodels = [\"x\"]\n",
        )
        .unwrap_err();
        assert!(err.contains("protocol"), "{err}");

        let err = resolve_toml(
            "[ai]\nprovider = \"my-gateway\"\nmodel = \"x\"\n\
             [ai.providers.my-gateway]\nprotocol = \"anthropic\"\nmodels = [\"x\"]\n",
        )
        .unwrap_err();
        assert!(err.contains("api_url"), "{err}");

        let err = resolve_toml("[ai]\nprovider = \"nope\"\nmodel = \"x\"\n").unwrap_err();
        assert!(err.contains("unknown provider"), "{err}");
        assert!(
            err.contains("deepseek"),
            "the error lists official ids: {err}"
        );
    }

    /// A missing model names the requirement and lists what the provider knows.
    #[test]
    fn missing_model_error_lists_the_providers_models() {
        let err = resolve_toml("[ai]\nprovider = \"deepseek\"\n").unwrap_err();
        assert!(err.contains("[ai] model is required"), "{err}");
        assert!(err.contains("deepseek-flash"), "{err}");
    }

    /// The key lookup is per provider: `ai_api_key_for(id, env)` (its own file → env → fallback).
    #[test]
    fn key_lookup_uses_the_provider_id_and_env_names() {
        let config =
            MoeConfig::from_toml("[ai]\nprovider = \"deepseek\"\nmodel = \"deepseek-flash\"\n")
                .unwrap();
        let seen = std::sync::Mutex::new(Vec::new());
        let provider = resolve_with(&config, model_catalog::embedded(), |id, env| {
            seen.lock().unwrap().push((id.to_string(), env.to_vec()));
            Some("sk-deepseek".to_string())
        })
        .unwrap();
        assert_eq!(provider.key, "sk-deepseek");
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].0, "deepseek");
        assert_eq!(seen[0].1, vec!["DEEPSEEK_API_KEY".to_string()]);

        // Missing key: the message names the provider's file path and its env var
        let err = resolve_with(&config, model_catalog::embedded(), |_, _| None).unwrap_err();
        assert!(
            err.contains("no API key for provider \"deepseek\""),
            "{err}"
        );
        assert!(err.contains("keys/deepseek"), "{err}");
        assert!(err.contains("DEEPSEEK_API_KEY"), "{err}");

        // Legacy configs resolve the implicit default provider (legacy key file, no env names)
        let legacy = MoeConfig::from_toml("[ai]\nbase_url = \"https://x/v1\"\n").unwrap();
        let seen = std::sync::Mutex::new(Vec::new());
        resolve_with(&legacy, model_catalog::embedded(), |id, env| {
            seen.lock().unwrap().push((id.to_string(), env.to_vec()));
            Some("sk-legacy".to_string())
        })
        .unwrap();
        let seen = seen.lock().unwrap();
        assert_eq!(seen[0].0, keychain::DEFAULT_PROVIDER_ID);
        assert!(seen[0].1.is_empty());
    }

    /// The 4xx hint names the models the provider lists.
    #[test]
    fn model_hint_lists_models_and_the_override_key() {
        let provider =
            resolve_toml("[ai]\nprovider = \"deepseek\"\nmodel = \"deepseek-flash\"\n").unwrap();
        let hint = provider.model_hint().expect("catalog lists models");
        assert!(hint.contains("deepseek-flash"), "{hint}");
        assert!(hint.contains("[ai.providers.deepseek] models"), "{hint}");
    }

    /// An unconfigured config errors with the setup guidance (the `configured()` pre-check keeps
    /// this out of the normal path).
    #[test]
    fn unconfigured_config_errors() {
        let config = MoeConfig::from_toml("").unwrap();
        let err = resolve_with(&config, model_catalog::embedded(), key_ok).unwrap_err();
        assert!(err.contains("not configured"), "{err}");
    }
}
