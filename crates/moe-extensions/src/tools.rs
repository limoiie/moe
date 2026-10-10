//! The v1 read-only Tools (ADR-0039 / MOE-0017): Web Search (self-hosted SearXNG), Web Fetch,
//! Read File. Specs are advertised per request (config-driven); execution is blocking with a 20 s
//! budget, results are truncated to ~8k characters with a note, and cancellation is cooperative —
//! the runner's flag is checked before each network step.

use std::io::Read;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use moe_platform::config::FetchFallback;

use crate::attachment;

/// Per-result character cap; truncation is noted inside the result (MOE-0017).
pub const TOOL_RESULT_CHARS: usize = 8_000;
/// Byte cap for a fetched page (before extraction).
pub const FETCH_BYTE_CAP: u64 = 1024 * 1024;
/// Byte cap for a read file (memory bound; the model sees the 8k-character cap above).
pub const READ_FILE_BYTE_CAP: usize = 256 * 1024;
/// Per tool call (the runner also enforces its own wait budget).
pub const TOOL_TIMEOUT: Duration = Duration::from_secs(20);
/// The keyless reader fallback base.
pub const JINA_READER: &str = "https://r.jina.ai";

/// Search hits folded into one result.
const SEARCH_RESULTS: usize = 8;
/// Below this the local extraction is treated as an empty shell → reader fallback.
const MIN_EXTRACT_CHARS: usize = 200;
/// A browser-like UA: many pages refuse library defaults.
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) moe/0.1 Safari/537.36";

/// Everything a tool needs to run: the configured backends plus the runner's cancel flag.
pub struct ToolContext {
    /// The self-hosted SearXNG base; unset → `web_search` is not advertised.
    pub searxng_url: Option<String>,
    pub fetch_fallback: FetchFallback,
    /// Reader fallback base (tests point this at a mock).
    pub reader_base: String,
    pub cancel: Option<Arc<AtomicBool>>,
}

impl ToolContext {
    pub fn new(
        searxng_url: Option<String>,
        fetch_fallback: FetchFallback,
        cancel: Option<Arc<AtomicBool>>,
    ) -> Self {
        Self {
            searxng_url,
            fetch_fallback,
            reader_base: JINA_READER.to_string(),
            cancel,
        }
    }
}

/// The advertised tool definitions (OpenAI function-calling schema): `web_search` only when a
/// SearXNG URL is configured — with no instance the tool is not in the request at all.
pub fn specs(ctx: &ToolContext) -> Vec<serde_json::Value> {
    let mut tools = Vec::new();
    if ctx
        .searxng_url
        .as_deref()
        .is_some_and(|url| !url.trim().is_empty())
    {
        tools.push(serde_json::json!({
            "type": "function",
            "function": {
                "name": "web_search",
                "description": "Search the web and return the top results as titles, URLs and snippets. Use for current or factual questions.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "The search query." }
                    },
                    "required": ["query"]
                }
            }
        }));
    }
    tools.push(serde_json::json!({
        "type": "function",
        "function": {
            "name": "web_fetch",
            "description": "Fetch one web page and return its readable text as Markdown.",
            "parameters": {
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "The absolute http(s) URL to fetch." }
                },
                "required": ["url"]
            }
        }
    }));
    tools.push(serde_json::json!({
        "type": "function",
        "function": {
            "name": "read_file",
            "description": "Read a local text file from disk (text only; secret paths are blocked).",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "The file path (`~` expands to the home directory)." }
                },
                "required": ["path"]
            }
        }
    }));
    tools
}

