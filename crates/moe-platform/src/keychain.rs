//! AI API key storage (ADR-0019 amending ADR-0005): a 0600 local file, not the config file.
//!
//! Why not the system keychain: for unsigned apps, macOS treats every keychain access as an "unknown app"
//! and repeatedly prompts "Improve security / enter login keychain password" — the keychain ACL is bound to the app's code signature,
//! which we don't have (ADR-0011). Raycast doesn't prompt because it has an Apple Developer ID signature.
//! A 0600 file is how the gh CLI stores tokens: readable only by the current user, good enough for a local desktop app.
//!
//! Read order: file → (macOS one-time migration) legacy keychain → environment variable `MOE_AI_API_KEY`.

use std::path::{Path, PathBuf};

#[cfg(target_os = "macos")]
const SERVICE: &str = "moe";
#[cfg(target_os = "macos")]
const ACCOUNT: &str = "ai.api-key";

fn key_path() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join("moe").join("api-key"))
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

pub fn ai_api_key() -> Option<String> {
    if let Some(path) = key_path()
        && let Some(key) = read_key_file_at(&path)
    {
        return Some(key);
    }
    // One-time migration: older versions stored the key in the system keychain. Persist it to disk when found,
    // so the unsigned app no longer triggers keychain permission prompts.
    #[cfg(target_os = "macos")]
    {
        let migrated = keyring::Entry::new(SERVICE, ACCOUNT)
            .ok()
            .and_then(|entry| {
                let secret = entry.get_password().ok()?;
                if secret.trim().is_empty() {
                    return None;
                }
                match (key_path(), write_key_file_at(key_path().as_ref()?, &secret)) {
                    (Some(path), Ok(())) => read_key_file_at(&path),
                    _ => None,
                }
            });
        if migrated.is_some() {
            return migrated;
        }
    }
    std::env::var("MOE_AI_API_KEY")
        .ok()
        .filter(|key| !key.trim().is_empty())
}

pub fn set_ai_api_key(key: &str) -> Result<(), String> {
    let path = key_path().ok_or_else(|| "no data dir".to_string())?;
    write_key_file_at(&path, key)
}

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
}
