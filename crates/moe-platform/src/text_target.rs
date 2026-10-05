//! TextTarget 的平台无关策略层（ADR-0002）：AX 优先，失败才降级到剪贴板粘贴。
//!
//! 平台胶水各自实现 [`Ax`] 与 [`Clipboard`]；这里的策略（何时动剪贴板、
//! 调用的先后顺序）是可测的纯逻辑。

use crate::{PlatformError, TextTarget};

/// 系统辅助功能（Accessibility）读写焦点元素选区的边界。
pub trait Ax: Send + Sync {
    /// 当前焦点元素的选区文本；无选区为 `Ok(None)`（光标模式）。
    fn selected_text(&self) -> Result<Option<String>, PlatformError>;

    /// 当前选区/光标范围 `(location, length)`（UTF-16 码元）；
    /// 应用不支持时为 `None`（此时跳过「回写后选中」）。
    fn selected_text_range(&self) -> Option<(i64, i64)> {
        None
    }

    /// 把选区设为指定范围（回写后选中写入内容，便于确认/重写）；尽力而为。
    fn set_selected_range(&self, _location: i64, _length: i64) {}

    /// 有选区则替换；无选区（光标态）则插入。
    fn set_selected_text(&self, text: &str) -> Result<(), PlatformError>;
}

/// 剪贴板边界：快照 / 写入 / 恢复 / 合成粘贴。
pub trait Clipboard: Send + Sync {
    /// 实现自定义的不透明快照（Mac 上是 items × types × data）；
    /// 生命周期仅在单次 write_text 同步调用内，因此不要求 Send。
    type Snapshot;

    fn snapshot(&self) -> Self::Snapshot;
    fn set_text(&self, text: &str);
    /// 合成 ⌘V / Ctrl+V。
    fn paste(&self);
    /// 等目标应用消费完粘贴后写回快照（含必要的延迟）。
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
        self.ax.selected_text()
    }

    fn write_text(&self, text: &str) -> Result<(), PlatformError> {
        // 先记下写入点（选区起点/光标位），写完后把新内容选中
        let insert_at = self.ax.selected_text_range();
        match self.ax.set_selected_text(text) {
            Ok(()) => {
                if let Some((location, _)) = insert_at {
                    self.ax.set_selected_range(location, utf16_len(text));
                }
                Ok(())
            }
            Err(_) => {
                // AX 不被目标应用接受：降级为「快照 → 写入 → 合成粘贴 → 恢复」
                let snapshot = self.clipboard.snapshot();
                self.clipboard.set_text(text);
                self.clipboard.paste();
                self.clipboard.restore(snapshot);
                Ok(())
            }
        }
    }
}

/// AX 范围用 UTF-16 码元计数（emoji 占两个单位）。
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

        fn paste(&self) {
            self.log.lock().unwrap().push("paste".into());
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
        let (t, _, _) = target(AxRead::Text("hi"), AxWrite::Ok, None);
        assert_eq!(t.read_selection().unwrap(), Some("hi".to_string()));
        let (t, _, _) = target(AxRead::NoSelection, AxWrite::Ok, None);
        assert_eq!(t.read_selection().unwrap(), None);
        let (t, _, _) = target(AxRead::Denied, AxWrite::Ok, None);
        assert!(matches!(
            t.read_selection(),
            Err(PlatformError::PermissionRequired)
        ));
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
        // 降级成功也算回写成功
        assert!(t.write_text("HELLO").is_ok());
        assert_eq!(
            *clip_log.lock().unwrap(),
            ["snapshot", "set:HELLO", "paste", "restore:SNAP"]
        );
        // 降级路径不涉及 AX 范围操作
        assert!(ax_log.lock().unwrap().is_empty());
    }

    #[test]
    fn ax_write_selects_inserted_text_using_utf16_length() {
        // 写入前选区在位置 7；"a😀" 的 UTF-16 长度是 3（emoji 占两个码元）
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
