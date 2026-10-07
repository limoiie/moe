//! Opening external URLs in the system default browser (ADR-0026): the About card's
//! "Send Feedback" lands here. Only http/https reaches the OS; anything else is refused
//! before any process is spawned.

use std::process::Command;

/// Open `url` in the system default browser. Rejects non-http(s) schemes.
pub fn open_url(url: &str) -> Result<(), String> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(format!("refusing to open a non-http(s) URL: {url}"));
    }
    let status = if cfg!(target_os = "macos") {
        Command::new("open").arg(url).status()
    } else {
        Command::new("xdg-open").arg(url).status()
    };
    match status {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(format!("opening the URL failed with {status}")),
        Err(err) => Err(format!("opening the URL failed: {err}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Only http(s) is accepted; dangerous schemes never reach the OS (no process spawned).
    #[test]
    fn refuses_non_http_schemes() {
        for url in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "moe://whatever",
            "ftp://example.com",
            "",
            "httpsx://example.com",
        ] {
            assert!(open_url(url).is_err(), "{url} must be refused");
        }
    }
}
