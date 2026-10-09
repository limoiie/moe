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
pub mod open;
pub mod screen;
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

/// The pre-show outcome of [`TextTarget::capture_selection`].
pub enum Capture {
    /// The selection is known: its text, or `None` for "nothing selected" (cursor mode).
    Done(Option<String>),
    /// The clipboard fallback's synthesized copy is still in flight (AX-opaque app): the panel may
    /// be shown now; [`PendingCapture::finish`] completes the capture off the main thread.
    Pending(Box<dyn PendingCapture>),
}

/// The unfinished tail of a capture whose fallback copy may not have landed yet.
///
/// `finish` waits out the remaining observation budget, reads the selection back and restores the
/// clipboard snapshot; it blocks, so run it off the main thread (the panel is already visible).
/// Dropping it without finishing leaves a copy that did land on the clipboard (the snapshot is
/// not written back) — callers must finish.
pub trait PendingCapture: Send {
    /// Complete the capture: the selection text when the copy landed and carried one, else `None`.
    fn finish(self: Box<Self>) -> Option<String>;
}

/// The Command layer reads and writes host-app text only through this boundary, without knowing the concrete mechanism.
pub trait TextTarget: Send + Sync {
    /// Grab the current Selection (text) before the panel becomes key. The AX path answers
    /// synchronously; when the app doesn't expose its selection (ADR-0002), the clipboard
    /// fallback fires its synthesized ⌘C — which is only valid while the host app still has focus —
    /// and either completes within a short grace (a real selection lands in a poll or two) or
    /// returns [`Capture::Pending`]: show the panel, then finish off the main thread. `None` means
    /// there is no selection (falling back to the cursor).
    fn capture_selection(&self) -> Capture;

    /// Replace the Selection if there is one, otherwise insert at the cursor.
    fn write_text(&self, text: &str) -> Result<(), PlatformError>;
}

/// Cross-platform placeholder implementation: lets upper layers compile and wire up before the real implementation lands.
pub struct Unsupported;

impl TextTarget for Unsupported {
    fn capture_selection(&self) -> Capture {
        // No platform text access: there is never a selection (matches the old error being
        // swallowed at the call site; the guidance UI keys off permission state instead).
        Capture::Done(None)
    }
    fn write_text(&self, _text: &str) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported("stub"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_has_no_selection_and_rejects_writes() {
        let target = Unsupported;
        assert!(matches!(target.capture_selection(), Capture::Done(None)));
        assert!(matches!(
            target.write_text("x"),
            Err(PlatformError::Unsupported(_))
        ));
    }
}
