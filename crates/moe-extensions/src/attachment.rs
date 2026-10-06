//! Attachments (ADR-0005 v1: text files + images; ADR-0010 `@path` mentions).
//!
//! Mention syntax: `@/abs/path` (up to whitespace) or `@"path with spaces"`.
//! The panel's ⌘⇧A and the side view's 📎 only do "validate + insert mention"; reading and expanding happen here.

use std::path::{Path, PathBuf};

use base64::Engine as _;
use moe_core::conversation::{AttachmentRef, Message};

/// Text attachment limit (read in as message content).
pub const TEXT_LIMIT: u64 = 512 * 1024;
/// Image attachment limit (base64 into multimodal requests).
pub const IMAGE_LIMIT: u64 = 5 * 1024 * 1024;

const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp"];
const TEXT_EXTS: &[&str] = &[
    "md",
    "markdown",
    "txt",
    "rs",
    "toml",
    "json",
    "yaml",
    "yml",
    "csv",
    "ts",
    "tsx",
    "js",
    "jsx",
    "py",
    "go",
    "java",
    "c",
    "h",
    "cpp",
    "hpp",
    "cc",
    "css",
    "html",
    "htm",
    "xml",
    "sh",
    "zsh",
    "bash",
    "sql",
    "log",
    "ini",
    "cfg",
    "conf",
    "env",
    "lock",
    "gitignore",
    "vue",
    "svelte",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Text,
    Image,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Image => "image",
        }
    }
}

/// Read-only inspection result: used by the panel/side view for "validate + display", without reading content.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentInfo {
    pub name: String,
    /// Resolved absolute path (`~` expanded).
    pub path: String,
    /// "text" | "image"
    pub kind: &'static str,
    pub bytes: u64,
}

/// Attachment content after reading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    pub kind: Kind,
    /// Text content, or a `data:` URL for images (base64).
    pub payload: String,
}

/// Expands `~` to home; everything else is kept as-is (relative paths resolve against the process cwd).
pub fn expand_path(raw: &str) -> PathBuf {
    let raw = raw.trim();
    if let Some(rest) = raw.strip_prefix("~/")
        && let Some(home) = dirs_home()
    {
        return home.join(rest);
    }
    if raw == "~"
        && let Some(home) = dirs_home()
    {
        return home;
    }
    PathBuf::from(raw)
}

fn dirs_home() -> Option<PathBuf> {
    // On Windows HOME is often unset; fall back to USERPROFILE
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// Extracts mentions: returns (visible text with mentions stripped, list of expanded paths).
/// `@` must sit on a token boundary (start of line or after whitespace); `foo@bar.com` does not count.
pub fn parse_mentions(input: &str) -> (String, Vec<PathBuf>) {
    let chars: Vec<char> = input.chars().collect();
    let mut cleaned = String::new();
    let mut paths = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let at_boundary = i == 0 || chars[i - 1].is_whitespace();
        if chars[i] == '@' && at_boundary {
            if chars.get(i + 1) == Some(&'"') {
                // @"path with spaces"
                if let Some(end) = chars[i + 2..].iter().position(|c| *c == '"') {
                    let token: String = chars[i + 2..i + 2 + end].iter().collect();
                    if !token.trim().is_empty() {
                        paths.push(expand_path(&token));
                    }
                    i = i + 2 + end + 1;
                    continue;
                }
            } else {
                let mut j = i + 1;
                let mut token = String::new();
                while j < chars.len() && !chars[j].is_whitespace() {
                    token.push(chars[j]);
                    j += 1;
                }
                if !token.is_empty() {
                    paths.push(expand_path(&token));
                    i = j;
                    continue;
                }
            }
        }
        cleaned.push(chars[i]);
        i += 1;
    }
    (tidy(&cleaned), paths)
}

