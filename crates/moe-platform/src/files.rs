//! Finder file selection (ADR-0021): grab the "selected files" when the panel is summoned; they become panel context together with the `Selection` (AI questions automatically include these files).
//!
//! Scope is Finder-only for now: other apps have no unified "file selection" AX protocol, and the most common
//! "select files → summon" flow is Finder, so start there and extend per-app later.
//! This is best-effort: failure to read (not Finder, no Automation permission, timeout) all count as "no files";
//! file grabbing must never slow down or interrupt summoning. The testable part (output parsing) lives in this module; platform calls stay thin.

/// Parse `osascript` output: one POSIX path per line.
/// Blank lines are dropped; `\r` and leading/trailing whitespace are handled by trim (Finder file names never contain leading/trailing whitespace or newlines).
pub fn parse_finder_paths(output: &str) -> Vec<String> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(String::from)
        .collect()
}

/// Currently selected files: only non-empty on macOS when Finder is frontmost; empty otherwise.
#[cfg(target_os = "macos")]
pub fn finder_selection() -> Vec<String> {
    imp::finder_selection()
}

/// Non-macOS platforms have no Finder: always empty (no error; the summon flow is unaffected).
#[cfg(not(target_os = "macos"))]
pub fn finder_selection() -> Vec<String> {
    Vec::new()
}

/// The frontmost application's bundle identifier (diagnostics: which app a summon was captured from).
#[cfg(target_os = "macos")]
pub fn frontmost_bundle_id() -> Option<String> {
    imp::frontmost_bundle_id()
}

#[cfg(not(target_os = "macos"))]
pub fn frontmost_bundle_id() -> Option<String> {
    None
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

    /// One line = one path: Windows/Unix separators inside file names are not treated as separators.
    #[test]
    fn single_path_keeps_commas_and_colons() {
        assert_eq!(
            parse_finder_paths("/tmp/a,comma:colon.md\n"),
            ["/tmp/a,comma:colon.md"]
        );
    }
}

/// macOS platform implementation: NSWorkspace frontmost check + AppleScript reading the Finder selection.
#[cfg(target_os = "macos")]
mod imp {
    use super::*;
    use objc2_app_kit::NSWorkspace;
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::{Duration, Instant};

    const FINDER_BUNDLE_ID: &str = "com.apple.finder";
    /// osascript time limit: while the TCC "Automation" permission prompt is showing, osascript hangs waiting for an answer;
    /// the panel must not be held up by it — kill it on timeout and treat this run as having no files.
    const OSC_SCRIPT_TIMEOUT: Duration = Duration::from_millis(2500);

    /// Join with linefeeds instead of AppleScript's default commas — paths may contain commas, newlines won't.
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

    pub(super) fn frontmost_bundle_id() -> Option<String> {
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
        // Timed wait: osascript hangs while the permission prompt shows; kill on timeout (retry on the next summon).
        let deadline = Instant::now() + OSC_SCRIPT_TIMEOUT;
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(40));
                }
                _ => {
                    let _ = child.kill();
                    eprintln!("moe: osascript Finder selection read timed out (ignored)");
                    return Vec::new();
                }
            }
        }
        let output = match child.wait_with_output() {
            Ok(out) if out.status.success() => out.stdout,
            _ => {
                // Common when the "Automation" permission is denied or unanswered: degrade silently, don't interrupt the summon.
                eprintln!(
                    "moe: Finder selection read failed (ignored; first run needs the \"Automation\" permission)"
                );
                return Vec::new();
            }
        };
        let paths = parse_finder_paths(&String::from_utf8_lossy(&output));
        if !paths.is_empty() {
            eprintln!("moe: Finder has {} selected files", paths.len());
        }
        paths
    }
}
