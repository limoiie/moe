//! `config.toml` 的解析与默认值（ADR-0008：呼出键可改）。

use std::fmt;
use std::path::PathBuf;

use crate::summon::Modifier;
use serde::Deserialize;

pub const DEFAULT_DOUBLE_TAP_MS: u64 = 400;
pub const MIN_DOUBLE_TAP_MS: u64 = 100;
pub const MAX_DOUBLE_TAP_MS: u64 = 1000;

/// 首次「打开配置文件」时落盘的默认模板（必须能解析为 [`MoeConfig::default()`]）。
pub const DEFAULT_TEMPLATE: &str = "\
# Moe 配置
[summon]
# double-cmd | double-option | double-ctrl | 组合键（如 cmd+shift+space）
key = \"double-cmd\"
double_tap_ms = 400

# [ai]  取消注释并填入任意 OpenAI 兼容端点（DeepSeek/OpenRouter/本地 llama.cpp …）
# base_url = \"https://api.deepseek.com/v1\"
# model = \"deepseek-chat\"
# API key 不写在这里：用命令「key <你的key>」存 keychain（或设 MOE_AI_API_KEY）
";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SummonKey {
    /// 双击修饰键（默认 double-cmd）。
    DoubleTap(Modifier),
    /// 组合键，如 `cmd+shift+space`。
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
        // 组合键：最后一个 token 是主键，前面必须都是修饰键
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

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AiConfig {
    pub base_url: Option<String>,
    pub model: Option<String>,
}

impl AiConfig {
    /// 端点未配置时 AI 命令不可用（引导项代替回答）。
    pub fn configured(&self) -> bool {
        self.base_url
            .as_deref()
            .is_some_and(|url| !url.trim().is_empty())
    }

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
}

#[derive(Deserialize, Default)]
struct RawConfig {
    summon: Option<RawSummon>,
    ai: Option<RawAi>,
}

#[derive(Deserialize, Default)]
struct RawAi {
    base_url: Option<String>,
    model: Option<String>,
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
        let ai = raw.ai.map_or_else(AiConfig::default, |raw_ai| AiConfig {
            base_url: raw_ai.base_url,
            model: raw_ai.model,
        });
        Ok(Self { summon, ai })
    }

    /// 从平台配置目录加载；文件缺失或解析失败时回落到默认值（ADR-0008）。
    pub fn load() -> Self {
        let Some(path) = config_path() else {
            return Self::default();
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => match Self::from_toml(&text) {
                Ok(cfg) => cfg,
                Err(err) => {
                    eprintln!("moe: {} 解析失败（{err}），使用默认呼出键", path.display());
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
            Self::Toml(msg) => write!(f, "invalid config.toml: {msg}"),
        }
    }
}

impl std::error::Error for ConfigError {}

/// 平台配置目录下的 `moe/config.toml`。
pub fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("moe").join("config.toml"))
}

/// 打开配置文件；不存在时先用默认模板创建。返回文件路径。
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

        // 只配端点：模型回落默认值
        let cfg = MoeConfig::from_toml("[ai]\nbase_url = \"https://x/v1\"\n").unwrap();
        assert!(cfg.ai.configured());
        assert_eq!(cfg.ai.model_or_default(), "gpt-4o-mini");
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
        // 无修饰键的组合会劫持全局字面输入
        assert!(SummonKey::parse("space").is_err());
        // 只有修饰键，没有主键
        assert!(SummonKey::parse("cmd+shift").is_err());
        assert!(SummonKey::parse("cmd+").is_err());

        assert!(MoeConfig::from_toml("[summon]\nkey = \"double-nope\"\n").is_err());
        assert!(MoeConfig::from_toml("[summon]\ndouble_tap_ms = 2000\n").is_err());
        assert!(MoeConfig::from_toml("[summon]\ndouble_tap_ms = 50\n").is_err());
    }
}
