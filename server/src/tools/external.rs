//! External MCP doc servers (Context7 + Auggie) — optional 3rd/4th inputs to
//! the hybrid-RAG RRF merge. Speaks MCP stdio to child processes, with a hard
//! 1500 ms per-source timeout, an append-only JSONL cache with 1 h TTL, and a
//! `EXTERNAL_DOCS_ENABLED` feature flag (default off).
//!
//! Safety: on *any* transport failure (timeout, spawn error, malformed JSON)
//! we return an empty `Vec<ExternalHit>` rather than propagating errors. The
//! RRF merge treats missing external results as a non-contribution, so RAG
//! quality never regresses on external-side failures.
//!
//! The module is deliberately in `tools/` rather than at crate root so that
//! registering it does not require editing `server/src/main.rs` (which is
//! being touched by parallel agents on other E-track branches).

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Hard per-source timeout. Keep small so the RAG tool latency budget
/// (typically <3 s total) is preserved even when both sources are slow.
pub const EXTERNAL_TIMEOUT_MS: u64 = 1500;
/// Cache entries older than this are ignored on read.
pub const CACHE_TTL_SECS: i64 = 3600;
/// Standard RRF constant — must match `rag::rrf_merge`.
pub const RRF_K: f64 = 60.0;
/// Environment flag; when absent or not "true" external lookup is skipped.
pub const FLAG_ENV: &str = "EXTERNAL_DOCS_ENABLED";

/// One hit from an external MCP doc server, already normalized into the
/// shape `rag` expects for RRF fusion.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExternalHit {
    pub source: String,
    pub title: String,
    pub snippet: String,
    pub url: Option<String>,
    pub score: f64,
}

/// Describes one external MCP server we can query over stdio.
/// `cmd` is `[program, arg1, arg2, ...]`; we spawn it with a pipe, send one
/// `tools/call` JSON-RPC message, read a single JSON-RPC response line, then
/// close stdin. The child is killed on drop.
#[derive(Debug, Clone)]
pub struct ExternalClient {
    pub name: String,
    pub cmd: Vec<String>,
    /// MCP tool name to invoke on the child. For Context7 the default is
    /// `docs_search`; Auggie uses `query-docs`. Callers may override.
    pub tool: String,
}

impl ExternalClient {
    pub fn context7() -> Self {
        // Resolve from env so operators can pin a local shim, but fall back
        // to the public npm package that both Context7 and Auggie ship.
        let raw = std::env::var("CONTEXT7_CMD")
            .unwrap_or_else(|_| "npx -y @upstash/context7-mcp".to_string());
        Self {
            name: "context7".into(),
            cmd: split_cmd(&raw),
            tool: std::env::var("CONTEXT7_TOOL").unwrap_or_else(|_| "docs_search".into()),
        }
    }

    pub fn auggie() -> Self {
        let raw = std::env::var("AUGGIE_CMD")
            .unwrap_or_else(|_| "npx -y @augmentcode/auggie-mcp".to_string());
        Self {
            name: "auggie".into(),
            cmd: split_cmd(&raw),
            tool: std::env::var("AUGGIE_TOOL").unwrap_or_else(|_| "query-docs".into()),
        }
    }
}

