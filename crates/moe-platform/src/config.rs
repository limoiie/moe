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
# google, xai, minimax, minimax-cn, mistral, groq, openrouter, …) and a model from its catalog list;
# the key file is keys/<provider>. Legacy base_url above keeps working (it is an implicit `default`
# provider).
# provider = \"deepseek\"
# model = \"deepseek-flash\"
#
# [ai.providers.openai]       # official providers: overrides only
# protocol = \"openai-responses\"   # openai-chat | openai-responses | anthropic
# models = [\"gpt-5.2\"]            # restrict the picker list (an unknown-model error lists these)
# hidden_models = [\"gpt-5.1\"]     # models the settings window hides from the pickers
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

#[derive(Debug, Clone, PartialEq)]
pub struct AiConfig {
    /// Active provider id (ADR-0040): an official (catalog) id or a `[ai.providers.*]` key.
    /// `None` = legacy mode (see `base_url`).
    pub provider: Option<String>,
    pub model: Option<String>,
    /// Legacy (ADR-0005): an OpenAI-compatible base URL; sugar for an implicit `default` provider.
    pub base_url: Option<String>,
    /// Provider overrides / custom providers by id (ADR-0040); `BTreeMap` for stable iteration.
    pub providers: BTreeMap<String, ProviderConfig>,
    /// Agentic Conversations advertise Tools; a kill-switch for new conversations (MOE-0026).
    pub tools: bool,
    /// The self-hosted SearXNG base URL; unset → `web_search` is not advertised at all.
    pub searxng_url: Option<String>,
    /// The `web_fetch` fallback when local extraction fails (MOE-0026).
    pub fetch_fallback: FetchFallback,
    /// Per-AI-Command run settings (MOE-0062), keyed by command id: the model the command runs on,
    /// its reasoning effort and its temperature. An id that is absent runs on the active pair with
    /// the provider's own defaults, so an empty table changes nothing.
    pub commands: BTreeMap<String, CommandAiConfig>,
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
    /// Models hidden from the pickers (the settings window's checkboxes, MOE-0029): subtracted
    /// from the effective list (`models` restriction first, then this). The active pair is never
    /// hidden from the surfaces that already use it — only from the pickers.
    pub hidden_models: Option<Vec<String>>,
    /// Override the reasoning opt-in (Anthropic `thinking` / Responses summaries); unset = the
    /// catalogued model's flag (official providers) or off (custom).
    pub reasoning: Option<bool>,
}

/// One `[ai.commands."<command-id>"]` block (MOE-0062): what one AI Command runs on. Every field
/// is optional — absent means "the active pair / the provider's own default" — so the settings
/// window can write one key at a time and removing it restores the default.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CommandAiConfig {
    /// The provider id this command runs on; `model` must name one of its models. Both or neither:
    /// a lone provider falls back to the active pair rather than guessing a model.
    pub provider: Option<String>,
    pub model: Option<String>,
    /// Reasoning effort: `off` / `low` / `medium` / `high`. Absent = the provider's own default
    /// (the catalog/block flag).
    pub reasoning: Option<ReasoningSetting>,
    /// Sampling temperature; absent = the protocol's default (the key is not sent at all).
    pub temperature: Option<f64>,
}

/// The per-command reasoning setting (MOE-0062): `off`, or an explicit effort the adapters map to
/// their protocol's own knob (Anthropic's thinking budget, the Responses API's `effort`).
/// The OpenAI Chat Completions dialect has no request-side knob, so a level is inert there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReasoningSetting {
    Off,
    Low,
    Medium,
    High,
}

