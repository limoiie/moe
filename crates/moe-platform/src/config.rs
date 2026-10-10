//! Parsing and defaults for `config.toml` (ADR-0008: the summon key is configurable).

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

use crate::summon::Modifier;
use serde::Deserialize;

pub const DEFAULT_DOUBLE_TAP_MS: u64 = 400;
pub const MIN_DOUBLE_TAP_MS: u64 = 100;
pub const MAX_DOUBLE_TAP_MS: u64 = 1000;

/// Default template persisted the first time "open config file" runs (must parse to [`MoeConfig::default()`]).
pub const DEFAULT_TEMPLATE: &str = "\
# Moe configuration
[summon]
# double-cmd | double-option | double-ctrl | a combo (e.g. cmd+shift+space)
key = \"double-cmd\"
double_tap_ms = 400

[ui]
# system | light | dark — system follows the OS appearance (ADR-0035)
theme = \"system\"

# [ai]  Uncomment and fill in any OpenAI-compatible endpoint (DeepSeek/OpenRouter/local llama.cpp …)
# base_url = \"https://api.deepseek.com/v1\"
# model = \"deepseek-chat\"
# Don't put the API key here: store it in the keychain via the command \"key <your-key>\" (or set MOE_AI_API_KEY)
# tools = true                           # conversations may search the web / fetch pages / read files (the tool loop)
# searxng_url = \"http://localhost:8080\"  # a SearXNG instance with JSON enabled; unset = no web_search tool
# fetch_fallback = \"jina\"                # jina | none — keyless r.jina.ai retry when a page fetch is blocked or empty

# Multi-provider (ADR-0040): pick an official provider by catalog id (deepseek, openai, anthropic,
# google, xai, mistral, groq, openrouter, …) and a model from its catalog list; the key file is
# keys/<provider>. Legacy base_url above keeps working (it is an implicit `default` provider).
# provider = \"deepseek\"
# model = \"deepseek-flash\"
#
# [ai.providers.openai]       # official providers: overrides only
# protocol = \"openai-responses\"   # openai-chat | openai-responses | anthropic
# models = [\"gpt-5.2\"]            # restrict the picker list (an unknown-model error lists these)
# reasoning = false                # opt in/out of thinking blocks / reasoning summaries
#
# [ai.providers.my-gateway]   # custom endpoints state protocol, api_url and models
# name = \"My Gateway\"
# protocol = \"anthropic\"
# api_url = \"https://gateway.example.com/anthropic\"
# models = [\"claude-sonnet-5-5\"]
";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SummonKey {
    /// Double-tap modifier (default double-cmd).
    DoubleTap(Modifier),
    /// A key combo, e.g. `cmd+shift+space`.
    Combo {
        modifiers: Vec<Modifier>,
        key: String,
    },
}

impl SummonKey {
    pub fn parse(raw: &str) -> Result<Self, ConfigError> {
        let s = raw.trim();
        if let Some(rest) = s.strip_prefix("double-") {
            let m = match rest.to_ascii_lowercase().as_str() {
                "cmd" | "command" | "meta" => Modifier::Meta,
                "opt" | "option" | "alt" => Modifier::Alt,
                "ctrl" | "control" => Modifier::Control,
                _ => return Err(ConfigError::UnknownSummonKey(raw.to_string())),
            };
            return Ok(SummonKey::DoubleTap(m));
        }
        // Combo: the last token is the base key; everything before it must be modifiers
        let parts: Vec<&str> = s.split('+').map(str::trim).collect();
        if parts.len() < 2 {
            return Err(ConfigError::ComboMissingModifier(raw.to_string()));
        }
        let (mods_raw, key) = parts.split_at(parts.len() - 1);
        let mut modifiers = Vec::new();
        for word in mods_raw {
            let m = parse_modifier(word)
                .ok_or_else(|| ConfigError::UnknownSummonKey(raw.to_string()))?;
            modifiers.push(m);
        }
        let key = key[0].to_ascii_lowercase();
        if key.is_empty() || parse_modifier(&key).is_some() {
            return Err(ConfigError::ComboMissingKey(raw.to_string()));
        }
        Ok(SummonKey::Combo { modifiers, key })
    }
}

