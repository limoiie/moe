//! 平台边界（ADR-0002/0008）：与宿主应用的文字互动、呼出键监听。
//! 当前只有 trait 定义与 stub——macOS AX 实现是 M2，CGEventTap 双击 ⌘ 是 M1 下一 ticket。

use std::fmt;

#[derive(Debug)]
pub enum PlatformError {
    /// 需要辅助功能/输入监控授权；调用方应展示内联引导（ADR-0008）。
    PermissionRequired,
    /// 该平台/会话下不可用（如 Wayland 读选区）。
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

/// Command 层只通过这个边界读写宿主应用文本，不感知具体手段。
pub trait TextTarget: Send + Sync {
    /// 抓取当前 Selection（文字）。无选区返回 Ok(None)，走光标降级。
    fn read_selection(&self) -> Result<Option<String>, PlatformError>;

    /// 有 Selection 则替换之，否则插入到光标处。
    fn write_text(&self, text: &str) -> Result<(), PlatformError>;
}

/// 呼出监听（默认：双击 ⌘，ADR-0008）。
pub trait SummonListener: Send + Sync {
    /// 修饰键双击依赖事件监听，macOS 上即需授权。
    fn is_authorized(&self) -> bool;
    fn start(&mut self, callback: Box<dyn Fn() + Send>);
}

/// 全平台占位实现：在真实实现落地前让上层可以编译与接线。
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
