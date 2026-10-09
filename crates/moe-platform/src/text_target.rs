//! TextTarget's platform-agnostic strategy layer (ADR-0002): AX first, falling back to the
//! clipboard only when a selection exists whose text the app doesn't expose.
//!
//! Platform glue implements [`Ax`] and [`Clipboard`] separately; the strategy here (when to touch
//! the clipboard, the order of calls, how long the fallback copy's observation runs) is testable
//! logic.
//!
//! The capture is split (MOE-0013): the synthesized ⌘C must fire before the panel takes key, but
//! *observing* whether it landed does not. A copy that does not land within [`FALLBACK_GRACE`]
//! leaves a pending tail for the caller to finish off the main thread, so the panel shows
//! immediately instead of waiting out the whole budget.

use std::time::{Duration, Instant};

use crate::{Capture, PendingCapture, PlatformError, TextTarget};

/// How long the pre-show phase lets the fallback copy land before handing back a pending capture:
/// a healthy app puts its selection on the pasteboard within a poll or two (5–15 ms), and the
/// grace also gives the synthesized ⌘C time to be dispatched to the host app before the panel
/// takes key.
const FALLBACK_GRACE: Duration = Duration::from_millis(20);
/// Total observation budget for the fallback copy, grace included: "nothing selected" can only be
/// proven at its end. Expiring the grace hands the remaining wait to the background finisher.
const FALLBACK_BUDGET: Duration = Duration::from_millis(100);
/// Pasteboard polling interval while observing the fallback copy.
const COPY_POLL_INTERVAL: Duration = Duration::from_millis(5);

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
pub trait Clipboard: Send + Sync + Clone {
    /// An implementation-defined opaque snapshot (items × types × data on Mac); it may be moved to
    /// the thread that finishes a pending capture, so it is Send.
    type Snapshot: Send;

    /// Handle observing an in-flight synthesized copy ([`Clipboard::copy_begin`]).
    type CopyWatch: Send;