fn parse_modifier(word: &str) -> Option<Modifier> {
    match word.to_ascii_lowercase().as_str() {
        "cmd" | "command" | "meta" => Some(Modifier::Meta),
        "opt" | "option" | "alt" => Some(Modifier::Alt),
        "ctrl" | "control" => Some(Modifier::Control),
        "shift" => Some(Modifier::Shift),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummonConfig {
    pub key: SummonKey,
    pub double_tap_ms: u64,
}

impl Default for SummonConfig {
    fn default() -> Self {
        Self {
            key: SummonKey::DoubleTap(Modifier::Meta),
            double_tap_ms: DEFAULT_DOUBLE_TAP_MS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiConfig {
    /// Active provider id (ADR-0040): an official (catalog) id or a `[ai.providers.*]` key.
    /// `None` = legacy mode (see `base_url`).
    pub provider: Option<String>,
    pub model: Option<String>,
    /// Legacy (ADR-0005): an OpenAI-compatible base URL; sugar for an implicit `default` provider.
    pub base_url: Option<String>,
    /// Provider overrides / custom providers by id (ADR-0040); `BTreeMap` for stable iteration.
    pub providers: BTreeMap<String, ProviderConfig>,
    /// Agentic Conversations advertise Tools; a kill-switch for new conversations (MOE-0017).
    pub tools: bool,
    /// The self-hosted SearXNG base URL; unset → `web_search` is not advertised at all.
    pub searxng_url: Option<String>,
    /// The `web_fetch` fallback when local extraction fails (MOE-0017).
    pub fetch_fallback: FetchFallback,
}

/// One `[ai.providers.<id>]` block: official providers use it for overrides; custom providers
/// state `protocol` + `api_url` + `models` (ADR-0040).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProviderConfig {
    /// Display name (custom providers; official ones default to the catalog name).
    pub name: Option<String>,
    /// Override the protocol (e.g. OpenAI defaults to `openai-chat`; opt in to `openai-responses`).
    pub protocol: Option<Protocol>,
    /// Override the catalog/table base URL (required for custom providers).
    pub api_url: Option<String>,
    /// The model ids: the picker list for custom providers, an optional restriction for official
    /// ones; also what an unknown-model error lists.
    pub models: Option<Vec<String>>,
    /// Override the reasoning opt-in (Anthropic `thinking` / Responses summaries); unset = the
    /// catalogued model's flag (official providers) or off (custom).
    pub reasoning: Option<bool>,
}

/// The wire protocols Moe speaks (ADR-0040): the OpenAI Chat Completions dialect (today's code;
/// MOE-0017's tool loop is gated to it), the OpenAI Responses API, and Anthropic Messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    OpenAiChat,
    OpenAiResponses,
    Anthropic,
}

impl Protocol {
    pub fn parse(raw: &str) -> Result<Self, ConfigError> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "openai-chat" => Ok(Self::OpenAiChat),
            "openai-responses" => Ok(Self::OpenAiResponses),
            "anthropic" => Ok(Self::Anthropic),
            _ => Err(ConfigError::UnknownProtocol(raw.to_string())),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::OpenAiChat => "openai-chat",
            Self::OpenAiResponses => "openai-responses",
            Self::Anthropic => "anthropic",
        }
    }
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            provider: None,
            model: None,
            base_url: None,
            providers: BTreeMap::new(),
            tools: true,
            searxng_url: None,
            fetch_fallback: FetchFallback::Jina,
        }
    }
}

/// The `web_fetch` fallback when the local GET/extraction fails (MOE-0017).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FetchFallback {
    /// Retry through the keyless r.jina.ai reader (default).
    #[default]
    Jina,
    /// Fail honestly to the model — no third-party reader.
    None,
}

