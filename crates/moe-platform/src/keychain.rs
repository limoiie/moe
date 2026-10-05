//! 系统凭据库：AI API key（ADR-0005，绝不落配置文件）。
//!
//! 读取顺序：keychain → 环境变量 `MOE_AI_API_KEY`（便于开发）。

// 仅 macOS 使用（Linux 走环境变量/后续 secret-service），避免非 mac 平台 dead_code。
#[cfg(target_os = "macos")]
const SERVICE: &str = "moe";
#[cfg(target_os = "macos")]
const ACCOUNT: &str = "ai.api-key";

pub fn ai_api_key() -> Option<String> {
    #[cfg(target_os = "macos")]
    if let Ok(entry) = keyring::Entry::new(SERVICE, ACCOUNT)
        && let Ok(secret) = entry.get_password()
        && !secret.trim().is_empty()
    {
        return Some(secret);
    }
    std::env::var("MOE_AI_API_KEY")
        .ok()
        .filter(|key| !key.trim().is_empty())
}

pub fn set_ai_api_key(key: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let entry = keyring::Entry::new(SERVICE, ACCOUNT).map_err(|err| err.to_string())?;
        entry
            .set_password(key.trim())
            .map_err(|err| err.to_string())?;
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = key;
        Err("keychain 目前仅支持 macOS；可设 MOE_AI_API_KEY 环境变量".into())
    }
}
