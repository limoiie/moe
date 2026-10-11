//! AI API key storage (ADR-0019 amending ADR-0005; per-provider since ADR-0040/MOE-0027):
//! 0600 local files, not the config file.
//!
//! Why not the system keychain: for unsigned apps, macOS treats every keychain access as an "unknown app"
//! and repeatedly prompts "Improve security / enter login keychain password" — the keychain ACL is bound to the app's code signature,
//! which we don't have (ADR-0011). Raycast doesn't prompt because it has an Apple Developer ID signature.
//! A 0600 file is how the gh CLI stores tokens: readable only by the current user, good enough for a local desktop app.
//!
//! Paths: `data_dir/moe/keys/<provider-id>` (0600); the implicit `default` provider of legacy
//! `[ai] base_url` configs keeps the legacy `data_dir/moe/api-key` file (zero-edit compatibility).
//! Resolution per provider: its file → the provider's `env[]` names (official providers; from the
//! catalog) → `MOE_AI_API_KEY` (any provider, last). The macOS one-time keychain migration applies
//! to the legacy file only.

use std::path::{Path, PathBuf};

/// The implicit provider of legacy `[ai] base_url` configs (ADR-0040).
pub const DEFAULT_PROVIDER_ID: &str = "default";

/// Environment fallback accepted for any provider.
const ENV_FALLBACK: &str = "MOE_AI_API_KEY";

/// `moe/` under the platform data directory.
fn data_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join("moe"))
}

/// Provider ids become file names: keep the safe ASCII set, fold everything else to `_`
/// (config keys are user-written; a key must never escape `keys/`).
fn key_file_name(provider_id: &str) -> String {
    provider_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// The key file for a provider: `keys/<id>`, except the implicit `default` provider, whose file is
/// the legacy `api-key` path.
pub fn provider_key_path(provider_id: &str) -> Option<PathBuf> {
    let base = data_dir()?;
    if provider_id == DEFAULT_PROVIDER_ID {
        Some(base.join("api-key"))
    } else {
        Some(base.join("keys").join(key_file_name(provider_id)))
    }
}

fn read_key_file_at(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty())
}

/// Write a 0600 key file (Unix; Windows ignores the mode, same as a regular file).
fn write_key_file_at(path: &Path, key: &str) -> Result<(), String> {
    use std::io::Write;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|err| err.to_string())?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|err| err.to_string())?;
    file.write_all(key.trim().as_bytes())
        .map_err(|err| err.to_string())
}

/// The resolution order (ADR-0040), pure for tests: the provider's file → its `env[]` names →
/// `MOE_AI_API_KEY` (last resort, any provider).
fn resolve_key(
    path: Option<&Path>,
    env_names: &[String],
    env: impl Fn(&str) -> Option<String>,
) -> Option<String> {
    if let Some(path) = path
        && let Some(key) = read_key_file_at(path)
    {
        return Some(key);
    }
    for name in env_names {
        if let Some(key) = env(name).filter(|key| !key.trim().is_empty()) {
            return Some(key);
        }
    }
    env(ENV_FALLBACK).filter(|key| !key.trim().is_empty())
}

/// One provider's key: its file → its env names → `MOE_AI_API_KEY`.
pub fn ai_api_key_for(provider_id: &str, env_names: &[String]) -> Option<String> {
    let path = provider_key_path(provider_id);
    // One-time migration: older versions stored the (single) key in the system keychain. Persist it
    // to the legacy file when found, so the unsigned app no longer triggers keychain permission prompts.
    #[cfg(target_os = "macos")]
    if provider_id == DEFAULT_PROVIDER_ID && path.as_deref().is_none_or(|p| !p.exists()) {
        let migrated = keyring::Entry::new(SERVICE, ACCOUNT)
            .ok()
            .and_then(|entry| {
                let secret = entry.get_password().ok()?;
                if secret.trim().is_empty() {
                    return None;
                }
                let path = path.as_deref()?;
                match write_key_file_at(path, &secret) {
                    Ok(()) => read_key_file_at(path),
                    Err(_) => None,
                }
            });
        if migrated.is_some() {
            return migrated;
        }
    }
    resolve_key(path.as_deref(), env_names, |name| std::env::var(name).ok())
}

fn catalog_env_names(provider_id: &str) -> Vec<String> {
    crate::model_catalog::load()
        .get(provider_id)
        .map(|provider| provider.env.clone())
        .unwrap_or_default()
}

