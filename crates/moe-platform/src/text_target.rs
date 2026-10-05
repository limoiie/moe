//! TextTarget 的平台无关策略层（ADR-0002）：AX 优先，失败才降级到剪贴板粘贴。
//!
//! 平台胶水各自实现 [`Ax`] 与 [`Clipboard`]；这里的策略（何时动剪贴板、
//! 调用的先后顺序）是可测的纯逻辑。

use crate::{PlatformError, TextTarget};

/// 系统辅助功能（Accessibility）读写焦点元素选区的边界。
pub trait Ax: Send + Sync {
    /// 当前焦点元素的选区文本；无选区为 `Ok(None)`（光标模式）。
    fn selected_text(&self) -> Result<Option<String>, PlatformError>;

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
        match self.ax.set_selected_text(text) {
            Ok(()) => Ok(()),
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
    }

    impl Ax for FakeAx {
        fn selected_text(&self) -> Result<Option<String>, PlatformError> {
            match self.read {
                AxRead::Text(t) => Ok(Some(t.to_string())),
                AxRead::NoSelection => Ok(None),
                AxRead::Denied => Err(PlatformError::PermissionRequired),
            }
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

    fn target(
        read: AxRead,
        write: AxWrite,
    ) -> (HybridTextTarget<FakeAx, FakeClip>, Arc<Mutex<Vec<String>>>) {
        let clip = FakeClip::default();
        let log = Arc::clone(&clip.log);
        (HybridTextTarget::new(FakeAx { read, write }, clip), log)
    }

    #[test]
    fn read_selection_passes_through() {
        let (t, _) = target(AxRead::Text("hi"), AxWrite::Ok);
        assert_eq!(t.read_selection().unwrap(), Some("hi".to_string()));
        let (t, _) = target(AxRead::NoSelection, AxWrite::Ok);
        assert_eq!(t.read_selection().unwrap(), None);
        let (t, _) = target(AxRead::Denied, AxWrite::Ok);
        assert!(matches!(
            t.read_selection(),
            Err(PlatformError::PermissionRequired)
        ));
    }

    #[test]
    fn ax_write_success_never_touches_clipboard() {
        let (t, log) = target(AxRead::NoSelection, AxWrite::Ok);
        assert!(t.write_text("HELLO").is_ok());
        assert!(log.lock().unwrap().is_empty());
    }

    #[test]
    fn ax_write_failure_falls_back_to_clipboard_paste() {
        let (t, log) = target(AxRead::NoSelection, AxWrite::Denied);
        // 降级成功也算回写成功
        assert!(t.write_text("HELLO").is_ok());
        assert_eq!(
            *log.lock().unwrap(),
            ["snapshot", "set:HELLO", "paste", "restore:SNAP"]
        );
    }
}