/// Execute one tool call; the result (or the error the model should see) is truncated and returned.
pub fn execute(name: &str, arguments: &str, ctx: &ToolContext) -> Result<String, String> {
    let args: serde_json::Value = serde_json::from_str(arguments)
        .map_err(|err| format!("invalid tool arguments (expected a JSON object): {err}"))?;
    let required = |key: &str| -> Result<String, String> {
        args[key]
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .ok_or_else(|| format!("{name} needs a non-empty `{key}` argument"))
    };
    let result = match name {
        "web_search" => web_search(&required("query")?, ctx)?,
        "web_fetch" => web_fetch(&required("url")?, ctx)?,
        "read_file" => read_file(&required("path")?)?,
        other => return Err(format!("unknown tool: {other}")),
    };
    Ok(truncate_result(&result))
}

fn web_search(query: &str, ctx: &ToolContext) -> Result<String, String> {
    let Some(base) = ctx
        .searxng_url
        .as_deref()
        .filter(|url| !url.trim().is_empty())
    else {
        return Err("web_search is not configured (no SearXNG instance)".into());
    };
    check_cancel(ctx)?;
    let url = format!(
        "{}/search?q={}&format=json",
        base.trim_end_matches('/'),
        percent_encode(query)
    );
    let response = http_client()?.get(&url).send().map_err(|err| {
        eprintln!("moe: searxng request failed: {err}");
        format!("search instance unreachable: {err}")
    })?;
    let status = response.status();
    if !status.is_success() {
        eprintln!("moe: searxng returned {status} (is the JSON format enabled in settings.yml?)");
        return Err(format!(
            "search instance returned {status} — enable the JSON format (`formats: [html, json]` in its settings.yml)"
        ));
    }
    let body = response
        .text()
        .map_err(|err| format!("could not read the search response: {err}"))?;
    search_results_text(&body)
}

/// Format a SearXNG JSON response: top hits as compact "n. title / url / snippet" blocks.
fn search_results_text(body: &str) -> Result<String, String> {
    let json: serde_json::Value = serde_json::from_str(body)
        .map_err(|err| format!("search instance returned invalid JSON: {err}"))?;
    let results = json["results"].as_array().cloned().unwrap_or_default();
    if results.is_empty() {
        return Ok("(no results)".to_string());
    }
    let mut out = String::new();
    for (index, result) in results.iter().take(SEARCH_RESULTS).enumerate() {
        let title = result["title"].as_str().unwrap_or_default().trim();
        let url = result["url"].as_str().unwrap_or_default().trim();
        let snippet = result["content"]
            .as_str()
            .unwrap_or_default()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        out.push_str(&format!("{}. {title}\n{url}\n{snippet}\n\n", index + 1));
    }
    Ok(out.trim_end().to_string())
}

fn web_fetch(raw_url: &str, ctx: &ToolContext) -> Result<String, String> {
    let url = raw_url.trim();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("web_fetch needs an absolute http(s) URL".into());
    }
    check_cancel(ctx)?;
    let local = fetch_local(url);
    if let Ok(text) = &local
        && text.trim().chars().count() >= MIN_EXTRACT_CHARS
    {
        return Ok(text.clone());
    }
    match ctx.fetch_fallback {
        FetchFallback::Jina => {
            check_cancel(ctx)?;
            fetch_reader(&ctx.reader_base, url).map_err(|err| match &local {
                Err(local_err) => format!("{local_err}; reader fallback also failed ({err})"),
                Ok(_) => err,
            })
        }
        FetchFallback::None => match local {
            Err(err) => Err(err),
            Ok(_) => Err(
                "the page yielded no readable text (and the reader fallback is disabled)".into(),
            ),
        },
    }
}