impl FetchFallback {
    pub fn parse(raw: &str) -> Result<Self, ConfigError> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "jina" => Ok(Self::Jina),
            "none" => Ok(Self::None),
            _ => Err(ConfigError::UnknownFetchFallback(raw.to_string())),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Jina => "jina",
            Self::None => "none",
        }
    }
}

impl AiConfig {
    /// AI surfaces answer with a setup card when no provider is selected (ADR-0040) and no legacy
    /// `base_url` is set (ADR-0005).
    pub fn configured(&self) -> bool {
        self.provider
            .as_deref()
            .is_some_and(|id| !id.trim().is_empty())
            || self
                .base_url
                .as_deref()
                .is_some_and(|url| !url.trim().is_empty())
    }

    /// The legacy mode's model default (base_url without a model keeps working unchanged).
    pub fn model_or_default(&self) -> &str {
        self.model
            .as_deref()
            .filter(|m| !m.trim().is_empty())
            .unwrap_or("gpt-4o-mini")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MoeConfig {
    pub summon: SummonConfig,
    pub ai: AiConfig,
    pub ui: UiConfig,
}

/// Appearance preference for the two UI themes (ADR-0035): follow the OS, or pin light/dark.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

impl Theme {
    pub fn parse(raw: &str) -> Result<Self, ConfigError> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "system" | "auto" => Ok(Self::System),
            "light" => Ok(Self::Light),
            "dark" => Ok(Self::Dark),
            _ => Err(ConfigError::UnknownTheme(raw.to_string())),
        }
    }

    /// The value the UI reads (`ui_theme`); the UI resolves `system` against the OS appearance.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UiConfig {
    pub theme: Theme,
}

#[derive(Deserialize, Default)]
struct RawConfig {
    summon: Option<RawSummon>,
    ai: Option<RawAi>,
    ui: Option<RawUi>,
}

#[derive(Deserialize, Default)]
struct RawUi {
    theme: Option<String>,
}

#[derive(Deserialize, Default)]
struct RawAi {
    provider: Option<String>,
    base_url: Option<String>,
    model: Option<String>,
    #[serde(default)]
    providers: BTreeMap<String, RawProvider>,
    tools: Option<bool>,
    searxng_url: Option<String>,
    fetch_fallback: Option<String>,
}

#[derive(Deserialize, Default)]
struct RawProvider {
    name: Option<String>,
    protocol: Option<String>,
    api_url: Option<String>,
    models: Option<Vec<String>>,
    reasoning: Option<bool>,
}

#[derive(Deserialize, Default)]
struct RawSummon {
    key: Option<String>,
    double_tap_ms: Option<u64>,
}

impl MoeConfig {
    pub fn from_toml(text: &str) -> Result<Self, ConfigError> {
        let raw: RawConfig = toml::from_str(text).map_err(|e| ConfigError::Toml(e.to_string()))?;
        let mut summon = SummonConfig::default();
        if let Some(raw_summon) = raw.summon {
            if let Some(key) = raw_summon.key {
                summon.key = SummonKey::parse(&key)?;
            }
            if let Some(ms) = raw_summon.double_tap_ms {
                if !(MIN_DOUBLE_TAP_MS..=MAX_DOUBLE_TAP_MS).contains(&ms) {
                    return Err(ConfigError::DoubleTapRange(ms));
                }
                summon.double_tap_ms = ms;
            }
        }
        let ai = match raw.ai {
            Some(raw_ai) => {
                let providers = raw_ai
                    .providers
                    .into_iter()
                    .map(|(id, raw)| {
                        let protocol = raw.protocol.as_deref().map(Protocol::parse).transpose()?;
                        Ok((
                            id,
                            ProviderConfig {
                                name: raw.name,
                                protocol,
                                api_url: raw.api_url,
                                models: raw.models,
                                reasoning: raw.reasoning,
                            },
                        ))
                    })
                    .collect::<Result<BTreeMap<_, _>, ConfigError>>()?;
                AiConfig {
                    provider: raw_ai.provider,
                    model: raw_ai.model,
                    base_url: raw_ai.base_url,
                    providers,
                    tools: raw_ai.tools.unwrap_or(true),
                    searxng_url: raw_ai.searxng_url,
                    fetch_fallback: raw_ai
                        .fetch_fallback
                        .as_deref()
                        .map(FetchFallback::parse)
                        .transpose()?
                        .unwrap_or_default(),
                }
            }
            None => AiConfig::default(),
        };
        let ui = match raw.ui {
            Some(raw_ui) => UiConfig {
                theme: raw_ui
                    .theme
                    .as_deref()
                    .map(Theme::parse)
                    .transpose()?
                    .unwrap_or_default(),
            },
            None => UiConfig::default(),
        };
        Ok(Self { summon, ai, ui })
    }