/// Removes extra whitespace left behind by mentions (preserving line structure).
fn tidy(text: &str) -> String {
    text.lines()
        .map(|line| {
            let mut out = String::new();
            let mut last_space = false;
            for ch in line.trim().chars() {
                if ch == ' ' {
                    if !last_space {
                        out.push(ch);
                    }
                    last_space = true;
                } else {
                    out.push(ch);
                    last_space = false;
                }
            }
            out
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// Builds the reference used for persistence (does not read the file: files are read at request time).
pub fn ref_for_path(path: &Path) -> AttachmentRef {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());
    AttachmentRef {
        name,
        path: path.to_string_lossy().to_string(),
    }
}

/// Inspects a file: existence, kind (extension + UTF-8 sniffing), size limit; `~` expansion matches `load`.
pub fn inspect(raw_path: &Path) -> Result<AttachmentInfo, String> {
    if raw_path.as_os_str().is_empty() {
        return Err("Path is empty".into());
    }
    let path = expand_path(&raw_path.to_string_lossy());
    let meta =
        std::fs::metadata(&path).map_err(|_| format!("File not found: {}", path.display()))?;
    if meta.is_dir() {
        return Err(format!(
            "This is a directory, not a file: {}",
            path.display()
        ));
    }
    let bytes = meta.len();
    let head = read_head(&path, 8192)?;
    let kind = classify(&path, &head)?;
    limit_of(kind, bytes)?;
    Ok(AttachmentInfo {
        name: path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string()),
        path: path.to_string_lossy().to_string(),
        kind: kind.as_str(),
        bytes,
    })
}

/// Reads attachment content (text → as-is; image → `data:` URL).
pub fn load(raw_path: &Path) -> Result<Attachment, String> {
    let path = expand_path(&raw_path.to_string_lossy());
    let meta =
        std::fs::metadata(&path).map_err(|_| format!("File not found: {}", path.display()))?;
    if meta.is_dir() {
        return Err(format!(
            "This is a directory, not a file: {}",
            path.display()
        ));
    }
    let bytes = std::fs::read(&path).map_err(|err| format!("Failed to read: {err}"))?;
    let kind = classify(&path, &bytes)?;
    limit_of(kind, meta.len())?;
    match kind {
        Kind::Text => {
            let text = String::from_utf8(bytes).map_err(|_| "Not valid UTF-8 text".to_string())?;
            Ok(Attachment {
                kind,
                payload: text,
            })
        }
        Kind::Image => {
            let mime = mime_of(&path);
            let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
            Ok(Attachment {
                kind,
                payload: format!("data:{mime};base64,{encoded}"),
            })
        }
    }
}

/// One message → one OpenAI messages entry: text inlined, images into the multimodal content array.
/// Unreadable attachments do not fail the whole question; a readable note is left in the text instead.
pub fn expand_message(message: &Message) -> serde_json::Value {
    let mut text = message.content.clone();
    let mut image_names: Vec<String> = Vec::new();
    let mut images: Vec<String> = Vec::new();
    for reference in &message.attachments {
        match load(Path::new(&reference.path)) {
            Ok(loaded) => match loaded.kind {
                Kind::Image => {
                    image_names.push(loaded_name(reference));
                    images.push(loaded.payload);
                }
                Kind::Text => text.push_str(&text_block(&loaded_name(reference), &loaded.payload)),
            },
            Err(err) => text.push_str(&format!(
                "\n\n[Attachment unreadable: {} ({err})]",
                loaded_name(reference)
            )),
        }
    }
    if images.is_empty() {
        serde_json::json!({ "role": message.role.as_str(), "content": text })
    } else {
        let mut content = vec![serde_json::json!({
            "type": "text",
            "text": format!("[Image attachment: {}]\n{text}", image_names.join(", ")),
        })];
        for url in images {
            content.push(serde_json::json!({
                "type": "image_url",
                "image_url": { "url": url },
            }));
        }
        serde_json::json!({ "role": message.role.as_str(), "content": content })
    }
}

fn loaded_name(reference: &AttachmentRef) -> String {
    if reference.name.trim().is_empty() {
        reference.path.clone()
    } else {
        reference.name.clone()
    }
}

/// Inline block for a text attachment (model-readable, and not dependent on code fences, so content containing ``` does not break it).
fn text_block(name: &str, content: &str) -> String {
    format!("\n\n--- Attachment {name} ---\n{content}\n--- End of attachment ---")
}

fn limit_of(kind: Kind, bytes: u64) -> Result<(), String> {
    let limit = match kind {
        Kind::Text => TEXT_LIMIT,
        Kind::Image => IMAGE_LIMIT,
    };
    if bytes > limit {
        return Err(format!(
            "File too large: {} (limit {})",
            human(bytes),
            human(limit)
        ));
    }
    Ok(())
}

fn human(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{} KiB", bytes.div_ceil(1024))
    }
}

fn read_head(path: &Path, max: usize) -> Result<Vec<u8>, String> {
    use std::io::Read as _;
    let mut file = std::fs::File::open(path).map_err(|err| format!("Failed to read: {err}"))?;
    let mut buf = vec![0u8; max];
    let read = file
        .read(&mut buf)
        .map_err(|err| format!("Failed to read: {err}"))?;
    buf.truncate(read);
    Ok(buf)
}

fn classify(path: &Path, head: &[u8]) -> Result<Kind, String> {
    let ext = path
        .extension()
        .map(|ext| ext.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if IMAGE_EXTS.contains(&ext.as_str()) {
        return Ok(Kind::Image);
    }
    if TEXT_EXTS.contains(&ext.as_str()) {
        return Ok(Kind::Text);
    }
    if looks_like_text(head) {
        return Ok(Kind::Text);
    }
    Err(format!(
        "Unsupported file type (v1 accepts text and images only): {}",
        path.display()
    ))
}

/// UTF-8 sniffing: no NUL bytes and decodable (a multibyte sequence truncated only at the end still counts as text).
fn looks_like_text(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return true;
    }
    if bytes.contains(&0) {
        return false;
    }
    match std::str::from_utf8(bytes) {
        Ok(_) => true,
        Err(err) => err.error_len().is_none(),
    }
}

