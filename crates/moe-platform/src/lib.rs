//! Platform boundary (ADR-0002/0008): text interaction with the host app and summon-key listening.
//! Summon listening: macOS = CGEventTap (permission gate), Linux = X11 XRecord (no permission needed, see
//! [`x11`]; Wayland self-detects and reports Unsupported); the macOS AX implementation of TextTarget is M2.

pub mod clipboard;
pub mod config;
pub mod db;
pub mod files;
pub mod keychain;
#[cfg(target_os = "macos")]
pub mod mac;
#[cfg(target_os = "macos")]
pub mod mac_text;
pub mod store;
pub mod summon;
pub mod text_target;
pub mod x11;

pub use summon::{SummonEvent, SummonListener, SummonStatus, UnsupportedSummon};

use std::fmt;

#[derive(Debug)]
pub enum PlatformError {
    /// Needs Accessibility/Input Monitoring permission; callers should show inline guidance (ADR-0008).
    PermissionRequired,
    /// Unavailable on this platform/session (e.g. reading the selection on Wayland).
    Unsupported(&'static str),
}

impl fmt::Display for PlatformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PermissionRequired => write!(f, "accessibility permission required"),
            Self::Unsupported(why) => write!(f, "unsupported: {why}"),
        }
    }
}

/// The Command layer reads and writes host-app text only through this boundary, without knowing the concrete mechanism.
pub trait TextTarget: Send + Sync {
    /// Grab the current Selection (text). Returns Ok(None) when there is no selection, falling back to the cursor.
    fn read_selection(&self) -> Result<Option<String>, PlatformError>;

    /// Replace the Selection if there is one, otherwise insert at the cursor.
    fn write_text(&self, text: &str) -> Result<(), PlatformError>;
}

/// Cross-platform placeholder implementation: lets upper layers compile and wire up before the real implementation lands.
pub struct Unsupported;

impl TextTarget for Unsupported {
    fn read_selection(&self) -> Result<Option<String>, PlatformError> {
        Err(PlatformError::Unsupported("stub"))
    }
    fn write_text(&self, _text: &str) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported("stub"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_reports_unsupported() {
        let target = Unsupported;
        assert!(matches!(
            target.read_selection(),
            Err(PlatformError::Unsupported(_))
        ));
    }
}