    /// Load from the platform config directory; fall back to defaults when the file is missing or fails to parse (ADR-0008).
    pub fn load() -> Self {
        let Some(path) = config_path() else {
            return Self::default();
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => match Self::from_toml(&text) {
                Ok(cfg) => cfg,
                Err(err) => {
                    eprintln!(
                        "moe: {} parse failed ({err}), using the default summon key",
                        path.display()
                    );
                    Self::default()
                }
            },
            Err(_) => Self::default(),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ConfigError {
    UnknownSummonKey(String),
    ComboMissingModifier(String),
    ComboMissingKey(String),
    DoubleTapRange(u64),
    UnknownTheme(String),
    UnknownFetchFallback(String),
    UnknownProtocol(String),
    Toml(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownSummonKey(raw) => write!(f, "unknown summon key: {raw}"),
            Self::ComboMissingModifier(raw) => {
                write!(f, "combo needs at least one modifier: {raw}")
            }
            Self::ComboMissingKey(raw) => write!(f, "combo is missing a base key: {raw}"),
            Self::DoubleTapRange(ms) => write!(
                f,
                "double_tap_ms {ms} out of range ({MIN_DOUBLE_TAP_MS}..={MAX_DOUBLE_TAP_MS})"
            ),
            Self::UnknownTheme(raw) => {
                write!(f, "unknown theme: {raw} (expected system | light | dark)")
            }
            Self::UnknownFetchFallback(raw) => {
                write!(f, "unknown fetch_fallback: {raw} (expected jina | none)")
            }
            Self::UnknownProtocol(raw) => write!(
                f,
                "unknown protocol: {raw} (expected openai-chat | openai-responses | anthropic)"
            ),
            Self::Toml(msg) => write!(f, "invalid config.toml: {msg}"),
        }
    }
}

impl std::error::Error for ConfigError {}

/// `moe/config.toml` under the platform config directory.
pub fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("moe").join("config.toml"))
}

/// Open the config file; create it from the default template first if missing. Returns the file path.
pub fn open_in_editor() -> std::io::Result<PathBuf> {
    let path = config_path().ok_or_else(|| std::io::Error::other("no config dir"))?;
    if !path.exists() {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&path, DEFAULT_TEMPLATE)?;
    }
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open")
        .arg("-t")
        .arg(&path)
        .spawn()?;
    #[cfg(not(target_os = "macos"))]
    let _ = std::process::Command::new("xdg-open").arg(&path).spawn()?;
    Ok(path)
}

/// The canonical `theme` assignment line for the persisted config.
fn theme_line(theme: Theme) -> String {
    format!("theme = \"{}\"", theme.as_str())
}

/// Whether a config line assigns the `theme` key (a commented line never counts).
fn is_theme_assignment(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.starts_with('#') {
        return false;
    }
    trimmed
        .split_once('=')
        .is_some_and(|(key, _)| key.trim() == "theme")
}

