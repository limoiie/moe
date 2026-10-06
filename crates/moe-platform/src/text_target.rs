//! TextTarget's platform-agnostic strategy layer (ADR-0002): AX first, falling back to clipboard paste only on failure.
//!
//! Platform glue implements [`Ax`] and [`Clipboard`] separately; the strategy here (when to touch the clipboard,
//! the order of calls) is testable pure logic.

use crate::{PlatformError, TextTarget};

/// Boundary for reading/writing the focused element's selection via system Accessibility.
pub trait Ax: Send + Sync {
    /// The focused element's selected text; `Ok(None)` when there is no selection (cursor mode).
    fn selected_text(&self) -> Result<Option<String>, PlatformError>;

    /// Current selection/cursor range `(location, length)` (UTF-16 code units);
    /// `None` when the app doesn't support it (skip "select after write back" in that case).
    fn selected_text_range(&self) -> Option<(i64, i64)> {
        None
    }

    /// Set the selection to the given range (selects the written content after write back, for confirmation/rewrite); best-effort.
    fn set_selected_range(&self, _location: i64, _length: i64) {}

    /// Replace if there is a selection; insert if there is none (cursor mode).
    fn set_selected_text(&self, text: &str) -> Result<(), PlatformError>;
}

/// Clipboard boundary: snapshot / write / read back / synthesized paste / synthesized copy / restore.
pub trait Clipboard: Send + Sync {
    /// An implementation-defined opaque snapshot (items × types × data on Mac);
    /// its lifetime spans only a single synchronous write_text call, so Send is not required.
    type Snapshot;

    fn snapshot(&self) -> Self::Snapshot;
    fn set_text(&self, text: &str);
    /// Read the current clipboard text (None when there is none).
    fn read_text(&self) -> Option<String>;
    /// Synthesize ⌘V / Ctrl+V.
    fn paste(&self);
    /// Synthesize ⌘C / Ctrl+C (fallback path for reading the selection: the target app puts the selection on the clipboard).
    fn copy(&self);
    /// Write the snapshot back after the target app has consumed the synthesized events (including necessary delays).
    fn restore(&self, snapshot: Self::Snapshot);
}

pub struct HybridTextTarget<A: Ax, C: Clipboard> {
    ax: A,
    clipboard: C,
}

impl<A: Ax, C: Clipboard> HybridTextTarget<A, C> {
    pub fn new(ax: A, clipboard: C) -> Self {
        Self { ax, clipboard }
    }
}

impl<A: Ax, C: Clipboard> TextTarget for HybridTextTarget<A, C> {
    fn read_selection(&self) -> Result<Option<String>, PlatformError> {
        if let Ok(Some(text)) = self.ax.selected_text() {
            return Ok(Some(text));
        }
        // AX can't read it (e.g. Zed doesn't expose AXSelectedText): fall back to clipboard copy.
        // The timing is before the panel is summoned, while the host app still has focus — ⌘C copies its selection.
        // Compare the clipboard before and after ⌘C: no change = the app has no selection (avoid treating the old clipboard as the selection).
        let snapshot = self.clipboard.snapshot();
        let before = self.clipboard.read_text();
        self.clipboard.copy();
        let after = self.clipboard.read_text();
        self.clipboard.restore(snapshot);
        let selection = after
            .filter(|text| !text.trim().is_empty() && Some(text.as_str()) != before.as_deref());
        Ok(selection)
    }

    fn write_text(&self, text: &str) -> Result<(), PlatformError> {
        // Note the write position first (selection start/cursor), then select the new content after writing
        let insert_at = self.ax.selected_text_range();
        match self.ax.set_selected_text(text) {
            Ok(()) => {
                if let Some((location, _)) = insert_at {
                    self.ax.set_selected_range(location, utf16_len(text));
                }
                Ok(())
            }
            Err(_) => {
                // The target app rejects AX: fall back to "snapshot → write → synthesized paste → restore"
                let snapshot = self.clipboard.snapshot();
                self.clipboard.set_text(text);
                self.clipboard.paste();
                self.clipboard.restore(snapshot);
                Ok(())
            }
        }
    }
}