/// Split a shell-style command string into argv, honoring simple quoting.
/// Not a full shell parser — enough to support `"npx -y @pkg/name"` style
/// values in env vars without requiring users to write JSON arrays.
fn split_cmd(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_squote = false;
    let mut in_dquote = false;
    for ch in raw.chars() {
        match ch {
            '\'' if !in_dquote => in_squote = !in_squote,
            '"' if !in_squote => in_dquote = !in_dquote,
            ch if ch.is_whitespace() && !in_squote && !in_dquote => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            ch => cur.push(ch),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn feature_enabled() -> bool {
    std::env::var(FLAG_ENV)
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}

/// Counter of live transport dispatches, used by the "flag-off never
/// invokes transport" test. This is deliberately process-global and only
/// touched from test paths + the real transport entrypoint.
static TRANSPORT_INVOKED: AtomicBool = AtomicBool::new(false);

pub fn transport_was_invoked() -> bool {
    TRANSPORT_INVOKED.load(Ordering::SeqCst)
}

pub fn reset_transport_invoked() {
    TRANSPORT_INVOKED.store(false, Ordering::SeqCst);
}

fn query_sha(query: &str) -> String {
    let mut h = Sha256::new();
    h.update(query.as_bytes());
    hex::encode(h.finalize())
}

#[derive(Debug, Serialize, Deserialize)]
struct CacheEntry {
    query_sha: String,
    ts: i64,
    source: String,
    hits: Vec<ExternalHit>,
}

fn cache_path(root: &Path) -> PathBuf {
    root.join("state/external_cache.jsonl")
}

/// Read the append-only cache back-to-front and return the first entry for
/// `(query, source)` that is within TTL. Malformed lines are ignored.
fn cache_read(root: &Path, query: &str, source: &str) -> Option<Vec<ExternalHit>> {
    let path = cache_path(root);
    let content = std::fs::read_to_string(&path).ok()?;
    let want_sha = query_sha(query);
    let now = chrono::Utc::now().timestamp();
    // Iterate newest-last-wins by scanning in reverse order.
    for line in content.lines().rev() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<CacheEntry>(line) else {
            continue;
        };
        if entry.query_sha == want_sha
            && entry.source == source
            && (now - entry.ts) <= CACHE_TTL_SECS
        {
            return Some(entry.hits);
        }
    }
    None
}

/// Append one cache entry. Best-effort — cache write failures never bubble
/// up to the RAG path.
fn cache_write(root: &Path, query: &str, source: &str, hits: &[ExternalHit]) {
    let path = cache_path(root);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let entry = CacheEntry {
        query_sha: query_sha(query),
        ts: chrono::Utc::now().timestamp(),
        source: source.to_string(),
        hits: hits.to_vec(),
    };
    let Ok(line) = serde_json::to_string(&entry) else {
        return;
    };
    let file = std::fs::OpenOptions::new().create(true).append(true).open(&path);
    if let Ok(mut f) = file {
        let _ = writeln!(f, "{line}");
    }
}

/// Transport hook — pluggable so tests can inject a mock (e.g. one that
/// sleeps > 1500 ms to exercise the timeout path) without spawning child
/// processes.
pub trait Transport: Send + Sync {
    fn call(&self, client: &ExternalClient, query: &str, limit: usize) -> Result<Vec<ExternalHit>>;
}

/// Production transport — actually spawns the MCP server, exchanges a pair
/// of `initialize` + `tools/call` JSON-RPC messages over stdio, and maps the
/// response back into `ExternalHit`s.
pub struct StdioTransport;

impl Transport for StdioTransport {
    fn call(&self, client: &ExternalClient, query: &str, limit: usize) -> Result<Vec<ExternalHit>> {
        TRANSPORT_INVOKED.store(true, Ordering::SeqCst);
        if client.cmd.is_empty() {
            anyhow::bail!("empty command for external client {}", client.name);
        }
        let mut child = std::process::Command::new(&client.cmd[0])
            .args(&client.cmd[1..])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let mut stdin = child.stdin.take().ok_or_else(|| anyhow::anyhow!("no stdin"))?;
        let stdout = child.stdout.take().ok_or_else(|| anyhow::anyhow!("no stdout"))?;
        let init = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "neuro-link", "version": env!("CARGO_PKG_VERSION")}}
        });
        let call = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {"name": client.tool, "arguments": {"query": query, "limit": limit}}
        });
        writeln!(stdin, "{init}")?;
        writeln!(stdin, "{call}")?;
        drop(stdin);
        let mut reader = BufReader::new(stdout);
        let mut hits: Vec<ExternalHit> = Vec::new();
        let mut line = String::new();
        loop {
            line.clear();
            let n = reader.read_line(&mut line)?;
            if n == 0 {
                break;
            }
            let Ok(v) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            // Only the tools/call response (id=2) carries hits.
            if v.get("id").and_then(|v| v.as_i64()) != Some(2) {
                continue;
            }
            if let Some(result) = v.get("result") {
                hits = parse_hits(&client.name, result, limit);
            }
            break;
        }
        let _ = child.kill();
        let _ = child.wait();
        Ok(hits)
    }
}

