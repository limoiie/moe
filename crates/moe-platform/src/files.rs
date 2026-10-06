//! Finder 文件选择（ADR-0021）：呼出面板时抓「选中的文件」，随 `Selection` 一起
//! 成为面板上下文（AI 提问自动附上这些文件）。
//!
//! 范围先只到 Finder：其它应用没有统一的「文件选择」AX 协议，最常见的
//! 「选中文件 → 呼出」场景就是 Finder，从它做起；将来按应用逐个扩。
//! 这里是 best-effort：读不到（非 Finder、无自动化授权、超时）都视为「没有文件」，
//! 绝不让文件抓取拖慢或打断呼出。可测部分（输出解析）放本模块，平台调用保持薄。

use std::time::Duration;

/// 解析 `osascript` 的输出：一行一个 POSIX 路径。
/// 空行丢弃；`\r` 与首尾空白由 trim 处理（Finder 文件名本身不含首尾空白/换行）。
pub fn parse_finder_paths(output: &str) -> Vec<String> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(String::from)
        .collect()
}

/// 当前选中的文件：仅 macOS 且前台是 Finder 时有内容；其余情况为空。
#[cfg(target_os = "macos")]
pub fn finder_selection() -> Vec<String> {
    imp::finder_selection()
}

/// 非 macOS 平台没有 Finder：恒为空（不报错，呼出流程不受影响）。
#[cfg(not(target_os = "macos"))]
pub fn finder_selection() -> Vec<String> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_path_lines_and_ignores_blanks() {
        assert_eq!(
            parse_finder_paths("/tmp/a.md\n/tmp/b c.png\r\n\n  \n/tmp/d.txt\n"),
            ["/tmp/a.md", "/tmp/b c.png", "/tmp/d.txt"]
        );
        assert_eq!(parse_finder_paths(""), Vec::<String>::new());
        assert_eq!(parse_finder_paths("   \n\n"), Vec::<String>::new());
    }

    /// 单行即单路径：Windows/Unix 分隔符混在文件名里不会被当分隔符。
    #[test]
    fn single_path_keeps_commas_and_colons() {
        assert_eq!(
            parse_finder_paths("/tmp/a,逗号:colon.md\n"),
            ["/tmp/a,逗号:colon.md"]
        );
    }
}

/// macOS 平台实现：NSWorkspace 前台判定 + AppleScript 读 Finder selection。
#[cfg(target_os = "macos")]
mod imp {
    use super::*;
    use objc2_app_kit::NSWorkspace;
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::Instant;

    const FINDER_BUNDLE_ID: &str = "com.apple.finder";
    /// osascript 限时：TCC「自动化」授权弹窗期间 osascript 会挂起等待用户应答，
    /// 呼出面板不能被它拖住——超时就杀掉，本次当作没有文件。
    const OSC_SCRIPT_TIMEOUT: Duration = Duration::from_millis(2500);

    /// 用 linefeed 而不是 AppleScript 默认的逗号拼接——路径本身可能含逗号，换行不会。
    const SCRIPT: &str = r#"tell application "Finder"
    set _sel to selection
    set _paths to {}
    repeat with _f in _sel
        set end of _paths to (POSIX path of (_f as alias))
    end repeat
    set AppleScript's text item delimiters to linefeed
    set _out to (_paths as text)
    set AppleScript's text item delimiters to {""}
    return _out
end tell"#;

    fn frontmost_bundle_id() -> Option<String> {
        let workspace = NSWorkspace::sharedWorkspace();
        let app = workspace.frontmostApplication()?;
        app.bundleIdentifier().map(|id| id.to_string())
    }

    pub fn finder_selection() -> Vec<String> {
        if frontmost_bundle_id().as_deref() != Some(FINDER_BUNDLE_ID) {
            return Vec::new();
        }
        let Ok(mut child) = Command::new("/usr/bin/osascript")
            .arg("-e")
            .arg(SCRIPT)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
        else {
            return Vec::new();
        };
        // 限时等待：授权弹窗期间 osascript 挂起，超时杀掉（下次呼出再试）。
        let deadline = Instant::now() + OSC_SCRIPT_TIMEOUT;
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(40));
                }
                _ => {
                    let _ = child.kill();
                    eprintln!("moe: osascript 读 Finder 选择超时（忽略）");
                    return Vec::new();
                }
            }
        }
        let output = match child.wait_with_output() {
            Ok(out) if out.status.success() => out.stdout,
            _ => {
                // 常见于「自动化」授权被拒或未应答：静默降级，不打断呼出。
                eprintln!("moe: Finder 选择读取失败（忽略，首次需「自动化」授权）");
                return Vec::new();
            }
        };
        let paths = parse_finder_paths(&String::from_utf8_lossy(&output));
        if !paths.is_empty() {
            eprintln!("moe: Finder 选中 {} 个文件", paths.len());
        }
        paths
    }
}