/// AX ranges count UTF-16 code units (an emoji takes two units).
fn utf16_len(text: &str) -> i64 {
    text.encode_utf16().count() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    enum AxRead {
        Text(&'static str),
        NoSelection,
        Denied,
    }

    enum AxWrite {
        Ok,
        Denied,
    }

    struct FakeAx {
        read: AxRead,
        write: AxWrite,
        range: Option<(i64, i64)>,
        log: Arc<Mutex<Vec<String>>>,
    }

    impl Ax for FakeAx {
        fn selected_text(&self) -> Result<Option<String>, PlatformError> {
            match self.read {
                AxRead::Text(t) => Ok(Some(t.to_string())),
                AxRead::NoSelection => Ok(None),
                AxRead::Denied => Err(PlatformError::PermissionRequired),
            }
        }

        fn selected_text_range(&self) -> Option<(i64, i64)> {
            self.range
        }

        fn set_selected_range(&self, location: i64, length: i64) {
            self.log
                .lock()
                .unwrap()
                .push(format!("range:{location}:{length}"));
        }

        fn set_selected_text(&self, _text: &str) -> Result<(), PlatformError> {
            match self.write {
                AxWrite::Ok => Ok(()),
                AxWrite::Denied => Err(PlatformError::PermissionRequired),
            }
        }
    }

    #[derive(Default)]
    struct FakeClip {
        log: Arc<Mutex<Vec<String>>>,
        /// Scripted return-value queue for read_text (read once before and once after copy).
        reads: Mutex<std::collections::VecDeque<Option<String>>>,
    }

    impl FakeClip {
        fn with_reads(reads: Vec<Option<String>>) -> Self {
            Self {
                reads: Mutex::new(reads.into()),
                ..Self::default()
            }
        }
    }

    impl Clipboard for FakeClip {
        type Snapshot = &'static str;

        fn snapshot(&self) -> &'static str {
            self.log.lock().unwrap().push("snapshot".into());
            "SNAP"
        }

        fn set_text(&self, text: &str) {
            self.log.lock().unwrap().push(format!("set:{text}"));
        }

        fn read_text(&self) -> Option<String> {
            self.log.lock().unwrap().push("read".into());
            self.reads.lock().unwrap().pop_front().unwrap_or(None)
        }

        fn paste(&self) {
            self.log.lock().unwrap().push("paste".into());
        }

        fn copy(&self) {
            self.log.lock().unwrap().push("copy".into());
        }

        fn restore(&self, snapshot: &'static str) {
            self.log.lock().unwrap().push(format!("restore:{snapshot}"));
        }
    }

    type Log = Arc<Mutex<Vec<String>>>;
    type TestTarget = (HybridTextTarget<FakeAx, FakeClip>, Log, Log);

    fn target(read: AxRead, write: AxWrite, range: Option<(i64, i64)>) -> TestTarget {
        let clip = FakeClip::default();
        let clip_log = Arc::clone(&clip.log);
        let ax_log = Arc::new(Mutex::new(Vec::new()));
        let ax = FakeAx {
            read,
            write,
            range,
            log: Arc::clone(&ax_log),
        };
        (HybridTextTarget::new(ax, clip), clip_log, ax_log)
    }

    #[test]
    fn read_selection_passes_through() {
        let (t, clip_log, _) = target(AxRead::Text("hi"), AxWrite::Ok, None);
        assert_eq!(t.read_selection().unwrap(), Some("hi".to_string()));
        // The clipboard is never touched when AX has text
        assert!(clip_log.lock().unwrap().is_empty());
    }

    fn target_with_clip(read: AxRead, clip: FakeClip) -> (HybridTextTarget<FakeAx, FakeClip>, Log) {
        let log = Arc::clone(&clip.log);
        let ax = FakeAx {
            read,
            write: AxWrite::Ok,
            range: None,
            log: Arc::new(Mutex::new(Vec::new())),
        };
        (HybridTextTarget::new(ax, clip), log)
    }

    /// When AX can't read, fall back to clipboard copy: if the clipboard changed across ⌘C, the selection was copied.
    #[test]
    fn read_selection_falls_back_to_copy_when_ax_has_no_text() {
        let clip = FakeClip::with_reads(vec![
            Some("old clipboard".into()),
            Some("selected text".into()),
        ]);
        let (target, log) = target_with_clip(AxRead::NoSelection, clip);
        assert_eq!(
            target.read_selection().unwrap(),
            Some("selected text".to_string())
        );
        assert_eq!(
            *log.lock().unwrap(),
            ["snapshot", "read", "copy", "read", "restore:SNAP"]
        );
    }

    /// Clipboard unchanged across ⌘C: the app has no selection (or the copy failed) — must not treat the old clipboard as the selection.
    #[test]
    fn read_selection_ignores_unchanged_clipboard() {
        let clip = FakeClip::with_reads(vec![
            Some("old clipboard".into()),
            Some("old clipboard".into()),
        ]);
        let (target, _) = target_with_clip(AxRead::NoSelection, clip);
        assert_eq!(target.read_selection().unwrap(), None);
    }

    /// When AX is denied (unauthorized), also fall back to copy: use it if the copy succeeds, else None.
    #[test]
    fn read_selection_falls_back_when_ax_is_denied() {
        let clip = FakeClip::with_reads(vec![None, Some("copied".into())]);
        let (target, _) = target_with_clip(AxRead::Denied, clip);
        assert_eq!(target.read_selection().unwrap(), Some("copied".into()));
    }

    #[test]
    fn ax_write_success_never_touches_clipboard() {
        let (t, clip_log, _) = target(AxRead::NoSelection, AxWrite::Ok, None);
        assert!(t.write_text("HELLO").is_ok());
        assert!(clip_log.lock().unwrap().is_empty());
    }

    #[test]
    fn ax_write_failure_falls_back_to_clipboard_paste() {
        let (t, clip_log, ax_log) = target(AxRead::NoSelection, AxWrite::Denied, None);
        // A successful fallback still counts as a successful write back
        assert!(t.write_text("HELLO").is_ok());
        assert_eq!(
            *clip_log.lock().unwrap(),
            ["snapshot", "set:HELLO", "paste", "restore:SNAP"]
        );
        // The fallback path involves no AX range operations
        assert!(ax_log.lock().unwrap().is_empty());
    }

    #[test]
    fn ax_write_selects_inserted_text_using_utf16_length() {
        // The selection is at position 7 before writing; "a😀" is 3 UTF-16 units long (emoji takes two code units)
        let (t, clip_log, ax_log) = target(AxRead::NoSelection, AxWrite::Ok, Some((7, 3)));
        assert!(t.write_text("a😀").is_ok());
        assert_eq!(*ax_log.lock().unwrap(), ["range:7:3"]);
        assert!(clip_log.lock().unwrap().is_empty());
    }

    #[test]
    fn no_range_info_skips_selection() {
        let (t, _, ax_log) = target(AxRead::NoSelection, AxWrite::Ok, None);
        assert!(t.write_text("hi").is_ok());
        assert!(ax_log.lock().unwrap().is_empty());
    }
}