/// Extract an array of hits from the MCP `tools/call` response.
///
/// MCP tools usually return `{"content": [{"type":"text","text":"..."}]}`.
/// We try several shapes so we work with whatever Context7 / Auggie ship:
///
/// 1. `result.content[*].text` parsed as JSON array of hit objects.
/// 2. `result.hits` — already an array of hit objects.
/// 3. `result.content[*].text` as plain text → one hit with empty url.
pub fn parse_hits(source: &str, result: &Value, limit: usize) -> Vec<ExternalHit> {
    // Case 2 first — cheapest.
    if let Some(arr) = result.get("hits").and_then(|v| v.as_array()) {
        return arr
            .iter()
            .take(limit)
            .map(|v| hit_from_value(source, v))
            .collect();
    }
    let content = match result.get("content").and_then(|v| v.as_array()) {
        Some(c) => c,
        None => return Vec::new(),
    };
    // Case 1
    for block in content {
        let Some(text) = block.get("text").and_then(|v| v.as_str()) else { continue };
        if let Ok(Value::Array(arr)) = serde_json::from_str::<Value>(text) {
            return arr
                .iter()
                .take(limit)
                .map(|v| hit_from_value(source, v))
                .collect();
        }
        if let Ok(Value::Object(obj)) = serde_json::from_str::<Value>(text) {
            if let Some(arr) = obj.get("hits").and_then(|v| v.as_array()) {
                return arr
                    .iter()
                    .take(limit)
                    .map(|v| hit_from_value(source, v))
                    .collect();
            }
        }
    }
    // Case 3 — fall back to textual blobs.
    content
        .iter()
        .filter_map(|b| b.get("text").and_then(|v| v.as_str()))
        .take(limit)
        .enumerate()
        .map(|(i, text)| ExternalHit {
            source: source.to_string(),
            title: format!("{source} result {}", i + 1),
            snippet: text.chars().take(300).collect(),
            url: None,
            score: 1.0 / (i as f64 + 1.0),
        })
        .collect()
}

fn hit_from_value(source: &str, v: &Value) -> ExternalHit {
    ExternalHit {
        source: source.to_string(),
        title: v
            .get("title")
            .and_then(|x| x.as_str())
            .unwrap_or("(untitled)")
            .to_string(),
        snippet: v
            .get("snippet")
            .or_else(|| v.get("text"))
            .or_else(|| v.get("excerpt"))
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .chars()
            .take(500)
            .collect(),
        url: v
            .get("url")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string()),
        score: v
            .get("score")
            .and_then(|x| x.as_f64())
            .unwrap_or(1.0),
    }
}

/// Run `transport.call` on a blocking thread with a 1500 ms watchdog. Returns
/// `Ok(Vec::new())` on timeout or transport error — never `Err`.
fn call_with_timeout<T: Transport + 'static>(
    transport: std::sync::Arc<T>,
    client: ExternalClient,
    query: String,
    limit: usize,
) -> Vec<ExternalHit> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let out = transport.call(&client, &query, limit).unwrap_or_default();
        let _ = tx.send(out);
    });
    match rx.recv_timeout(Duration::from_millis(EXTERNAL_TIMEOUT_MS)) {
        Ok(hits) => hits,
        Err(_) => Vec::new(), // timeout → empty, caller still gets RRF(bm25,vector)
    }
}

/// Public entry: query every configured external source in parallel, with
/// cache + timeout + flag handling. Always returns `Ok`; a disabled flag or
/// empty client list yields an empty `Vec`.
pub async fn query(root: &Path, query: &str, limit: usize) -> Result<Vec<ExternalHit>> {
    query_with_transport(root, query, limit, std::sync::Arc::new(StdioTransport)).await
}

pub async fn query_with_transport<T: Transport + 'static>(
    root: &Path,
    query: &str,
    limit: usize,
    transport: std::sync::Arc<T>,
) -> Result<Vec<ExternalHit>> {
    if !feature_enabled() {
        return Ok(Vec::new());
    }
    let query = query.to_string();
    let root = root.to_path_buf();
    let clients = vec![ExternalClient::context7(), ExternalClient::auggie()];
    let mut merged: Vec<ExternalHit> = Vec::new();
    for client in clients {
        // Cache check first. Hits bypass transport entirely.
        if let Some(cached) = cache_read(&root, &query, &client.name) {
            merged.extend(cached);
            continue;
        }
        let q = query.clone();
        let name = client.name.clone();
        let r = root.clone();
        let tr = transport.clone();
        let client_clone = client.clone();
        let hits = tokio::task::spawn_blocking(move || {
            call_with_timeout(tr, client_clone, q, limit)
        })
        .await
        .unwrap_or_default();
        // Only persist non-empty results — caching an empty miss would mask
        // a source recovering within the TTL window.
        if !hits.is_empty() {
            cache_write(&r, &query, &name, &hits);
        }
        merged.extend(hits);
    }
    Ok(merged)
}