impl ReasoningSetting {
    pub fn parse(raw: &str) -> Result<Self, ConfigError> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "off" => Ok(Self::Off),
            "low" => Ok(Self::Low),
            "medium" => Ok(Self::Medium),
            "high" => Ok(Self::High),
            _ => Err(ConfigError::UnknownReasoning(raw.to_string())),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }

    /// The effort as a 0..1 fraction of the maximum: what the protocol adapters scale their own
    /// knob by (Anthropic's budget, the Responses API's `effort` ladder).
    pub fn fraction(self) -> f64 {
        match self {
            Self::Off => 0.0,
            Self::Low => 0.25,
            Self::Medium => 0.5,
            Self::High => 1.0,
        }
    }
}

/// The wire protocols Moe speaks (ADR-0040): the OpenAI Chat Completions dialect (today's code;
/// MOE-0026's tool loop is gated to it), the OpenAI Responses API, and Anthropic Messages.
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
            commands: BTreeMap::new(),
        }
    }
}

/// The `web_fetch` fallback when the local GET/extraction fails (MOE-0026).
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

#[derive(Debug, Clone, PartialEq, Default)]
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
    #[serde(default)]
    commands: BTreeMap<String, RawCommandAi>,
}

/// One `[ai.commands."<command-id>"]` block (MOE-0062), as written in the file.
#[derive(Deserialize, Default)]
struct RawCommandAi {
    provider: Option<String>,
    model: Option<String>,
    reasoning: Option<String>,
    temperature: Option<f64>,
}