fn mime_of(path: &Path) -> &'static str {
    match path
        .extension()
        .map(|ext| ext.to_string_lossy().to_lowercase())
        .unwrap_or_default()
        .as_str()
    {
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        _ => "image/jpeg",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use moe_core::conversation::Role;
    use std::io::Write as _;

    fn temp_file(name: &str, bytes: &[u8]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("moe-attach-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(bytes).unwrap();
        path
    }

    #[test]
    fn parses_quoted_and_bare_mentions_at_boundaries() {
        let (text, paths) = parse_mentions(
            "Summarize @/tmp/a.md and @\"/tmp/with space.txt\", email x@y.com is not a mention",
        );
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0], PathBuf::from("/tmp/a.md"));
        assert_eq!(paths[1], PathBuf::from("/tmp/with space.txt"));
        assert_eq!(text, "Summarize and , email x@y.com is not a mention");
    }

    #[test]
    fn unclosed_quote_is_not_a_mention() {
        let (text, paths) = parse_mentions("see @\"unclosed");
        assert!(paths.is_empty());
        assert_eq!(text, "see @\"unclosed");
    }

    #[test]
    fn expand_path_handles_home() {
        let home = dirs_home().unwrap_or_default();
        if !home.as_os_str().is_empty() {
            assert_eq!(expand_path("~/x.md"), home.join("x.md"));
        }
        assert_eq!(expand_path("/abs/y.md"), PathBuf::from("/abs/y.md"));
    }

    #[test]
    fn inspect_reads_kind_and_rejects_oversize_and_binary() {
        let text = temp_file("note.md", b"# hello");
        let info = inspect(&text).unwrap();
        assert_eq!(info.kind, "text");
        assert_eq!(info.name, "note.md");
        assert_eq!(info.bytes, 7);

        let tiny_binary = temp_file("blob.bin", &[0x00, 0x01, 0xff, 0x00]);
        assert!(inspect(&tiny_binary).unwrap_err().contains("Unsupported"));

        let big = temp_file("big.txt", &vec![b'a'; (TEXT_LIMIT + 1) as usize]);
        assert!(inspect(&big).unwrap_err().contains("File too large"));

        assert!(
            inspect(Path::new("/definitely/missing.md"))
                .unwrap_err()
                .contains("not found")
        );
    }

    /// `~` expands consistently in both inspect and load (error messages contain the expanded path).
    #[test]
    fn inspect_expands_home() {
        if dirs_home().is_none() {
            return;
        }
        let err = inspect(Path::new("~/definitely-missing-moe.md")).unwrap_err();
        assert!(
            !err.contains('~'),
            "the error message should already contain the expanded path: {err}"
        );
    }

    #[test]
    fn image_load_becomes_data_url() {
        // 1x1 PNG
        let png: &[u8] = &[
            0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48,
            0x44, 0x52,
        ];
        let path = temp_file("pixel.png", png);
        let info = inspect(&path).unwrap();
        assert_eq!(info.kind, "image");
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.kind, Kind::Image);
        assert!(loaded.payload.starts_with("data:image/png;base64,"));
    }

    #[test]
    fn expand_message_inlines_text_and_multimodal_images() {
        let text = temp_file("notes.md", "body content".as_bytes());
        let image = temp_file("shot.png", &[0x89, 0x50, 0x4e, 0x47]);
        let message = Message {
            role: Role::User,
            content: "Summarize".into(),
            attachments: vec![ref_for_path(&text), ref_for_path(&image)],
        };
        let json = expand_message(&message);
        assert_eq!(json["role"], "user");
        let parts = json["content"].as_array().expect("multimodal parts");
        assert_eq!(parts[0]["type"], "text");
        let text_part = parts[0]["text"].as_str().unwrap();
        assert!(text_part.contains("Attachment notes.md"));
        assert!(text_part.contains("body content"));
        assert!(text_part.contains("Image attachment: shot.png"));
        assert_eq!(parts[1]["type"], "image_url");
        assert!(
            parts[1]["image_url"]["url"]
                .as_str()
                .unwrap()
                .starts_with("data:image/png;base64,")
        );
    }

    #[test]
    fn expand_message_keeps_plain_text_when_no_attachments() {
        let message = Message {
            role: Role::User,
            content: "hello".into(),
            attachments: vec![],
        };
        let json = expand_message(&message);
        assert_eq!(json["content"], "hello");
    }

    #[test]
    fn expand_message_reports_unreadable_attachment_without_failing() {
        let message = Message {
            role: Role::User,
            content: "Check".into(),
            attachments: vec![AttachmentRef {
                name: "gone.md".into(),
                path: "/definitely/missing.md".into(),
            }],
        };
        let json = expand_message(&message);
        let text = json["content"].as_str().unwrap();
        assert!(text.contains("Attachment unreadable: gone.md"));
        assert!(text.contains("Check"));
    }
}