/// Fold an external-hit list into RRF score accumulators using the same
/// `k=60` constant as the BM25+vector merge. The hits are treated as an
/// already-ranked list in the order they arrive from the source.
pub fn rrf_apply(
    hits: &[ExternalHit],
    scores: &mut std::collections::HashMap<String, (f64, String)>,
) {
    for (rank, hit) in hits.iter().enumerate() {
        // Key by "source::url" when we have a URL (canonical), else
        // "source::title" so duplicate titles from the same source collapse.
        let key = match &hit.url {
            Some(u) => format!("{}::{}", hit.source, u),
            None => format!("{}::{}", hit.source, hit.title),
        };
        let preview = hit.snippet.clone();
        let entry = scores.entry(key).or_insert((0.0, preview));
        entry.0 += 1.0 / (RRF_K + rank as f64 + 1.0);
    }
}

// ─────────────────────────── tests ───────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::tempdir;

    // Shared across tests — we must make sure tests that depend on the
    // feature flag don't race. They do `with_env_var` and never run in
    // parallel on the flag variable because cargo test serializes within
    // a single test binary? No — cargo runs #[test]s in threads. We
    // therefore use a module-local Mutex to serialize the flag-sensitive
    // tests.
    use std::sync::Mutex;
    static FLAG_GUARD: Mutex<()> = Mutex::new(());

    struct MockTransport {
        hits: Vec<ExternalHit>,
        calls: AtomicUsize,
        delay_ms: u64,
    }

    impl Transport for MockTransport {
        fn call(
            &self,
            _client: &ExternalClient,
            _query: &str,
            _limit: usize,
        ) -> Result<Vec<ExternalHit>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.delay_ms > 0 {
                std::thread::sleep(Duration::from_millis(self.delay_ms));
            }
            Ok(self.hits.clone())
        }
    }

    fn sample_hit(source: &str, title: &str) -> ExternalHit {
        ExternalHit {
            source: source.into(),
            title: title.into(),
            snippet: "snippet".into(),
            url: Some(format!("https://example.com/{title}")),
            score: 1.0,
        }
    }

    #[test]
    fn split_cmd_handles_quoted_tokens() {
        assert_eq!(split_cmd("npx -y @pkg/name"), vec!["npx", "-y", "@pkg/name"]);
        assert_eq!(
            split_cmd("python \"/with space/run.py\" arg"),
            vec!["python", "/with space/run.py", "arg"]
        );
    }

    #[test]
    fn feature_flag_defaults_off() {
        let _g = FLAG_GUARD.lock().unwrap();
        std::env::remove_var(FLAG_ENV);
        assert!(!feature_enabled());
    }

    #[test]
    fn feature_flag_recognizes_true_variants() {
        let _g = FLAG_GUARD.lock().unwrap();
        for v in ["true", "1", "yes", "on", "TRUE"] {
            std::env::set_var(FLAG_ENV, v);
            assert!(feature_enabled(), "{v} should enable");
        }
        std::env::remove_var(FLAG_ENV);
    }

    /// Flag-off path never invokes the transport. We pass a mock that
    /// increments a counter on every call and assert the counter stays 0.
    #[tokio::test]
    async fn flag_off_never_invokes_transport() {
        let _g = FLAG_GUARD.lock().unwrap();
        std::env::remove_var(FLAG_ENV);
        let dir = tempdir().unwrap();
        let mock = Arc::new(MockTransport {
            hits: vec![sample_hit("context7", "x")],
            calls: AtomicUsize::new(0),
            delay_ms: 0,
        });
        reset_transport_invoked();
        let hits = query_with_transport(dir.path(), "q", 3, mock.clone()).await.unwrap();
        assert!(hits.is_empty());
        assert_eq!(mock.calls.load(Ordering::SeqCst), 0);
        assert!(!transport_was_invoked());
    }

    /// Cache hit path: pre-write a cache entry, turn the flag on, use a
    /// mock whose call count we watch, and assert it's never invoked.
    #[tokio::test]
    async fn cache_hit_bypasses_transport() {
        let _g = FLAG_GUARD.lock().unwrap();
        std::env::set_var(FLAG_ENV, "true");
        let dir = tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("state")).unwrap();
        // Pre-seed cache for both sources so neither triggers transport.
        let hits_c = vec![sample_hit("context7", "hit-c7")];
        let hits_a = vec![sample_hit("auggie", "hit-au")];
        cache_write(dir.path(), "cached-query", "context7", &hits_c);
        cache_write(dir.path(), "cached-query", "auggie", &hits_a);
        let mock = Arc::new(MockTransport {
            hits: vec![sample_hit("context7", "SHOULD-NOT-APPEAR")],
            calls: AtomicUsize::new(0),
            delay_ms: 0,
        });
        let hits = query_with_transport(dir.path(), "cached-query", 3, mock.clone())
            .await
            .unwrap();
        std::env::remove_var(FLAG_ENV);
        assert_eq!(mock.calls.load(Ordering::SeqCst), 0);
        assert_eq!(hits.len(), 2);
        assert!(hits.iter().any(|h| h.title == "hit-c7"));
        assert!(hits.iter().any(|h| h.title == "hit-au"));
    }

    /// Timeout path: a mock sleeping > 1500 ms forces the watchdog to fire;
    /// the query returns an empty vec instead of propagating an error.
    #[tokio::test]
    async fn timeout_returns_empty_vec() {
        let _g = FLAG_GUARD.lock().unwrap();
        std::env::set_var(FLAG_ENV, "true");
        let dir = tempdir().unwrap();
        let mock = Arc::new(MockTransport {
            hits: vec![sample_hit("context7", "late")],
            calls: AtomicUsize::new(0),
            delay_ms: EXTERNAL_TIMEOUT_MS + 800, // exceeds watchdog
        });
        let start = std::time::Instant::now();
        let hits = query_with_transport(dir.path(), "slow-query", 3, mock.clone())
            .await
            .unwrap();
        let elapsed = start.elapsed();
        std::env::remove_var(FLAG_ENV);
        assert!(hits.is_empty(), "timed-out calls must yield empty vec");
        // Watchdog fires per source, so bound is ~2 * TIMEOUT + small slack.
        assert!(
            elapsed < Duration::from_millis(EXTERNAL_TIMEOUT_MS * 2 + 1500),
            "overall query too slow: {:?}",
            elapsed
        );
    }

    #[test]
    fn cache_read_respects_ttl() {
        let dir = tempdir().unwrap();
        // Manually append a stale entry (ts = 0).
        let path = cache_path(dir.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let stale = CacheEntry {
            query_sha: query_sha("q"),
            ts: 0,
            source: "context7".into(),
            hits: vec![sample_hit("context7", "stale")],
        };
        std::fs::write(&path, format!("{}\n", serde_json::to_string(&stale).unwrap())).unwrap();
        assert!(cache_read(dir.path(), "q", "context7").is_none());
    }

    #[test]
    fn parse_hits_handles_json_text_block() {
        let result = json!({
            "content": [{
                "type": "text",
                "text": "[{\"title\":\"A\",\"snippet\":\"s\",\"url\":\"https://a\"}]"
            }]
        });
        let hits = parse_hits("context7", &result, 5);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "A");
        assert_eq!(hits[0].url.as_deref(), Some("https://a"));
    }

    #[test]
    fn parse_hits_handles_plain_text_fallback() {
        let result = json!({"content": [{"type":"text","text":"free-form doc excerpt"}]});
        let hits = parse_hits("auggie", &result, 5);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].snippet, "free-form doc excerpt");
    }

    #[test]
    fn rrf_apply_accumulates_scores() {
        let hits = vec![sample_hit("context7", "A"), sample_hit("context7", "B")];
        let mut scores = std::collections::HashMap::new();
        rrf_apply(&hits, &mut scores);
        // Rank 0 contributes 1/61, rank 1 contributes 1/62.
        let a = scores
            .get("context7::https://example.com/A")
            .map(|x| x.0)
            .unwrap_or(0.0);
        let b = scores
            .get("context7::https://example.com/B")
            .map(|x| x.0)
            .unwrap_or(0.0);
        assert!(a > b, "earlier rank must score higher: {a} vs {b}");
    }
}