#[derive(Deserialize, Default)]
struct RawProvider {
    name: Option<String>,
    protocol: Option<String>,
    api_url: Option<String>,
    models: Option<Vec<String>>,
    hidden_models: Option<Vec<String>>,
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
                                hidden_models: raw.hidden_models,
                                reasoning: raw.reasoning,
                            },
                        ))
                    })
                    .collect::<Result<BTreeMap<_, _>, ConfigError>>()?;
                let commands = raw_ai
                    .commands
                    .into_iter()
                    .map(|(id, raw)| {
                        let reasoning = raw
                            .reasoning
                            .as_deref()
                            .map(ReasoningSetting::parse)
                            .transpose()?;
                        Ok((
                            id,
                            CommandAiConfig {
                                provider: raw.provider,
                                model: raw.model,
                                reasoning,
                                temperature: raw.temperature,
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
                    commands,
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
    UnknownReasoning(String),
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
            Self::UnknownReasoning(raw) => {
                write!(
                    f,
                    "unknown reasoning: {raw} (expected off | low | medium | high)"
                )
            }
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

/// One TOML basic-string value: values come from config input (provider ids, model ids), so quotes
/// and backslashes escape — the written file must always parse.
fn toml_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Whether a config line assigns `key` (a commented line never counts).
fn is_key_assignment(line: &str, key: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.starts_with('#') {
        return false;
    }
    trimmed
        .split_once('=')
        .is_some_and(|(found, _)| found.trim() == key)
}

/// Insert or replace one assignment inside `[section]`, keeping every other line byte for byte.
/// A missing section is appended after a blank line.
fn set_section_assignment(lines: Vec<String>, section: &str, key: &str, line: &str) -> Vec<String> {
    set_section_assignment_opt(lines, section, key, Some(line))
}

/// Like [`set_section_assignment`] but `None` removes the assignment (an empty hidden-models list
/// leaves no trace in the user's file). A removal never creates the section.
fn set_section_assignment_opt(
    mut lines: Vec<String>,
    section: &str,
    key: &str,
    line: Option<&str>,
) -> Vec<String> {
    let header = format!("[{section}]");
    match lines
        .iter()
        .position(|candidate| candidate.trim() == header)
    {
        Some(start) => {
            let end = lines[start + 1..]
                .iter()
                .position(|candidate| candidate.trim_start().starts_with('['))
                .map_or(lines.len(), |offset| start + 1 + offset);
            match (start + 1..end).find(|&i| is_key_assignment(&lines[i], key)) {
                Some(at) => match line {
                    Some(line) => lines[at] = line.to_string(),
                    None => {
                        lines.remove(at);
                        // A provider block that carried only this key leaves no trace: drop the
                        // header (and the blank line that separated it) when nothing but
                        // whitespace remains — user comments keep the block alive.
                        let body_end = lines[start + 1..]
                            .iter()
                            .position(|candidate| candidate.trim_start().starts_with('['))
                            .map_or(lines.len(), |offset| start + 1 + offset);
                        if lines[start + 1..body_end]
                            .iter()
                            .all(|candidate| candidate.trim().is_empty())
                        {
                            lines.remove(start);
                            if start > 0
                                && lines[start - 1].trim().is_empty()
                                && lines.get(start).is_none_or(|next| next.trim().is_empty())
                            {
                                lines.remove(start - 1);
                            }
                        }
                    }
                },
                None => {
                    if let Some(line) = line {
                        lines.insert(start + 1, line.to_string());
                    }
                }
            }
        }
        None => {
            let Some(line) = line else { return lines };
            if lines
                .last()
                .is_some_and(|candidate| !candidate.trim().is_empty())
            {
                lines.push(String::new());
            }
            lines.push(header);
            lines.push(line.to_string());
        }
    }
    lines
}

/// Edit several assignments of one section in place (each call recomputes the section, so the
/// replacements compose); reversed iteration makes fresh insertions land in the given order.
fn edit_section(text: &str, section: &str, assignments: &[(&str, String)]) -> String {
    let mut lines: Vec<String> = if text.is_empty() {
        Vec::new()
    } else {
        text.split('\n').map(str::to_string).collect()
    };
    // Drop the empty tail of a trailing newline so appends land before it; the join re-adds it.
    if lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    for (key, line) in assignments.iter().rev() {
        lines = set_section_assignment(lines, section, key, line);
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

/// Like [`edit_section`] but each assignment may be a removal (`None`).
fn edit_section_opt(text: &str, section: &str, assignments: &[(&str, Option<String>)]) -> String {
    let mut lines: Vec<String> = if text.is_empty() {
        Vec::new()
    } else {
        text.split('\n').map(str::to_string).collect()
    };
    if lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    for (key, line) in assignments.iter().rev() {
        lines = set_section_assignment_opt(lines, section, key, line.as_deref());
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

/// A TOML array of basic strings (`hidden_models = ["a", "b"]`).
fn toml_string_array(values: &[String]) -> String {
    let items: Vec<String> = values.iter().map(|value| toml_string(value)).collect();
    format!("[{}]", items.join(", "))
}

/// Remove one whole `[section]` (header + body lines) from the line list, keeping every other
/// line byte for byte (the settings window's provider removal, MOE-0032): the blank-line
/// separator the section brought goes with it, and a missing section is a no-op.
fn remove_section(mut lines: Vec<String>, section: &str) -> Vec<String> {
    let header = format!("[{section}]");
    // A hand-written header may carry a trailing comment (`[ai.providers.x]  # note`).
    let Some(start) = lines.iter().position(|candidate| {
        candidate
            .trim()
            .strip_prefix(&header)
            .is_some_and(|rest| rest.trim().is_empty() || rest.trim_start().starts_with('#'))
    }) else {
        return lines;
    };
    let end = lines[start + 1..]
        .iter()
        .position(|candidate| candidate.trim_start().starts_with('['))
        .map_or(lines.len(), |offset| start + 1 + offset);
    lines.drain(start..end);
    // Collapse the blank seam the removal left (the separator line before the header).
    let blank = |line: &String| line.trim().is_empty();
    if start > 0 && lines.get(start - 1).is_some_and(blank) && lines.get(start).is_none_or(blank) {
        lines.remove(start - 1);
    }
    lines
}

/// Replace (or add) the `theme` key inside the `[ui]` section of config.toml text, keeping every
/// other line — comments included — byte for byte (ADR-0035 amendment): the file is the user's, so
/// the app never re-serializes it. Pure, so the surgery is unit-tested.
pub fn set_theme_in_toml(text: &str, theme: Theme) -> String {
    edit_section(text, "ui", &[("theme", theme_line(theme))])
}

/// Replace (or add) the `provider` and `model` keys inside the `[ai]` section (the picker's
/// apply, MOE-0028): the same comment-preserving in-place surgery as the theme writer.
pub fn set_ai_selection_in_toml(text: &str, provider: &str, model: &str) -> String {
    edit_section(
        text,
        "ai",
        &[
            ("provider", format!("provider = {}", toml_string(provider))),
            ("model", format!("model = {}", toml_string(model))),
        ],
    )
}

/// Persist the appearance preference to `config.toml` (a missing file is created from the default
/// template first, so its comments teach the other settings too).
pub fn store_theme(theme: Theme) -> std::io::Result<()> {
    edit_config_file(|text| set_theme_in_toml(text, theme))
}

/// Persist the picked provider/model pair (the model picker's apply, MOE-0028) with the same rule:
/// the file's comments and every unrelated line survive byte for byte.
pub fn store_ai_selection(provider: &str, model: &str) -> std::io::Result<()> {
    edit_config_file(|text| set_ai_selection_in_toml(text, provider, model))
}

/// Replace (or add, or remove) the `searxng_url` key inside `[ai]` (the settings window's Web
/// Search row, MOE-0034): a blank value removes the line again — unset means the `web_search`
/// tool is not advertised at all. Pure, so the surgery is unit-tested.
pub fn set_searxng_url_in_toml(text: &str, url: Option<&str>) -> String {
    let line = url
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!("searxng_url = {}", toml_string(value)));
    edit_section_opt(text, "ai", &[("searxng_url", line)])
}

/// Persist the settings window's SearXNG instance URL (MOE-0034), comment-preserving like every
/// other writer: the file is the user's.
pub fn store_searxng_url(url: Option<&str>) -> std::io::Result<()> {
    edit_config_file(|text| set_searxng_url_in_toml(text, url))
}

/// Replace (or add, or remove) the `hidden_models` key inside `[ai.providers.<id>]` (the settings
/// window's model checkboxes, MOE-0029): an empty list removes the line again, and a provider
/// block that only carries hidden models is created for official providers too.
pub fn set_provider_hidden_models_in_toml(text: &str, provider: &str, hidden: &[String]) -> String {
    let line =
        (!hidden.is_empty()).then(|| format!("hidden_models = {}", toml_string_array(hidden)));
    edit_section_opt(
        text,
        &format!("ai.providers.{provider}"),
        &[("hidden_models", line)],
    )
}

/// Persist the settings window's picker visibility for one provider's models (MOE-0029).
pub fn store_provider_hidden_models(provider: &str, hidden: &[String]) -> std::io::Result<()> {
    edit_config_file(|text| set_provider_hidden_models_in_toml(text, provider, hidden))
}

/// Replace (or add, or remove) one `[ai.commands."<id>"]` block (MOE-0062). Every `None` removes
/// its line, and a block with nothing left goes away entirely — so "Default" in the settings window
/// really means the key is gone, not written as a value the resolver has to interpret.
/// Pure, so the surgery is unit-tested.
pub fn set_command_ai_in_toml(text: &str, command: &str, config: &CommandAiConfig) -> String {
    let section = format!("ai.commands.{}", toml_string(command));
    if config == &CommandAiConfig::default() {
        let mut lines: Vec<String> = if text.is_empty() {
            Vec::new()
        } else {
            text.split('\n').map(str::to_string).collect()
        };
        if lines.last().is_some_and(String::is_empty) {
            lines.pop();
        }
        let mut out = remove_section(lines, &section).join("\n");
        out.push('\n');
        return out;
    }
    let values: [(&str, Option<String>); 4] = [
        ("provider", config.provider.as_deref().map(toml_string)),
        ("model", config.model.as_deref().map(toml_string)),
        (
            "reasoning",
            config.reasoning.map(|effort| toml_string(effort.as_str())),
        ),
        ("temperature", config.temperature.map(toml_float)),
    ];
    let assignments: Vec<(&str, Option<String>)> = values
        .iter()
        .map(|(key, value)| (*key, value.as_ref().map(|value| format!("{key} = {value}"))))
        .collect();
    edit_section_opt(text, &section, &assignments)
}

/// Persist one AI Command's run settings (MOE-0062).
pub fn store_command_ai(command: &str, config: &CommandAiConfig) -> std::io::Result<()> {
    edit_config_file(|text| set_command_ai_in_toml(text, command, config))
}

/// A TOML float that always carries a decimal point (`1.0`, not `1`): the file reads as a float,
/// and the deserializer never has to coerce an integer into one.
fn toml_float(value: f64) -> String {
    let mut text = format!("{value}");
    if !text.contains('.') && !text.contains('e') {
        text.push_str(".0");
    }
    text
}

/// Remove one provider from the config text (MOE-0032): its whole `[ai.providers.<id>]` block
/// goes away, and — when the caller says the selection pointed at it — `[ai] provider` / `model`
/// are removed too, so the AI surfaces fall back to their setup state (or the legacy `base_url`).
/// Pure, so the surgery is unit-tested.
pub fn remove_provider_in_toml(text: &str, provider: &str, clear_selection: bool) -> String {
    let mut lines: Vec<String> = if text.is_empty() {
        Vec::new()
    } else {
        text.split('\n').map(str::to_string).collect()
    };
    if lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    lines = remove_section(lines, &format!("ai.providers.{provider}"));
    let mut out = lines.join("\n");
    out.push('\n');
    if clear_selection {
        out = edit_section_opt(&out, "ai", &[("provider", None), ("model", None)]);
    }
    out
}

/// Persist a provider removal (the settings window's Remove, MOE-0032).
pub fn store_provider_removal(provider: &str, clear_selection: bool) -> std::io::Result<()> {
    edit_config_file(|text| remove_provider_in_toml(text, provider, clear_selection))
}

/// Read config.toml (creating it from the default template when missing) and write back an edit.
fn edit_config_file(edit: impl FnOnce(&str) -> String) -> std::io::Result<()> {
    let path = config_path().ok_or_else(|| std::io::Error::other("no config dir"))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => DEFAULT_TEMPLATE.to_string(),
        Err(err) => return Err(err),
    };
    std::fs::write(&path, edit(&text))
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

    /// The per-command AI settings (MOE-0062): every key is optional, the reasoning value is
    /// validated, and an unknown value is a config error rather than a silent default.
    #[test]
    fn command_ai_settings_parse_per_command() {
        let cfg = MoeConfig::from_toml("[ai]\nprovider = \"openai\"\n").unwrap();
        assert!(cfg.ai.commands.is_empty());

        let cfg = MoeConfig::from_toml(
            "[ai]\nprovider = \"openai\"\n\
             [ai.commands.\"aicmd.translate\"]\nprovider = \"anthropic\"\nmodel = \"claude-sonnet-4-5\"\n\
             reasoning = \"high\"\ntemperature = 0.2\n\
             [ai.commands.\"aicmd.improve\"]\nreasoning = \"off\"\n",
        )
        .unwrap();
        let translate = cfg.ai.commands.get("aicmd.translate").unwrap();
        assert_eq!(translate.provider.as_deref(), Some("anthropic"));
        assert_eq!(translate.model.as_deref(), Some("claude-sonnet-4-5"));
        assert_eq!(translate.reasoning, Some(ReasoningSetting::High));
        assert_eq!(translate.temperature, Some(0.2));
        let improve = cfg.ai.commands.get("aicmd.improve").unwrap();
        assert_eq!(improve.reasoning, Some(ReasoningSetting::Off));
        assert_eq!(improve.provider, None);
        assert_eq!(improve.temperature, None);

        assert!(MoeConfig::from_toml("[ai.commands.x]\nreasoning = \"sometimes\"\n").is_err());
    }

    /// The settings window writes one command's block by surgery (MOE-0062): keys are added,
    /// replaced and removed in place, and a block with nothing left goes away entirely — so
    /// "Default" in the UI really means the key is gone.
    #[test]
    fn command_ai_surgery_adds_replaces_and_removes() {
        let base = "# mine\n[ai]\nprovider = \"openai\"\n";
        let written = set_command_ai_in_toml(
            base,
            "aicmd.translate",
            &CommandAiConfig {
                provider: Some("anthropic".into()),
                model: Some("claude-sonnet-4-5".into()),
                reasoning: Some(ReasoningSetting::Low),
                temperature: Some(1.0),
            },
        );
        assert!(
            written.contains("[ai.commands.\"aicmd.translate\"]"),
            "{written}"
        );
        assert!(written.contains("reasoning = \"low\""), "{written}");
        assert!(
            written.contains("temperature = 1.0"),
            "a float stays a float: {written}"
        );
        // The file round-trips through the parser
        let parsed = MoeConfig::from_toml(&written).unwrap();
        let stored = parsed.ai.commands.get("aicmd.translate").unwrap();
        assert_eq!(stored.model.as_deref(), Some("claude-sonnet-4-5"));
        assert_eq!(stored.temperature, Some(1.0));

        // Replacing one key leaves the others alone
        let replaced = set_command_ai_in_toml(
            &written,
            "aicmd.translate",
            &CommandAiConfig {
                temperature: None,
                ..stored.clone()
            },
        );
        assert!(!replaced.contains("temperature"), "{replaced}");
        assert!(replaced.contains("reasoning = \"low\""), "{replaced}");

        // Everything cleared: the whole block goes, and the rest of the file is byte-identical
        let cleared =
            set_command_ai_in_toml(&replaced, "aicmd.translate", &CommandAiConfig::default());
        assert_eq!(cleared, base);
        // A command with no block at all is a no-op
        assert_eq!(
            set_command_ai_in_toml(base, "aicmd.none", &CommandAiConfig::default()),
            base
        );
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

    /// The picker's apply (MOE-0028): `provider`/`model` are written in place, comments and every
    /// unrelated line survive, and a commented sample is not mistaken for an assignment.
    #[test]
    fn set_ai_selection_replaces_in_place_and_keeps_everything_else() {
        let text = "# mine\n[summon]\nkey = \"double-option\"\n\n[ai]\n# provider = \"openai\"\nbase_url = \"https://x/v1\"\nmodel = \"old\"\n";
        let out = set_ai_selection_in_toml(text, "deepseek", "deepseek-flash");
        assert!(out.contains("provider = \"deepseek\""));
        assert!(out.contains("model = \"deepseek-flash\""));
        assert!(!out.contains("model = \"old\""));
        assert!(out.contains("# mine"), "comments survive: {out}");
        assert!(out.contains("# provider = \"openai\""), "{out}");
        assert!(out.contains("base_url = \"https://x/v1\""), "{out}");
        let cfg = MoeConfig::from_toml(&out).unwrap();
        assert_eq!(cfg.ai.provider.as_deref(), Some("deepseek"));
        assert_eq!(cfg.ai.model.as_deref(), Some("deepseek-flash"));
        assert_eq!(cfg.summon.key, SummonKey::DoubleTap(Modifier::Alt));
    }

    /// Fresh insertions land under the `[ai]` header in the given order; a missing section is
    /// appended (the template already carries commented samples, so the write replaces them).
    #[test]
    fn set_ai_selection_inserts_both_keys_in_order() {
        let out = set_ai_selection_in_toml("[summon]\nkey = \"double-cmd\"\n", "lab", "m");
        assert!(
            out.ends_with("[ai]\nprovider = \"lab\"\nmodel = \"m\"\n"),
            "{out}"
        );

        let out = set_ai_selection_in_toml("[ai]\nbase_url = \"https://x\"\n", "lab", "m");
        let cfg = MoeConfig::from_toml(&out).unwrap();
        assert_eq!(cfg.ai.provider.as_deref(), Some("lab"));
        assert_eq!(cfg.ai.model.as_deref(), Some("m"));
        assert_eq!(cfg.ai.base_url.as_deref(), Some("https://x"));
        assert!(
            out.contains("[ai]\nprovider = \"lab\"\nmodel = \"m\"\nbase_url"),
            "{out}"
        );

        // The template's commented provider samples are replaced by real assignments
        let out = set_ai_selection_in_toml(DEFAULT_TEMPLATE, "ollama", "llama3.2");
        let cfg = MoeConfig::from_toml(&out).unwrap();
        assert_eq!(cfg.ai.provider.as_deref(), Some("ollama"));
        assert!(cfg.ai.configured());
    }

    /// Values escape so the written file always parses, even with quotes or backslashes (custom
    /// provider ids and model names are user input).
    #[test]
    fn set_ai_selection_escapes_values() {
        let out = set_ai_selection_in_toml("", "x\"y", "m\\n");
        let cfg = MoeConfig::from_toml(&out).unwrap();
        assert_eq!(cfg.ai.provider.as_deref(), Some("x\"y"));
        assert_eq!(cfg.ai.model.as_deref(), Some("m\\n"));
    }

    /// The settings window's picker visibility (MOE-0029): `hidden_models` lands in the provider's
    /// block, creates the block for official providers, and an empty list removes the line again.
    #[test]
    fn set_provider_hidden_models_edits_the_provider_block() {
        let hidden = vec!["gpt-5.1".to_string(), "gpt-5.1-mini".to_string()];

        // Existing block: the key joins it, everything else survives
        let text = "# mine\n[ai]\nprovider = \"openai\"\n\n[ai.providers.openai]\n# overrides\nmodels = [\"gpt-5.2\"]\n";
        let out = set_provider_hidden_models_in_toml(text, "openai", &hidden);
        assert!(out.contains("# mine"));
        assert!(out.contains("# overrides"));
        assert!(out.contains("models = [\"gpt-5.2\"]"));
        let cfg = MoeConfig::from_toml(&out).unwrap();
        assert_eq!(
            cfg.ai
                .providers
                .get("openai")
                .unwrap()
                .hidden_models
                .as_deref(),
            Some(&hidden[..])
        );

        // No block yet (official provider): one is created
        let out = set_provider_hidden_models_in_toml(
            "[ai]\nprovider = \"deepseek\"\n",
            "deepseek",
            &hidden,
        );
        let cfg = MoeConfig::from_toml(&out).unwrap();
        assert_eq!(
            cfg.ai
                .providers
                .get("deepseek")
                .unwrap()
                .hidden_models
                .as_deref(),
            Some(&hidden[..])
        );

        // Empty list removes the line (and leaves a block-less file block-less)
        let out = set_provider_hidden_models_in_toml(&out, "deepseek", &[]);
        assert!(!out.contains("hidden_models"));
        let cfg = MoeConfig::from_toml(&out).unwrap();
        assert!(cfg.ai.providers.is_empty());

        // Replacement in place, not duplicated
        let out = set_provider_hidden_models_in_toml(text, "openai", &hidden);
        let out = set_provider_hidden_models_in_toml(&out, "openai", &["gpt-5.2".to_string()]);
        assert_eq!(out.matches("hidden_models").count(), 1);
        let cfg = MoeConfig::from_toml(&out).unwrap();
        assert_eq!(
            cfg.ai
                .providers
                .get("openai")
                .unwrap()
                .hidden_models
                .as_deref(),
            Some(&["gpt-5.2".to_string()][..])
        );
    }

    /// The settings window's Web Search row (MOE-0034): replace / add / remove `[ai] searxng_url`.
    #[test]
    fn set_searxng_url_edits_the_ai_section() {
        // Existing section: the key joins it, comments and unrelated lines survive
        let text =
            "[summon]\nkey = \"double-cmd\"\n\n[ai]\n# my endpoint\nmodel = \"deepseek-flash\"\n";
        let out = set_searxng_url_in_toml(text, Some("http://localhost:8888"));
        assert!(out.contains("# my endpoint"));
        assert!(out.contains("model = \"deepseek-flash\""));
        let cfg = MoeConfig::from_toml(&out).unwrap();
        assert_eq!(cfg.ai.searxng_url.as_deref(), Some("http://localhost:8888"));

        // Replacement in place, not duplicated; surrounding whitespace is trimmed
        let out = set_searxng_url_in_toml(&out, Some("  http://127.0.0.1:8080  "));
        assert_eq!(out.matches("searxng_url").count(), 1);
        let cfg = MoeConfig::from_toml(&out).unwrap();
        assert_eq!(cfg.ai.searxng_url.as_deref(), Some("http://127.0.0.1:8080"));

        // Blank (or None) removes the line again — unset = no web_search tool
        let out = set_searxng_url_in_toml(&out, Some("   "));
        assert!(!out.contains("searxng_url"));
        assert!(out.contains("# my endpoint"));
        let cfg = MoeConfig::from_toml(&out).unwrap();
        assert_eq!(cfg.ai.searxng_url, None);

        // No [ai] section yet: one is created
        let out = set_searxng_url_in_toml("[ui]\ntheme = \"dark\"\n", Some("http://searx.local"));
        let cfg = MoeConfig::from_toml(&out).unwrap();
        assert_eq!(cfg.ai.searxng_url.as_deref(), Some("http://searx.local"));
    }

    /// The settings window's Remove (MOE-0032): the provider block leaves whole (its separator
    /// blank line included), the active selection is cleared only when asked, and everything
    /// else — comments, other sections — survives byte for byte. A missing provider is a no-op.
    #[test]
    fn remove_provider_edits_the_block_and_optionally_the_selection() {
        let text = "# mine\n[summon]\nkey = \"double-option\"\n\n[ai]\n# pick\nprovider = \"alibaba\"\nmodel = \"qwen\"\n\n[ai.providers.alibaba]  # note\nhidden_models = [\"qwen-old\"]\n\n[ui]\ntheme = \"dark\"\n";
        let out = remove_provider_in_toml(text, "alibaba", true);
        assert!(!out.contains("ai.providers.alibaba"));
        assert!(!out.contains("provider ="));
        assert!(!out.contains("model ="));
        assert!(out.contains("# mine"));
        assert!(out.contains("# pick"));
        assert!(out.contains("[ui]"));
        let cfg = MoeConfig::from_toml(&out).unwrap();
        assert_eq!(cfg.ai.provider, None);
        assert_eq!(cfg.summon.key, SummonKey::DoubleTap(Modifier::Alt));
        assert!(!out.contains("\n\n\n"), "no doubled blank seam: {out}");

        // Block only (the provider was not the active pair): the selection stays
        let out = remove_provider_in_toml(text, "alibaba", false);
        assert!(!out.contains("ai.providers.alibaba"));
        let cfg = MoeConfig::from_toml(&out).unwrap();
        assert_eq!(cfg.ai.provider.as_deref(), Some("alibaba"));

        // A provider that is not there (and no selection clearing asked) leaves the text alone
        let out = remove_provider_in_toml("[ai]\nprovider = \"deepseek\"\n", "alibaba", false);
        assert_eq!(out, "[ai]\nprovider = \"deepseek\"\n");
        // With clear_selection the caller says the pair pointed at the removed id: the lines go,
        // and the emptied [ai] section loses its header too (nothing is left to declare)
        let out = remove_provider_in_toml(
            "[ai]\nprovider = \"alibaba\"\nmodel = \"q\"\n",
            "alibaba",
            true,
        );
        assert_eq!(out, "\n");
        assert!(MoeConfig::from_toml(&out).unwrap().ai.provider.is_none());
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