/// Replace (or add) the `theme` key inside the `[ui]` section of config.toml text, keeping every
/// other line — comments included — byte for byte (ADR-0035 amendment): the file is the user's, so
/// the app never re-serializes it. Pure, so the surgery is unit-tested.
pub fn set_theme_in_toml(text: &str, theme: Theme) -> String {
    let key_line = theme_line(theme);
    let mut lines: Vec<String> = if text.is_empty() {
        Vec::new()
    } else {
        text.split('\n').map(str::to_string).collect()
    };
    // Drop the empty tail of a trailing newline so appends land before it; the join re-adds it.
    if lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    match lines.iter().position(|line| line.trim() == "[ui]") {
        Some(start) => {
            let end = lines[start + 1..]
                .iter()
                .position(|line| line.trim_start().starts_with('['))
                .map_or(lines.len(), |offset| start + 1 + offset);
            match (start + 1..end).find(|&i| is_theme_assignment(&lines[i])) {
                Some(at) => lines[at] = key_line,
                None => lines.insert(start + 1, key_line),
            }
        }
        None => {
            if lines.last().is_some_and(|line| !line.trim().is_empty()) {
                lines.push(String::new());
            }
            lines.push("[ui]".to_string());
            lines.push(key_line);
        }
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

/// Persist the appearance preference to `config.toml` (a missing file is created from the default
/// template first, so its comments teach the other settings too).
pub fn store_theme(theme: Theme) -> std::io::Result<()> {
    let path = config_path().ok_or_else(|| std::io::Error::other("no config dir"))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => DEFAULT_TEMPLATE.to_string(),
        Err(err) => return Err(err),
    };
    std::fs::write(&path, set_theme_in_toml(&text, theme))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::summon::Modifier;

    #[test]
    fn default_is_double_cmd_with_400ms() {
        let cfg = MoeConfig::from_toml("").unwrap();
        assert_eq!(cfg.summon.key, SummonKey::DoubleTap(Modifier::Meta));
        assert_eq!(cfg.summon.double_tap_ms, 400);
    }

    #[test]
    fn parses_double_modifier_keys_and_toml_overrides() {
        assert_eq!(
            SummonKey::parse("double-cmd").unwrap(),
            SummonKey::DoubleTap(Modifier::Meta)
        );
        assert_eq!(
            SummonKey::parse("double-option").unwrap(),
            SummonKey::DoubleTap(Modifier::Alt)
        );
        assert_eq!(
            SummonKey::parse("double-ctrl").unwrap(),
            SummonKey::DoubleTap(Modifier::Control)
        );

        let cfg = MoeConfig::from_toml("[summon]\nkey = \"double-option\"\ndouble_tap_ms = 300\n")
            .unwrap();
        assert_eq!(cfg.summon.key, SummonKey::DoubleTap(Modifier::Alt));
        assert_eq!(cfg.summon.double_tap_ms, 300);
    }

    #[test]
    fn parses_combos() {
        assert_eq!(
            SummonKey::parse("cmd+shift+space").unwrap(),
            SummonKey::Combo {
                modifiers: vec![Modifier::Meta, Modifier::Shift],
                key: "space".into()
            }
        );
    }

    #[test]
    fn ai_section_parses_with_defaults() {
        let cfg = MoeConfig::from_toml("").unwrap();
        assert!(!cfg.ai.configured());
        assert_eq!(cfg.ai.model_or_default(), "gpt-4o-mini");

        let cfg = MoeConfig::from_toml(
            "[ai]\nbase_url = \"https://api.deepseek.com/v1\"\nmodel = \"deepseek-chat\"\n",
        )
        .unwrap();
        assert!(cfg.ai.configured());
        assert_eq!(
            cfg.ai.base_url.as_deref(),
            Some("https://api.deepseek.com/v1")
        );
        assert_eq!(cfg.ai.model_or_default(), "deepseek-chat");

        // Endpoint only: the model falls back to the default
        let cfg = MoeConfig::from_toml("[ai]\nbase_url = \"https://x/v1\"\n").unwrap();
        assert!(cfg.ai.configured());
        assert_eq!(cfg.ai.model_or_default(), "gpt-4o-mini");

        // A provider counts as configured without a legacy base_url (ADR-0040)
        let cfg =
            MoeConfig::from_toml("[ai]\nprovider = \"deepseek\"\nmodel = \"deepseek-flash\"\n")
                .unwrap();
        assert!(cfg.ai.configured());
        assert_eq!(cfg.ai.provider.as_deref(), Some("deepseek"));
    }

    /// The multi-provider schema (ADR-0040): provider + model select the pair, `[ai.providers.*]`
    /// blocks override official defaults or define custom endpoints.
    #[test]
    fn provider_blocks_parse_and_validate_protocol() {
        let cfg = MoeConfig::from_toml(
            "[ai]\nprovider = \"my-gateway\"\nmodel = \"claude-sonnet-5-5\"\n\
             [ai.providers.my-gateway]\nname = \"My Gateway\"\nprotocol = \"anthropic\"\n\
             api_url = \"https://gateway.example.com/anthropic\"\nmodels = [\"claude-sonnet-5-5\"]\n\
             reasoning = true\n",
        )
        .unwrap();
        let provider = cfg.ai.providers.get("my-gateway").unwrap();
        assert_eq!(provider.name.as_deref(), Some("My Gateway"));
        assert_eq!(provider.protocol, Some(Protocol::Anthropic));
        assert_eq!(
            provider.api_url.as_deref(),
            Some("https://gateway.example.com/anthropic")
        );
        assert_eq!(
            provider.models.as_deref(),
            Some(&["claude-sonnet-5-5".to_string()][..])
        );
        assert_eq!(provider.reasoning, Some(true));

        // Unknown protocol strings fail at parse time, naming the vocabulary
        let err = MoeConfig::from_toml(
            "[ai.providers.x]\nprotocol = \"gemini\"\napi_url = \"https://x\"\n",
        )
        .unwrap_err();
        assert_eq!(err, ConfigError::UnknownProtocol("gemini".into()));
        assert!(
            err.to_string()
                .contains("openai-chat | openai-responses | anthropic")
        );

        // Protocol round-trips its config spelling
        for (raw, protocol) in [
            ("openai-chat", Protocol::OpenAiChat),
            ("openai-responses", Protocol::OpenAiResponses),
            ("anthropic", Protocol::Anthropic),
        ] {
            assert_eq!(Protocol::parse(raw).unwrap(), protocol);
            assert_eq!(protocol.as_str(), raw);
        }
        assert!(
            Protocol::parse(" OpenAI-Chat ").is_ok(),
            "trimmed, case-insensitive"
        );
    }

    /// An empty `[ai.providers.*]` block is legal (official providers are overrides-only).
    #[test]
    fn provider_block_shape_is_optional() {
        let cfg = MoeConfig::from_toml(
            "[ai]\nprovider = \"openai\"\n\n[ai.providers.openai]\nmodels = [\"gpt-5.2\"]\n",
        )
        .unwrap();
        let provider = cfg.ai.providers.get("openai").unwrap();
        assert_eq!(provider.protocol, None);
        assert_eq!(provider.reasoning, None);
        assert_eq!(
            provider.models.as_deref(),
            Some(&["gpt-5.2".to_string()][..])
        );
    }

    #[test]
    fn ai_tool_settings_parse_with_defaults() {
        let cfg = MoeConfig::from_toml("").unwrap();
        assert!(cfg.ai.tools, "tools default on for new conversations");
        assert_eq!(cfg.ai.searxng_url, None);
        assert_eq!(cfg.ai.fetch_fallback, FetchFallback::Jina);
        assert_eq!(FetchFallback::Jina.as_str(), "jina");
        assert_eq!(FetchFallback::None.as_str(), "none");

        let cfg = MoeConfig::from_toml(
            "[ai]\ntools = false\nsearxng_url = \"http://localhost:8888\"\nfetch_fallback = \"none\"\n",
        )
        .unwrap();
        assert!(!cfg.ai.tools);
        assert_eq!(cfg.ai.searxng_url.as_deref(), Some("http://localhost:8888"));
        assert_eq!(cfg.ai.fetch_fallback, FetchFallback::None);

        assert!(MoeConfig::from_toml("[ai]\nfetch_fallback = \"bogus\"\n").is_err());
    }

    #[test]
    fn ui_theme_parses_and_defaults_to_system() {
        assert_eq!(MoeConfig::from_toml("").unwrap().ui.theme, Theme::System);
        assert_eq!(Theme::parse(" Light ").unwrap(), Theme::Light);
        assert_eq!(Theme::parse("auto").unwrap(), Theme::System);
        assert_eq!(Theme::parse("dark").unwrap().as_str(), "dark");
        assert!(Theme::parse("neon").is_err());

        let cfg = MoeConfig::from_toml("[ui]\ntheme = \"dark\"\n").unwrap();
        assert_eq!(cfg.ui.theme, Theme::Dark);
        assert!(MoeConfig::from_toml("[ui]\ntheme = \"neon\"\n").is_err());
    }

    #[test]
    fn set_theme_in_toml_replaces_in_place_and_keeps_everything_else() {
        let text = "# mine\n[summon]\nkey = \"double-option\"\n\n[ui]\n# appearance\ntheme = \"dark\"\n\n[ai]\nmodel = \"deepseek-chat\"\n";
        let out = set_theme_in_toml(text, Theme::Light);
        assert!(out.contains("theme = \"light\""));
        assert!(!out.contains("theme = \"dark\""));
        assert!(out.contains("# appearance"));
        assert!(out.contains("# mine"));
        assert!(out.ends_with('\n'));
        // The rest of the file still lands
        let cfg = MoeConfig::from_toml(&out).unwrap();
        assert_eq!(cfg.ui.theme, Theme::Light);
        assert_eq!(cfg.summon.key, SummonKey::DoubleTap(Modifier::Alt));
        assert_eq!(cfg.ai.model.as_deref(), Some("deepseek-chat"));
    }

    #[test]
    fn set_theme_in_toml_inserts_the_key_or_the_section() {
        // The section exists without the key: the key goes right under the header
        let out = set_theme_in_toml("[ui]\n", Theme::Dark);
        assert_eq!(out, "[ui]\ntheme = \"dark\"\n");

        // No section at all: one is appended after a blank line
        let out = set_theme_in_toml("[summon]\nkey = \"double-cmd\"\n", Theme::Light);
        assert!(out.ends_with("[ui]\ntheme = \"light\"\n"));
        assert_eq!(MoeConfig::from_toml(&out).unwrap().ui.theme, Theme::Light);

        // A commented-out key is not the key
        let out = set_theme_in_toml("# theme = \"light\"\n", Theme::Dark);
        assert!(out.contains("# theme = \"light\""));
        assert!(out.contains("theme = \"dark\""));

        // The template (which already carries the key) is edited in place
        let out = set_theme_in_toml(DEFAULT_TEMPLATE, Theme::Dark);
        assert_eq!(MoeConfig::from_toml(&out).unwrap().ui.theme, Theme::Dark);
        assert!(out.contains("system | light | dark"));
    }

    #[test]
    fn default_template_parses_and_matches_defaults() {
        let cfg = MoeConfig::from_toml(DEFAULT_TEMPLATE).unwrap();
        assert_eq!(cfg, MoeConfig::default());
    }

    #[test]
    fn rejects_invalid_keys_and_out_of_range_gap() {
        assert!(SummonKey::parse("double-space").is_err());
        assert!(SummonKey::parse("double-shift").is_err());
        // A combo without modifiers would hijack global literal input
        assert!(SummonKey::parse("space").is_err());
        // Only modifiers, no base key
        assert!(SummonKey::parse("cmd+shift").is_err());
        assert!(SummonKey::parse("cmd+").is_err());

        assert!(MoeConfig::from_toml("[summon]\nkey = \"double-nope\"\n").is_err());
        assert!(MoeConfig::from_toml("[summon]\ndouble_tap_ms = 2000\n").is_err());
        assert!(MoeConfig::from_toml("[summon]\ndouble_tap_ms = 50\n").is_err());
    }
}
