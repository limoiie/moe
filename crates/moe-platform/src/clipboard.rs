//! System clipboard write (a platform capability, same boundary as [`crate::TextTarget`]).
//!
//! "Copy"-style actions go through here: they only write the clipboard and do not write back to the host app (unlike WriteBack).

use crate::PlatformError;

/// Write text to the system clipboard.
#[cfg(target_os = "macos")]
pub fn copy(text: &str) -> Result<(), PlatformError> {
    crate::mac_text::copy_text(text);
    Ok(())
}

/// Not implemented on non-macOS yet (add X11 PRIMARY/CLIPBOARD selections when M4 Linux verification happens).
#[cfg(not(target_os = "macos"))]
pub fn copy(_text: &str) -> Result<(), PlatformError> {
    Err(PlatformError::Unsupported("clipboard write"))
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    /// Real write, real read: write a unique text to the system clipboard, then read it back (briefly occupies the user's clipboard).
    /// CI runners may not have a clipboard service; skip instead of failing when the text can't be read back.
    #[test]
    fn copy_round_trips_through_system_pasteboard() {
        let probe = format!("moe-clipboard-test-{}", std::process::id());
        super::copy(&probe).expect("copy");
        match crate::mac_text::clipboard_read_text() {
            Some(text) => assert_eq!(text, probe),
            None => eprintln!("skipped: no system clipboard service in this environment"),
        }
    }
}
