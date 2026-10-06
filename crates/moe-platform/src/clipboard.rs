//! 系统剪贴板写入（平台能力，与 [`crate::TextTarget`] 同一边界）。
//!
//! 「复制」类动作走这里：只写剪贴板，不回写宿主应用（与 WriteBack 的区别）。

use crate::PlatformError;

/// 把文本写入系统剪贴板。
#[cfg(target_os = "macos")]
pub fn copy(text: &str) -> Result<(), PlatformError> {
    crate::mac_text::copy_text(text);
    Ok(())
}

/// 非 macOS 暂未实现（M4 Linux 验证时补 X11 的 PRIMARY/CLIPBOARD 选择区）。
#[cfg(not(target_os = "macos"))]
pub fn copy(_text: &str) -> Result<(), PlatformError> {
    Err(PlatformError::Unsupported("clipboard write"))
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    /// 真写真读：往系统剪贴板写一段唯一文本再读回来（会短暂占用用户剪贴板）。
    #[test]
    fn copy_round_trips_through_system_pasteboard() {
        let probe = format!("moe-clipboard-test-{}", std::process::id());
        super::copy(&probe).expect("copy");
        assert_eq!(
            crate::mac_text::clipboard_read_text().as_deref(),
            Some(probe.as_str())
        );
    }
}
