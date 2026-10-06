//! 附件（ADR-0005 v1：文本文件 + 图片；ADR-0010 `@path` mention）。
//!
//! mention 语法：`@/abs/path`（到空白为止）或 `@"path with spaces"`。
//! 面板的 ⌘⇧A 与侧栏的 📎 只做「校验 + 插入 mention」，读取与展开都在这里。

use std::path::{Path, PathBuf};

use base64::Engine as _;
use moe_core::conversation::{AttachmentRef, Message};

/// 文本附件上限（读入为消息内容）。
pub const TEXT_LIMIT: u64 = 512 * 1024;
/// 图片附件上限（base64 进多模态请求）。
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

/// 只读探测结果：面板/侧栏「校验 + 展示」用，不读入内容。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentInfo {
    pub name: String,
    /// 解析后的绝对路径（`~` 展开）。
    pub path: String,
    /// "text" | "image"
    pub kind: &'static str,
    pub bytes: u64,
}

/// 读入后的附件内容。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    pub kind: Kind,
    /// 文本内容，或图片的 `data:` URL（base64）。
    pub payload: String,
}

/// `~` 展开为 home；其余原样（相对路径按进程 cwd 解析）。
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
    // Windows 上 HOME 常缺省，回退 USERPROFILE
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// 提取 mention：返回（剥离 mention 的可见文本，展开后的路径列表）。
/// `@` 必须落在 token 边界（行首或空白后），`foo@bar.com` 不算。
pub fn parse_mentions(input: &str) -> (String, Vec<PathBuf>) {
    let chars: Vec<char> = input.chars().collect();
    let mut cleaned = String::new();
    let mut paths = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let at_boundary = i == 0 || chars[i - 1].is_whitespace();
        if chars[i] == '@' && at_boundary {
            if chars.get(i + 1) == Some(&'"') {
                // @"带空格 的路径"
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

/// 去掉 mention 留下的多余空格（保留换行结构）。
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

/// 生成落库用的引用（不读文件：文件在请求时现读）。
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

/// 探测文件：存在性、类型（扩展名 + UTF-8 嗅探）、大小上限；`~` 展开与 `load` 一致。
pub fn inspect(raw_path: &Path) -> Result<AttachmentInfo, String> {
    if raw_path.as_os_str().is_empty() {
        return Err("路径为空".into());
    }
    let path = expand_path(&raw_path.to_string_lossy());
    let meta = std::fs::metadata(&path).map_err(|_| format!("文件不存在：{}", path.display()))?;
    if meta.is_dir() {
        return Err(format!("这是目录，不是文件：{}", path.display()));
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

/// 读入附件内容（文本 → 原文；图片 → `data:` URL）。
pub fn load(raw_path: &Path) -> Result<Attachment, String> {
    let path = expand_path(&raw_path.to_string_lossy());
    let meta = std::fs::metadata(&path).map_err(|_| format!("文件不存在：{}", path.display()))?;
    if meta.is_dir() {
        return Err(format!("这是目录，不是文件：{}", path.display()));
    }
    let bytes = std::fs::read(&path).map_err(|err| format!("读取失败：{err}"))?;
    let kind = classify(&path, &bytes)?;
    limit_of(kind, meta.len())?;
    match kind {
        Kind::Text => {
            let text = String::from_utf8(bytes).map_err(|_| "不是有效的 UTF-8 文本".to_string())?;
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

/// 一条消息 → OpenAI messages 项：文本内联，图片进多模态 content 数组。
/// 附件不可读时不失败整次提问，而是在文本里留下可读的提示。
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
                "\n\n[附件不可读：{}（{err}）]",
                loaded_name(reference)
            )),
        }
    }
    if images.is_empty() {
        serde_json::json!({ "role": message.role.as_str(), "content": text })
    } else {
        let mut content = vec![serde_json::json!({
            "type": "text",
            "text": format!("[图片附件：{}]\n{text}", image_names.join("、")),
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

/// 文本附件的内联块（模型可读，且不依赖代码围栏，避免内容里出现 ``` 时破碎）。
fn text_block(name: &str, content: &str) -> String {
    format!("\n\n--- 附件 {name} ---\n{content}\n--- 附件结束 ---")
}

fn limit_of(kind: Kind, bytes: u64) -> Result<(), String> {
    let limit = match kind {
        Kind::Text => TEXT_LIMIT,
        Kind::Image => IMAGE_LIMIT,
    };
    if bytes > limit {
        return Err(format!(
            "文件过大：{}（上限 {}）",
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
    let mut file = std::fs::File::open(path).map_err(|err| format!("读取失败：{err}"))?;
    let mut buf = vec![0u8; max];
    let read = file
        .read(&mut buf)
        .map_err(|err| format!("读取失败：{err}"))?;
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
        "不支持的文件类型（v1 只收文本与图片）：{}",
        path.display()
    ))
}

/// UTF-8 嗅探：无 NUL 且可解码（仅末尾被截断的多字节序列视为文本）。
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
        let (text, paths) =
            parse_mentions("总结一下 @/tmp/a.md 和 @\"/tmp/with space.txt\"，邮箱 x@y.com 不算");
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0], PathBuf::from("/tmp/a.md"));
        assert_eq!(paths[1], PathBuf::from("/tmp/with space.txt"));
        assert_eq!(text, "总结一下 和 ，邮箱 x@y.com 不算");
    }

    #[test]
    fn unclosed_quote_is_not_a_mention() {
        let (text, paths) = parse_mentions("看 @\"没闭合");
        assert!(paths.is_empty());
        assert_eq!(text, "看 @\"没闭合");
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
        assert!(inspect(&tiny_binary).unwrap_err().contains("不支持"));

        let big = temp_file("big.txt", &vec![b'a'; (TEXT_LIMIT + 1) as usize]);
        assert!(inspect(&big).unwrap_err().contains("文件过大"));

        assert!(
            inspect(Path::new("/definitely/missing.md"))
                .unwrap_err()
                .contains("不存在")
        );
    }

    /// `~` 在探测与读取两处一致展开（错误信息里是展开后的路径）。
    #[test]
    fn inspect_expands_home() {
        if dirs_home().is_none() {
            return;
        }
        let err = inspect(Path::new("~/definitely-missing-moe.md")).unwrap_err();
        assert!(!err.contains('~'), "错误信息应已是展开后的路径：{err}");
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
        let text = temp_file("notes.md", "正文内容".as_bytes());
        let image = temp_file("shot.png", &[0x89, 0x50, 0x4e, 0x47]);
        let message = Message {
            role: Role::User,
            content: "总结".into(),
            attachments: vec![ref_for_path(&text), ref_for_path(&image)],
        };
        let json = expand_message(&message);
        assert_eq!(json["role"], "user");
        let parts = json["content"].as_array().expect("multimodal parts");
        assert_eq!(parts[0]["type"], "text");
        let text_part = parts[0]["text"].as_str().unwrap();
        assert!(text_part.contains("附件 notes.md"));
        assert!(text_part.contains("正文内容"));
        assert!(text_part.contains("图片附件：shot.png"));
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
            content: "你好".into(),
            attachments: vec![],
        };
        let json = expand_message(&message);
        assert_eq!(json["content"], "你好");
    }

    #[test]
    fn expand_message_reports_unreadable_attachment_without_failing() {
        let message = Message {
            role: Role::User,
            content: "看看".into(),
            attachments: vec![AttachmentRef {
                name: "gone.md".into(),
                path: "/definitely/missing.md".into(),
            }],
        };
        let json = expand_message(&message);
        let text = json["content"].as_str().unwrap();
        assert!(text.contains("附件不可读：gone.md"));
        assert!(text.contains("看看"));
    }
}