/// Local GET → readability extraction → Markdown; the reader fallback fires only when this fails
/// or the extraction is an empty shell (JS-only pages).
fn fetch_local(url: &str) -> Result<String, String> {
    let response = http_client()?
        .get(url)
        .send()
        .map_err(|err| format!("could not fetch {url}: {err}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("the page returned {status}"));
    }
    let mut bytes = Vec::new();
    response
        .take(FETCH_BYTE_CAP)
        .read_to_end(&mut bytes)
        .map_err(|err| format!("could not read the page: {err}"))?;
    extract_markdown(&String::from_utf8_lossy(&bytes), url)
}

/// Readability extraction + Markdown conversion (split out so the chain is unit-testable); the
/// page URL resolves relative links so the model sees absolute ones.
fn extract_markdown(html: &str, page_url: &str) -> Result<String, String> {
    let base = url::Url::parse(page_url)
        .unwrap_or_else(|_| url::Url::parse("https://extract.invalid/").expect("static url"));
    let product = readability::extractor::extract(&mut html.as_bytes(), &base)
        .map_err(|err| format!("could not extract the page content: {err}"))?;
    htmd::convert(&product.content)
        .map_err(|err| format!("could not convert the page content to Markdown: {err}"))
}

/// The keyless reader fallback: `{base}/{url}` returns readable Markdown directly.
fn fetch_reader(base: &str, url: &str) -> Result<String, String> {
    let target = format!("{}/{}", base.trim_end_matches('/'), url);
    let response = http_client()?
        .get(&target)
        .send()
        .map_err(|err| format!("reader fallback could not fetch the page: {err}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("reader fallback returned {status}"));
    }
    let mut bytes = Vec::new();
    response
        .take(FETCH_BYTE_CAP)
        .read_to_end(&mut bytes)
        .map_err(|err| format!("reader fallback could not read the page: {err}"))?;
    Ok(String::from_utf8_lossy(&bytes).to_string())
}

fn read_file(raw_path: &str) -> Result<String, String> {
    let path = attachment::expand_path(raw_path);
    let canonical =
        std::fs::canonicalize(&path).map_err(|_| format!("file not found: {}", path.display()))?;
    if blocked_path(&canonical) {
        return Err(format!(
            "reading {} is blocked by the secrets policy",
            canonical.display()
        ));
    }
    let metadata = std::fs::metadata(&canonical)
        .map_err(|err| format!("cannot stat {}: {err}", canonical.display()))?;
    if metadata.is_dir() {
        return Err("read_file reads files, not directories".into());
    }
    let file = std::fs::File::open(&canonical)
        .map_err(|err| format!("cannot read {}: {err}", canonical.display()))?;
    let mut bytes = Vec::new();
    file.take((READ_FILE_BYTE_CAP + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|err| format!("cannot read {}: {err}", canonical.display()))?;
    bytes.truncate(READ_FILE_BYTE_CAP);
    if bytes.iter().take(8 * 1024).any(|byte| *byte == 0) {
        return Err("binary file — only text files are supported".into());
    }
    Ok(String::from_utf8_lossy(&bytes).to_string())
}

/// The v1 secrets blocklist (MOE-0017): `~/.ssh`, `~/.gnupg`, `~/.aws`, `~/Library/Keychains`,
/// and `.env*` files anywhere. Evaluated on the canonical path.
fn blocked_path(path: &Path) -> bool {
    let components: Vec<&str> = path
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .collect();
    if components
        .iter()
        .any(|component| matches!(*component, ".ssh" | ".gnupg" | ".aws"))
    {
        return true;
    }
    if components
        .windows(2)
        .any(|pair| pair == ["Library", "Keychains"])
    {
        return true;
    }
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with(".env"))
}

fn truncate_result(text: &str) -> String {
    if text.chars().count() <= TOOL_RESULT_CHARS {
        return text.to_string();
    }
    let mut out: String = text.chars().take(TOOL_RESULT_CHARS).collect();
    out.push_str(&format!(
        "\n\n(result truncated at {TOOL_RESULT_CHARS} characters)"
    ));
    out
}

/// Percent-encode a query parameter (RFC 3986 unreserved set stays; everything else becomes `%XX`).
fn percent_encode(input: &str) -> String {
    let mut out = String::new();
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn check_cancel(ctx: &ToolContext) -> Result<(), String> {
    if ctx
        .cancel
        .as_ref()
        .is_some_and(|flag| flag.load(Ordering::Relaxed))
    {
        return Err("stopped by the user".into());
    }
    Ok(())
}

fn http_client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(TOOL_TIMEOUT)
        .user_agent(USER_AGENT)
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|err| format!("could not initialize the HTTP client: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain_ctx() -> ToolContext {
        ToolContext::new(None, FetchFallback::None, None)
    }

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("moe-tools-test-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// One HTTP response over one connection.
    fn serve_once(status: &str, body: &str) -> (String, std::thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let handle = std::thread::spawn(move || {
            use std::io::{Read, Write};
            let Ok((mut socket, _)) = listener.accept() else {
                return;
            };
            let mut buf = [0u8; 4096];
            let _ = socket.read(&mut buf);
            let _ = socket.write_all(response.as_bytes());
            let _ = socket.flush();
        });
        (format!("http://{addr}"), handle)
    }

    #[test]
    fn specs_follow_the_search_configuration() {
        let bare = specs(&plain_ctx());
        let names: Vec<&str> = bare
            .iter()
            .map(|tool| tool["function"]["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            ["web_fetch", "read_file"],
            "no searxng url → no web_search"
        );

        let with_search = specs(&ToolContext::new(
            Some("http://localhost:8888".into()),
            FetchFallback::Jina,
            None,
        ));
        assert_eq!(
            with_search[0]["function"]["name"], "web_search",
            "a configured instance advertises the tool"
        );
        assert_eq!(
            with_search[0]["function"]["parameters"]["required"][0],
            "query"
        );
    }

    #[test]
    fn percent_encoding_covers_reserved_and_unicode() {
        assert_eq!(percent_encode("rust lang"), "rust%20lang");
        assert_eq!(percent_encode("a/b?c=d&e"), "a%2Fb%3Fc%3Dd%26e");
        assert_eq!(percent_encode("中文"), "%E4%B8%AD%E6%96%87");
        assert_eq!(percent_encode("safe-._~"), "safe-._~");
    }

    #[test]
    fn result_truncation_notes_the_cap() {
        let short = "ok".to_string();
        assert_eq!(truncate_result(&short), "ok");
        let long = "x".repeat(TOOL_RESULT_CHARS + 500);
        let truncated = truncate_result(&long);
        assert!(truncated.starts_with(&"x".repeat(100)));
        assert!(truncated.contains("(result truncated at 8000 characters)"));
        assert_eq!(
            truncated
                .trim_end_matches("(result truncated at 8000 characters)")
                .trim_end()
                .chars()
                .count(),
            TOOL_RESULT_CHARS
        );
    }

    #[test]
    fn search_results_text_formats_top_hits() {
        let body = r#"{"results":[
            {"title":"First","url":"https://a.example","content":"alpha   beta"},
            {"title":"Second","url":"https://b.example","content":"gamma"}
        ]}"#;
        let text = search_results_text(body).unwrap();
        assert!(text.starts_with("1. First\nhttps://a.example\nalpha beta"));
        assert!(text.contains("2. Second"));
        assert_eq!(
            search_results_text(r#"{"results":[]}"#).unwrap(),
            "(no results)"
        );
        assert!(search_results_text("not json").is_err());
    }

    #[test]
    fn read_file_reads_text_and_refuses_binary_and_blocked() {
        let dir = temp_dir("read");
        let ok = dir.join("ok.txt");
        std::fs::write(&ok, "hello tool").unwrap();
        let text = execute(
            "read_file",
            &format!("{{\"path\":\"{}\"}}", ok.display()),
            &plain_ctx(),
        )
        .unwrap();
        assert_eq!(text, "hello tool");

        let bin = dir.join("bin.dat");
        std::fs::write(&bin, [b'a', 0, b'b']).unwrap();
        assert!(
            execute(
                "read_file",
                &format!("{{\"path\":\"{}\"}}", bin.display()),
                &plain_ctx()
            )
            .unwrap_err()
            .contains("binary")
        );

        let env = dir.join(".env");
        std::fs::write(&env, "SECRET=1").unwrap();
        assert!(
            execute(
                "read_file",
                &format!("{{\"path\":\"{}\"}}", env.display()),
                &plain_ctx()
            )
            .unwrap_err()
            .contains("secrets policy")
        );

        let ssh = dir.join(".ssh");
        std::fs::create_dir_all(&ssh).unwrap();
        let key = ssh.join("id_ed25519");
        std::fs::write(&key, "key").unwrap();
        assert!(
            execute(
                "read_file",
                &format!("{{\"path\":\"{}\"}}", key.display()),
                &plain_ctx()
            )
            .unwrap_err()
            .contains("secrets policy")
        );

        // Missing required argument and unknown tools are honest errors
        assert!(execute("read_file", "{}", &plain_ctx()).is_err());
        assert!(execute("run_command", "{}", &plain_ctx()).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn web_search_queries_a_mock_instance() {
        let body = r#"{"results":[{"title":"First result","url":"https://a.example","content":"snippet"}]}"#;
        let (base, server) = serve_once("200 OK", body);
        let ctx = ToolContext::new(Some(base), FetchFallback::None, None);
        let text = execute("web_search", "{\"query\":\"rust lang\"}", &ctx).unwrap();
        assert!(text.contains("1. First result"));
        server.join().unwrap();
    }

    #[test]
    fn web_search_reports_a_403_with_the_json_hint() {
        let (base, server) = serve_once("403 Forbidden", "");
        let ctx = ToolContext::new(Some(base), FetchFallback::None, None);
        let err = execute("web_search", "{\"query\":\"x\"}", &ctx).unwrap_err();
        assert!(err.contains("403"), "status is in the error: {err}");
        assert!(err.contains("formats"), "the hint names the fix: {err}");
        server.join().unwrap();
    }

    #[test]
    fn web_fetch_extracts_the_article() {
        let article = "<html><head><title>The Page</title></head><body><article>\
            <h1>The quick brown fox</h1>\
            <p>The quick brown fox jumps over the lazy dog while writing a long enough paragraph \
            for the extractor to see this as the main content of the page, with several words.</p>\
            <p>Another paragraph continues the article so readability has enough material to \
            keep, explaining things at a comfortable length for a test fixture.</p>\
            </article></body></html>";
        let (base, server) = serve_once("200 OK", article);
        let mut ctx = ToolContext::new(None, FetchFallback::Jina, None);
        // If the local extraction failed, the fallback would dial this dead address and the result
        // would not contain the article text — the assertion below catches it.
        ctx.reader_base = "http://127.0.0.1:1".into();
        let text = execute("web_fetch", &format!("{{\"url\":\"{base}/page\"}}"), &ctx).unwrap();
        assert!(
            text.to_ascii_lowercase().contains("quick brown fox"),
            "extracted markdown should contain the article: {text}"
        );
        server.join().unwrap();
    }

    #[test]
    fn web_fetch_falls_back_to_the_reader_on_403() {
        let (base, local) = serve_once("403 Forbidden", "");
        let (reader, reader_server) = serve_once("200 OK", "reader text from the fallback");
        let mut ctx = ToolContext::new(None, FetchFallback::Jina, None);
        ctx.reader_base = reader;
        let text = execute("web_fetch", &format!("{{\"url\":\"{base}/page\"}}"), &ctx).unwrap();
        assert_eq!(text, "reader text from the fallback");
        local.join().unwrap();
        reader_server.join().unwrap();
    }

    #[test]
    fn web_fetch_fails_honestly_when_the_fallback_is_off() {
        let (base, server) = serve_once("403 Forbidden", "");
        let ctx = ToolContext::new(None, FetchFallback::None, None);
        let err = execute("web_fetch", &format!("{{\"url\":\"{base}/page\"}}"), &ctx).unwrap_err();
        assert!(err.contains("403"), "{err}");
        server.join().unwrap();
    }
}