    fn snapshot(&self) -> Self::Snapshot;
    fn set_text(&self, text: &str);
    /// Read the current clipboard text (None when there is none).
    fn read_text(&self) -> Option<String>;
    /// Synthesize ⌘V / Ctrl+V, then wait for the target app to consume the clipboard (the caller
    /// restores the snapshot right afterwards).
    fn paste(&self);
    /// Synthesize ⌘C / Ctrl+C *without waiting*: returns a watch to poll with
    /// [`Clipboard::copy_landed`]. The strategy owns the wait — it continues across the panel show
    /// when the grace expires.
    fn copy_begin(&self) -> Self::CopyWatch;
    /// Whether the target app's pasteboard write has landed since `copy_begin` (observed through
    /// the platform's pasteboard version counter on Mac, where there is no completion signal).
    /// Records the pasteboard version in the watch the first time it lands.
    fn copy_landed(&self, watch: &mut Self::CopyWatch) -> bool;
    /// Whether the pasteboard changed after our copy landed — someone else wrote (e.g. the user
    /// copied from the panel), and the snapshot must not clobber that. `false` when it never landed.
    fn changed_since_copied(&self, watch: &Self::CopyWatch) -> bool;
    /// Write the snapshot back (the wait for the target app belongs to [`Clipboard::paste`]; the
    /// copy side is waited on by the strategy).
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

impl<A: Ax, C: Clipboard + 'static> TextTarget for HybridTextTarget<A, C> {
    fn capture_selection(&self) -> Capture {
        if let Ok(Some(text)) = self.ax.selected_text() {
            return Capture::Done(Some(text));
        }
        // AX handed over no text: either nothing is selected, or a selection exists whose text the
        // app doesn't expose. An explicit empty range is a cursor and nothing more — the ⌘C round
        // trip below would burn its wait budget for nothing (and fire a stray copy into the host
        // app), so skip it. A missing range keeps the fallback: that is the case the fallback is
        // for (e.g. Zed doesn't expose AXSelectedText).
        if matches!(self.ax.selected_text_range(), Some((_, length)) if length <= 0) {
            return Capture::Done(None);
        }
        // AX can't read it (e.g. Zed doesn't expose AXSelectedText): fall back to clipboard copy.
        // The ⌘C must fire now, while the host app still has focus — that is the one part of the
        // fallback that cannot run after the panel becomes key.
        // Compare the clipboard before and after ⌘C: no change = the app has no selection (avoid
        // treating the old clipboard as the selection).
        let snapshot = self.clipboard.snapshot();
        let before = self.clipboard.read_text();
        let mut watch = self.clipboard.copy_begin();
        let started = Instant::now();
        let grace_deadline = started + FALLBACK_GRACE;
        loop {
            if self.clipboard.copy_landed(&mut watch) {
                let after = self.clipboard.read_text();
                if !self.clipboard.changed_since_copied(&watch) {
                    self.clipboard.restore(snapshot);
                }
                return Capture::Done(filter_selection(after, before));
            }
            if Instant::now() >= grace_deadline {
                break;
            }
            std::thread::sleep(COPY_POLL_INTERVAL);
        }
        // The copy is still in flight: hand the observation to the caller's finisher. The panel
        // shows now; a selection that lands late still becomes context, because `state.selection`
        // is read when a command is invoked, not when the panel shows.
        Capture::Pending(Box::new(PendingFallback {
            clipboard: self.clipboard.clone(),
            snapshot,
            before,
            watch,
            deadline: started + FALLBACK_BUDGET,
        }))
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

/// The tail of a fallback capture whose copy may not have landed yet (see
/// [`TextTarget::capture_selection`]): keeps observing the pasteboard off the main thread.
struct PendingFallback<C: Clipboard> {
    /// A clone of the strategy's clipboard handle (the original stays with the caller).
    clipboard: C,
    snapshot: C::Snapshot,
    before: Option<String>,
    watch: C::CopyWatch,
    /// When the observation ends, shared with the pre-show grace (the total budget).
    deadline: Instant,
}

impl<C: Clipboard + 'static> PendingCapture for PendingFallback<C> {
    fn finish(self: Box<Self>) -> Option<String> {
        let Self {
            clipboard,
            snapshot,
            before,
            mut watch,
            deadline,
        } = *self;
        loop {
            if clipboard.copy_landed(&mut watch) {
                let after = clipboard.read_text();
                if !clipboard.changed_since_copied(&watch) {
                    clipboard.restore(snapshot);
                }
                return filter_selection(after, before);
            }
            if Instant::now() >= deadline {
                // No write ever landed: the clipboard was never touched — nothing to read back and
                // nothing to restore (no churn on the "nothing selected" path).
                return None;
            }
            std::thread::sleep(COPY_POLL_INTERVAL);
        }
    }
}

/// The fallback's verdict: keep the post-copy clipboard text unless it is empty/whitespace-only
/// or identical to the pre-copy clipboard (then nothing was copied).
fn filter_selection(after: Option<String>, before: Option<String>) -> Option<String> {
    after.filter(|text| !text.trim().is_empty() && Some(text.as_str()) != before.as_deref())
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

    /// Scripted clipboard. `landed` is the `copy_landed` verdict (the test flips it between the
    /// capture and the finisher to simulate a copy that lands after the grace); `changed_since`
    /// scripts another writer having taken the pasteboard after our copy landed.
    #[derive(Clone, Default)]
    struct FakeClip {
        log: Arc<Mutex<Vec<String>>>,
        /// Scripted return-value queue for read_text (read once before and once after copy).
        reads: Arc<Mutex<std::collections::VecDeque<Option<String>>>>,
        landed: Arc<Mutex<bool>>,
        changed_since: Arc<Mutex<bool>>,
    }

    impl FakeClip {
        fn with_reads(reads: Vec<Option<String>>) -> Self {
            Self {
                reads: Arc::new(Mutex::new(reads.into())),
                ..Self::default()
            }
        }

        /// A copy that lands immediately — a real selection in a healthy app.
        fn landing_now(reads: Vec<Option<String>>) -> Self {
            let clip = Self::with_reads(reads);
            *clip.landed.lock().unwrap() = true;
            clip
        }

        /// Handle to flip "the copy has landed" from the test.
        fn landing(&self) -> Arc<Mutex<bool>> {
            Arc::clone(&self.landed)
        }

        /// Someone else wrote the pasteboard after our copy landed (e.g. the user copied from the panel).
        fn moved_on(&self) {
            *self.changed_since.lock().unwrap() = true;
        }
    }

    impl Clipboard for FakeClip {
        type Snapshot = &'static str;
        type CopyWatch = ();

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

        fn copy_begin(&self) {
            self.log.lock().unwrap().push("copy".into());
        }

        fn copy_landed(&self, _watch: &mut ()) -> bool {
            *self.landed.lock().unwrap()
        }

        fn changed_since_copied(&self, _watch: &()) -> bool {
            *self.changed_since.lock().unwrap()
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

    /// Drive a capture to its final text: `Done` passes through; `Pending` finishes inline (the
    /// production caller runs that tail off the main thread).
    fn finish(capture: Capture) -> Option<String> {
        match capture {
            Capture::Done(text) => text,
            Capture::Pending(pending) => pending.finish(),
        }
    }

    #[test]
    fn ax_text_wins_without_touching_the_clipboard() {
        let (t, clip_log, _) = target(AxRead::Text("hi"), AxWrite::Ok, None);
        assert_eq!(finish(t.capture_selection()), Some("hi".to_string()));
        // The clipboard is never touched when AX has text
        assert!(clip_log.lock().unwrap().is_empty());
    }

    /// An empty AX range is a cursor and nothing more: the fallback must not run at all — no wait
    /// budget spent, no stray ⌘C fired into the host app (the summon-path latency fix).
    #[test]
    fn empty_ax_range_skips_the_clipboard_fallback() {
        let (t, clip_log, _) = target(AxRead::NoSelection, AxWrite::Ok, Some((12, 0)));
        assert_eq!(finish(t.capture_selection()), None);
        assert!(clip_log.lock().unwrap().is_empty());
    }

    /// A copy that lands within the grace completes before the panel would show: read, restore,
    /// done — the pre-show behavior as it always was for a real selection.
    #[test]
    fn copy_landing_within_the_grace_captures_and_restores() {
        let clip = FakeClip::landing_now(vec![
            Some("old clipboard".into()),
            Some("selected text".into()),
        ]);
        let (target, log) = target_with_clip(AxRead::NoSelection, clip);
        let capture = target.capture_selection();
        assert!(matches!(capture, Capture::Done(_)));
        assert_eq!(finish(capture), Some("selected text".to_string()));
        assert_eq!(
            *log.lock().unwrap(),
            ["snapshot", "read", "copy", "read", "restore:SNAP"]
        );
    }

    /// The pasteboard version moved but the text did not: the app wrote something that is not a
    /// text selection — the old clipboard must not be treated as the selection.
    #[test]
    fn landed_copy_without_new_text_is_not_a_selection() {
        let clip = FakeClip::landing_now(vec![
            Some("old clipboard".into()),
            Some("old clipboard".into()),
        ]);
        let (target, _) = target_with_clip(AxRead::NoSelection, clip);
        assert_eq!(finish(target.capture_selection()), None);
    }

    /// When AX is denied (unauthorized), also fall back to copy: use it if the copy succeeds, else None.
    #[test]
    fn read_selection_falls_back_when_ax_is_denied() {
        let clip = FakeClip::landing_now(vec![None, Some("copied".into())]);
        let (target, _) = target_with_clip(AxRead::Denied, clip);
        assert_eq!(finish(target.capture_selection()), Some("copied".into()));
    }

    /// Nothing selected in an AX-opaque app: the grace expires without a pasteboard write, the
    /// capture goes pending, and the finisher concludes "none" without ever reading or rewriting
    /// the clipboard (the never-landed path has no churn).
    #[test]
    fn never_landing_copy_goes_pending_and_finishes_without_churn() {
        let clip = FakeClip::with_reads(vec![Some("old clipboard".into())]);
        let (target, log) = target_with_clip(AxRead::NoSelection, clip);
        let capture = target.capture_selection();
        assert!(matches!(capture, Capture::Pending(_)));
        assert_eq!(finish(capture), None);
        assert_eq!(*log.lock().unwrap(), ["snapshot", "read", "copy"]);
    }

    /// A copy can land after the grace: the finisher still captures it — off the main thread (the
    /// pending tail is Send by contract).
    #[test]
    fn copy_landing_after_the_grace_is_captured_by_the_finisher() {
        let clip = FakeClip::with_reads(vec![
            Some("old clipboard".into()),
            Some("selected text".into()),
        ]);
        let landing = clip.landing();
        let (target, log) = target_with_clip(AxRead::NoSelection, clip);
        let Capture::Pending(pending) = target.capture_selection() else {
            panic!("expected a pending capture");
        };
        // The write lands now (after the grace expired, before the finisher's first check).
        *landing.lock().unwrap() = true;
        let text = std::thread::spawn(move || pending.finish()).join().unwrap();
        assert_eq!(text, Some("selected text".to_string()));
        assert_eq!(
            *log.lock().unwrap(),
            ["snapshot", "read", "copy", "read", "restore:SNAP"]
        );
    }

    /// The snapshot must not clobber a write that happened after our copy landed (e.g. the user
    /// copied a result from the panel while the fallback was still pending): the text is still
    /// captured, the restore is skipped.
    #[test]
    fn restore_is_skipped_when_the_pasteboard_moved_on() {
        let clip = FakeClip::landing_now(vec![
            Some("old clipboard".into()),
            Some("selected text".into()),
        ]);
        clip.moved_on();
        let (target, log) = target_with_clip(AxRead::NoSelection, clip);
        assert_eq!(
            finish(target.capture_selection()),
            Some("selected text".to_string())
        );
        assert_eq!(*log.lock().unwrap(), ["snapshot", "read", "copy", "read"]);
    }

    /// A non-empty AX range means a selection exists whose text the app doesn't expose: the
    /// clipboard fallback still reads it.
    #[test]
    fn nonempty_ax_range_still_falls_back_to_copy() {
        let clip = FakeClip::landing_now(vec![
            Some("old clipboard".into()),
            Some("selected text".into()),
        ]);
        let log = Arc::clone(&clip.log);
        let ax = FakeAx {
            read: AxRead::NoSelection,
            write: AxWrite::Ok,
            range: Some((3, 5)),
            log: Arc::new(Mutex::new(Vec::new())),
        };
        let target = HybridTextTarget::new(ax, clip);
        assert_eq!(
            finish(target.capture_selection()),
            Some("selected text".to_string())
        );
        assert_eq!(
            *log.lock().unwrap(),
            ["snapshot", "read", "copy", "read", "restore:SNAP"]
        );
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
