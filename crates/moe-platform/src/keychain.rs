//! AI API key 存储（ADR-0005 的 ADR-0019 增补）：0600 本地文件，不进配置文件。
//!
//! 为什么不用系统钥匙串：未签名应用访问钥匙串时，macOS 会把每次访问都当成「未知应用」，
//! 反复弹「改进安全性 / 输入 login 钥匙串密码」——因为钥匙串的 ACL 绑定应用的代码签名，
//! 而我们没有签名（ADR-0011）。Raycast 不弹是因为它有 Apple Developer ID 签名。
//! 0600 文件是同 gh CLI 存 token 的做法：只有当前用户能读，本机桌面应用够用。
//!
//! 读取顺序：文件 →（macOS 一次性迁移）旧钥匙串 → 环境变量 `MOE_AI_API_KEY`。

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

/// 写 0600 密钥文件（Unix；Windows 忽略 mode，等同普通文件）。
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
    // 一次性迁移：旧版本把 key 存在系统钥匙串里。读到就落盘并清掉，
    // 让未签名应用以后不再触发钥匙串的授权弹窗。
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

    /// 密钥文件往返 + 0600 权限（Unix）。
    #[test]
    fn key_file_round_trips_with_private_permissions() {
        let path = std::env::temp_dir().join(format!("moe-key-{}.tmp", std::process::id()));
        write_key_file_at(&path, "  sk-1234  ").expect("write");
        assert_eq!(read_key_file_at(&path).as_deref(), Some("sk-1234"));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).expect("meta").permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "密钥文件只允许当前用户读写");
        }
        let _ = std::fs::remove_file(&path);
    }
}