/// The active provider's id: `[ai] provider`, or the implicit `default` of legacy configs.
pub fn active_provider_id() -> String {
    crate::config::MoeConfig::load()
        .ai
        .provider
        .filter(|id| !id.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_PROVIDER_ID.to_string())
}

/// The key of the provider the config selects (the surfaces resolving a provider use
/// [`ai_api_key_for`] directly — this is the no-config-context entry, e.g. `key <value>`).
pub fn ai_api_key() -> Option<String> {
    let id = active_provider_id();
    let env_names = if id == DEFAULT_PROVIDER_ID {
        Vec::new()
    } else {
        catalog_env_names(&id)
    };
    ai_api_key_for(&id, &env_names)
}

/// Save a key for the active provider (`key <value>` / Moe: Save AI Key).
pub fn set_ai_api_key(key: &str) -> Result<(), String> {
    set_provider_key(&active_provider_id(), key)
}

/// Save a key for one named provider (the settings window's key rows, MOE-0029); the file is the
/// provider's own (`keys/<id>`, the legacy `api-key` file for `default`).
pub fn set_provider_key(provider_id: &str, key: &str) -> Result<(), String> {
    let path = provider_key_path(provider_id).ok_or_else(|| "no data dir".to_string())?;
    write_key_file_at(&path, key)
}

/// Delete one provider's key file (the settings window's Remove, MOE-0032): a provider without
/// a key file — or without a data dir — is success; removing twice must not fail.
pub fn remove_provider_key(provider_id: &str) -> Result<(), String> {
    let Some(path) = provider_key_path(provider_id) else {
        return Ok(());
    };
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.to_string()),
    }
}

#[cfg(target_os = "macos")]
const SERVICE: &str = "moe";
#[cfg(target_os = "macos")]
const ACCOUNT: &str = "ai.api-key";

#[cfg(test)]
mod tests {
    use super::*;

    /// Key file round-trip + 0600 permissions (Unix).
    #[test]
    fn key_file_round_trips_with_private_permissions() {
        let path = std::env::temp_dir().join(format!("moe-key-{}.tmp", std::process::id()));
        write_key_file_at(&path, "  sk-1234  ").expect("write");
        assert_eq!(read_key_file_at(&path).as_deref(), Some("sk-1234"));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).expect("meta").permissions().mode();
            assert_eq!(
                mode & 0o777,
                0o600,
                "key file must be readable/writable only by the current user"
            );
        }
        let _ = std::fs::remove_file(&path);
    }

    /// Per-provider paths (ADR-0040): `keys/<id>`, the legacy file for the implicit `default`.
    #[test]
    fn provider_key_paths_are_per_provider_and_safe() {
        let base = data_dir().expect("data dir");
        assert_eq!(
            provider_key_path(DEFAULT_PROVIDER_ID),
            Some(base.join("api-key"))
        );
        assert_eq!(
            provider_key_path("deepseek"),
            Some(base.join("keys").join("deepseek"))
        );
        // A config id can never escape the keys directory
        assert_eq!(key_file_name("../../etc/passwd"), ".._.._etc_passwd");
        let escaped = provider_key_path("../../etc/passwd").unwrap();
        assert!(escaped.starts_with(base.join("keys")));
    }

    /// Resolution order: the provider's file wins over env names, `MOE_AI_API_KEY` is last.
    #[test]
    fn key_resolution_order_is_file_then_env_names_then_fallback() {
        let path = std::env::temp_dir().join(format!("moe-key-order-{}.tmp", std::process::id()));
        let env = |name: &str| match name {
            "DEEPSEEK_API_KEY" => Some("sk-env".to_string()),
            "MOE_AI_API_KEY" => Some("sk-fallback".to_string()),
            _ => None,
        };
        let names = vec!["DEEPSEEK_API_KEY".to_string()];

        // No file: the provider's env name wins over the generic fallback
        assert_eq!(
            resolve_key(Some(&path), &names, env).as_deref(),
            Some("sk-env")
        );
        // Unknown provider (no env names): the fallback still works
        assert_eq!(
            resolve_key(Some(&path), &[], env).as_deref(),
            Some("sk-fallback")
        );
        // The provider's file wins over everything
        write_key_file_at(&path, "sk-file").expect("write");
        assert_eq!(
            resolve_key(Some(&path), &names, env).as_deref(),
            Some("sk-file")
        );
        let _ = std::fs::remove_file(&path);
    }
}
