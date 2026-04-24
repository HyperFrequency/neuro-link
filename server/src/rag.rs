//! RAG orchestrator — two complementary retrieval paths:
//!
//! # Path A — Octen-backed (primary, nlr_wiki qdrant, 4096-dim)
//!
//! The canonical neuro-link pipeline: **Octen-Embedding-8B (f16 GGUF) via
//! llama-server :8400 → qdrant `nlr_wiki` → Qwen3-Reranker via llama-server
//! :8401**. All three layers are warm HTTP services (see
//! `pkg/docker/compose.yaml` profile `rag`); no cold-model loading per query.
//!
//! Use this path for anything indexed in 02-KB-main/ (the llm-wiki).
//! Queries embed in ~10-50ms (p95), qdrant search ~5-20ms, rerank ~50-150ms.
//! Target p95 for the full pipeline: <300ms.
//!
//! Entry point: [`octen_search`] — takes a plain-text query, returns a
//! reranked list of hits with qdrant payloads + Qwen3 rerank scores.
//!
//! # Path B — qmd subprocess (secondary, dtw_wiki sqlite, 1024-dim)
//!
//! For deep-tool-wiki content where BM25 keyword dominates (tool API names,
//! specific function references), qmd 0.1.2's sqlite-backed search is a
//! better fit. qmd uses its own `Qwen/Qwen3-Embedding-0.6B` (1024-dim)
//! internally — NOT Octen-8B. Ingest via
//! `scripts/ingest_deep_tool_wiki_into_qmd.sh`. This path is primarily
//! BM25+rerank; dense embeddings live inside qmd's sqlite, separate from
//! neuro-link's qdrant.
//!
//! Entry point: [`QmdClient::search`] — shells out to the `qmd` CLI.
//!
//! # MCP routing recommendation
//!
//! For `mcp__neuro-link-http__nlr_rag_query`:
//! - If the query contains named library symbols (vectorbtpro::*,
//!   nautilus_trader::*, etc.) → Path B (qmd+BM25 excels at symbol lookup)
//! - Otherwise → Path A (Octen-backed dense semantic)
//! - For ambiguous queries, run both + RRF-merge (see `tools/rag.rs`).
//!
//! # Tests
//!
//! Unit tests in `#[cfg(test)]` use injectable stdout-parsing + JSON-body
//! parsing to avoid needing live llama-server/qmd at CI time. Integration
//! tests that hit real services are gated behind `--features docker_tests`.

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use tokio::process::Command;

/// One hit from `qmd search`. Mirrors qmd 0.1.2's stdout schema.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct QmdHit {
    /// {document_id, chunk_index, char_start, char_end}
    pub chunk_ref: ChunkRef,
    /// The chunk's raw text.
    pub text: String,
    /// Fused score (BM25 + vector RRF; 0.1.2 doesn't expose components).
    pub score: f32,
    /// Present only when `--rerank` was passed AND the reranker model is
    /// cache-available at `~/.cache/qmd/models/qwen3-reranker-0.6b-q8_0.gguf`.
    #[serde(default)]
    pub rerank_score: Option<f32>,
    /// Caller-set metadata from `document add --metadata-json '...'`.
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ChunkRef {
    pub document_id: String,
    pub chunk_index: u32,
    pub char_start: u32,
    pub char_end: u32,
}

/// Thin handle that holds the path to the qmd sqlite db. Creates a new
/// subprocess per call; intended for infrequent (MCP tool) invocations,
/// not hot-loop use — see `warm_servers.rs` (TBD) for the HTTP-backed
/// warm-server equivalent once the compose.yaml `rag` profile is wired.
#[derive(Clone, Debug)]
pub struct QmdClient {
    db_path: PathBuf,
    qmd_bin: PathBuf,
}

impl QmdClient {
    /// Create a client given the db path. Resolves `qmd` on PATH.
    pub fn new(db_path: impl AsRef<Path>) -> Result<Self> {
        let qmd_bin = which::which("qmd").or_else(|_| {
            // Fallback: check $HOME/.local/share/neuro-link/qmd-venv/bin/qmd
            // (the location our setup script creates).
            let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
            let fallback = PathBuf::from(home)
                .join(".local/share/neuro-link/qmd-venv/bin/qmd");
            if fallback.is_file() {
                Ok(fallback)
            } else {
                Err(anyhow!("qmd binary not found on PATH or at {:?}", fallback))
            }
        })?;
        Ok(Self {
            db_path: db_path.as_ref().to_path_buf(),
            qmd_bin,
        })
    }

    /// Explicit binary override — useful for tests.
    pub fn with_bin(db_path: impl AsRef<Path>, qmd_bin: impl AsRef<Path>) -> Self {
        Self {
            db_path: db_path.as_ref().to_path_buf(),
            qmd_bin: qmd_bin.as_ref().to_path_buf(),
        }
    }

    /// Run `qmd search` against the configured db.
    ///
    /// Returns hits ordered by qmd's scoring (if `rerank=true`, rerank_score
    /// breaks ties). Empty-collection queries return an empty vec, not an
    /// error.
    pub async fn search(
        &self,
        collection: &str,
        query: &str,
        top_k: usize,
        rerank: bool,
    ) -> Result<Vec<QmdHit>> {
        let mut cmd = Command::new(&self.qmd_bin);
        cmd.arg("--db-path")
            .arg(&self.db_path)
            .arg("search")
            .arg("--collection")
            .arg(collection)
            .arg("--query")
            .arg(query)
            .arg("--top-k")
            .arg(top_k.to_string());
        if rerank {
            cmd.arg("--rerank");
        }
        let out = cmd
            .output()
            .await
            .with_context(|| format!("spawning {} search", self.qmd_bin.display()))?;
        if !out.status.success() {
            return Err(anyhow!(
                "qmd search exit={} stderr={}",
                out.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        parse_qmd_stdout(&out.stdout).with_context(|| "parsing qmd search output")
    }
}

/// qmd's stdout on success is a JSON array. On CUDA warnings / HF
/// authentication nags, qmd writes warnings to stderr — stdout stays
/// pure JSON in the working path. This function is pure so it's
/// trivially testable.
pub(crate) fn parse_qmd_stdout(stdout: &[u8]) -> Result<Vec<QmdHit>> {
    let s = std::str::from_utf8(stdout).context("qmd stdout is not utf-8")?;
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Ok(vec![]);
    }
    let hits: Vec<QmdHit> =
        serde_json::from_str(trimmed).with_context(|| format!("stdout was: {}", trimmed.chars().take(200).collect::<String>()))?;
    Ok(hits)
}

// ============================================================================
// Path A — Octen-backed RAG via warm llama-server :8400 + qdrant + :8401 rerank
// ============================================================================

/// One hit from the Octen-backed RAG pipeline. Separate from `QmdHit` because
/// the payload shape comes from qdrant (caller-controlled), not qmd.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct OctenHit {
    /// Qdrant point id.
    pub id: String,
    /// Qdrant cosine-similarity score (0.0–1.0) against the query vector.
    pub vector_score: f32,
    /// Qwen3-Reranker score (0.0–1.0). Present iff `rerank=true`.
    #[serde(default)]
    pub rerank_score: Option<f32>,
    /// Passthrough of qdrant's payload — typically includes `title`, `path`,
    /// `domain`, `chunk_index`, etc. for nlr_wiki points.
    pub payload: serde_json::Value,
}

/// Full-pipeline query: embed via Octen → qdrant top-K → optional Qwen3 rerank.
///
/// - `embedding_url`: typically `http://localhost:8400/v1/embeddings` (matches
///   the existing `embed.rs` convention; OpenAI-compatible llama-server format).
/// - `embedding_model`: the model name llama-server reports via /v1/models
///   (e.g. `Octen-Embedding-8B.f16`). Used for server-side routing when
///   multiple models are loaded.
/// - `qdrant_url`: `http://localhost:6333` (no trailing slash).
/// - `collection`: `nlr_wiki` for the canonical 02-KB-main index.
/// - `rerank_url`: `http://localhost:8401/reranking` (llama-server reranking
///   mode). When `None`, skip rerank and return qdrant's raw top-K ordering.
pub async fn octen_search(
    http: &reqwest::Client,
    query: &str,
    top_k: usize,
    embedding_url: &str,
    embedding_model: &str,
    qdrant_url: &str,
    collection: &str,
    rerank_url: Option<&str>,
) -> Result<Vec<OctenHit>> {
    // 1. Embed query via llama-server :8400 (Octen-8B, 4096-dim)
    let embed_resp = http
        .post(embedding_url)
        .json(&serde_json::json!({
            "model": embedding_model,
            "input": query,
        }))
        .send()
        .await
        .with_context(|| format!("POST {embedding_url} (query embed)"))?
        .error_for_status()
        .with_context(|| format!("{embedding_url} returned non-2xx"))?;
    let embed_body: serde_json::Value = embed_resp.json().await?;
    let vector = parse_openai_embedding(&embed_body)?;

    // 2. Qdrant top-K (over-fetch if reranking; rerank narrows)
    let fetch_k = if rerank_url.is_some() {
        (top_k * 4).max(20).min(100)
    } else {
        top_k
    };
    let search_url = format!("{qdrant_url}/collections/{collection}/points/search");
    let search_resp = http
        .post(&search_url)
        .json(&serde_json::json!({
            "vector": vector,
            "limit": fetch_k,
            "with_payload": true,
            "with_vector": false,
        }))
        .send()
        .await
        .with_context(|| format!("POST {search_url}"))?
        .error_for_status()
        .with_context(|| format!("qdrant search returned non-2xx"))?;
    let search_body: serde_json::Value = search_resp.json().await?;
    let mut hits = parse_qdrant_search(&search_body)?;

    // 3. Optional rerank via llama-server :8401 (Qwen3-Reranker)
    if let Some(rurl) = rerank_url {
        if !hits.is_empty() {
            let texts: Vec<String> = hits
                .iter()
                .map(|h| payload_to_rerank_text(&h.payload))
                .collect();
            match run_qwen_rerank(http, rurl, query, &texts).await {
                Ok(scores) => {
                    for (h, s) in hits.iter_mut().zip(scores.into_iter()) {
                        h.rerank_score = Some(s);
                    }
                    // Sort by rerank_score desc (None last)
                    hits.sort_by(|a, b| {
                        b.rerank_score
                            .unwrap_or(f32::NEG_INFINITY)
                            .partial_cmp(&a.rerank_score.unwrap_or(f32::NEG_INFINITY))
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                }
                Err(e) => {
                    tracing::warn!(
                        "rag::octen_search rerank failed ({}); returning qdrant-only ordering",
                        e
                    );
                }
            }
        }
    }

    hits.truncate(top_k);
    Ok(hits)
}

/// Parse OpenAI-compatible `{data: [{embedding: [...]}]}` response body.
pub(crate) fn parse_openai_embedding(body: &serde_json::Value) -> Result<Vec<f32>> {
    body["data"][0]["embedding"]
        .as_array()
        .ok_or_else(|| anyhow!("response missing data[0].embedding array"))?
        .iter()
        .map(|v| {
            v.as_f64()
                .map(|f| f as f32)
                .ok_or_else(|| anyhow!("embedding element not a float"))
        })
        .collect()
}

/// Parse qdrant `{result: [{id, score, payload}]}` search body.
pub(crate) fn parse_qdrant_search(body: &serde_json::Value) -> Result<Vec<OctenHit>> {
    let arr = body["result"]
        .as_array()
        .ok_or_else(|| anyhow!("qdrant response missing result[] array"))?;
    let mut out = Vec::with_capacity(arr.len());
    for p in arr {
        let id = match &p["id"] {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Number(n) => n.to_string(),
            other => {
                return Err(anyhow!("qdrant point id unexpected shape: {other}"));
            }
        };
        let vector_score = p["score"]
            .as_f64()
            .map(|f| f as f32)
            .ok_or_else(|| anyhow!("qdrant point missing score"))?;
        let payload = p["payload"].clone();
        out.push(OctenHit {
            id,
            vector_score,
            rerank_score: None,
            payload,
        });
    }
    Ok(out)
}

/// Extract text from a qdrant payload for rerank. Prefers `text` field, then
/// `content`, then falls back to JSON stringification.
fn payload_to_rerank_text(payload: &serde_json::Value) -> String {
    if let Some(s) = payload.get("text").and_then(|v| v.as_str()) {
        return s.to_string();
    }
    if let Some(s) = payload.get("content").and_then(|v| v.as_str()) {
        return s.to_string();
    }
    if let Some(s) = payload.get("title").and_then(|v| v.as_str()) {
        return s.to_string();
    }
    payload.to_string()
}

/// Call llama-server's /reranking endpoint with a query + list of documents.
/// Returns one rerank score per input document, preserving order.
///
/// llama-server reranking API format (as of 2026-04 releases):
/// ```json
/// { "query": "<q>", "documents": ["<d1>", "<d2>"] }
/// → { "results": [{"index": 0, "relevance_score": 0.87}, ...] }
/// ```
pub(crate) async fn run_qwen_rerank(
    http: &reqwest::Client,
    rerank_url: &str,
    query: &str,
    documents: &[String],
) -> Result<Vec<f32>> {
    let resp = http
        .post(rerank_url)
        .json(&serde_json::json!({
            "query": query,
            "documents": documents,
        }))
        .send()
        .await
        .with_context(|| format!("POST {rerank_url}"))?
        .error_for_status()
        .with_context(|| format!("rerank server returned non-2xx"))?;
    let body: serde_json::Value = resp.json().await?;
    parse_rerank_response(&body, documents.len())
}

pub(crate) fn parse_rerank_response(body: &serde_json::Value, n: usize) -> Result<Vec<f32>> {
    let results = body["results"]
        .as_array()
        .ok_or_else(|| anyhow!("rerank response missing results[] array"))?;
    let mut scores = vec![0.0f32; n];
    for r in results {
        let idx = r["index"]
            .as_u64()
            .ok_or_else(|| anyhow!("rerank result missing index"))? as usize;
        let score = r["relevance_score"]
            .as_f64()
            .map(|f| f as f32)
            .ok_or_else(|| anyhow!("rerank result missing relevance_score"))?;
        if idx < n {
            scores[idx] = score;
        }
    }
    Ok(scores)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty() {
        let out = parse_qmd_stdout(b"").unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn parse_realistic() {
        let stdout = br#"[
  {
    "chunk_ref": {"document_id": "vectorbtpro:wiki", "chunk_index": 0, "char_start": 0, "char_end": 200},
    "text": "Portfolio.from_signals defaults to signal-bar close fill timing",
    "score": 0.067,
    "bm25_score": null,
    "vector_score": null,
    "rerank_score": 0.9123,
    "metadata": {"tool": "vectorbtpro", "kind": "wiki"}
  }
]"#;
        let out = parse_qmd_stdout(stdout).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].chunk_ref.document_id, "vectorbtpro:wiki");
        assert_eq!(out[0].score, 0.067);
        assert_eq!(out[0].rerank_score, Some(0.9123));
        let tool = out[0].metadata.get("tool").and_then(|v| v.as_str());
        assert_eq!(tool, Some("vectorbtpro"));
    }

    #[test]
    fn parse_invalid_json_errors_cleanly() {
        let err = parse_qmd_stdout(b"this is not json").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("stdout was:"), "msg was: {msg}");
    }

    // Path A (Octen-backed) tests — pure parsers, no network

    #[test]
    fn parse_openai_embedding_ok() {
        let body = serde_json::json!({
            "object": "list",
            "data": [{
                "object": "embedding",
                "embedding": [0.1, 0.2, 0.3, -0.4],
                "index": 0
            }],
            "model": "Octen-Embedding-8B.f16",
            "usage": {"prompt_tokens": 7, "total_tokens": 7}
        });
        let v = parse_openai_embedding(&body).unwrap();
        assert_eq!(v, vec![0.1f32, 0.2, 0.3, -0.4]);
    }

    #[test]
    fn parse_openai_embedding_missing_data_errors() {
        let body = serde_json::json!({"error": "nope"});
        let err = parse_openai_embedding(&body).unwrap_err();
        assert!(err.to_string().contains("data[0].embedding"));
    }

    #[test]
    fn parse_qdrant_search_ok() {
        let body = serde_json::json!({
            "result": [
                {
                    "id": "abc-123",
                    "score": 0.873,
                    "payload": {
                        "title": "vectorbtpro/wiki.md",
                        "domain": "vectorbtpro",
                        "text": "Portfolio.from_signals fill timing..."
                    }
                },
                {
                    "id": 42,
                    "score": 0.721,
                    "payload": {"title": "pine-script/pitfalls.md"}
                }
            ],
            "status": "ok"
        });
        let hits = parse_qdrant_search(&body).unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].id, "abc-123");
        assert!((hits[0].vector_score - 0.873).abs() < 1e-5);
        assert_eq!(hits[0].rerank_score, None);
        assert_eq!(
            hits[0].payload.get("title").and_then(|v| v.as_str()),
            Some("vectorbtpro/wiki.md")
        );
        // Numeric id case
        assert_eq!(hits[1].id, "42");
    }

    #[test]
    fn parse_rerank_response_ok() {
        let body = serde_json::json!({
            "results": [
                {"index": 0, "relevance_score": 0.88},
                {"index": 2, "relevance_score": 0.42},
                {"index": 1, "relevance_score": 0.31}
            ]
        });
        let scores = parse_rerank_response(&body, 3).unwrap();
        assert!((scores[0] - 0.88).abs() < 1e-5);
        assert!((scores[1] - 0.31).abs() < 1e-5);
        assert!((scores[2] - 0.42).abs() < 1e-5);
    }

    #[test]
    fn payload_to_rerank_text_prefers_text_field() {
        let p = serde_json::json!({
            "text": "the body",
            "content": "alt body",
            "title": "T"
        });
        assert_eq!(payload_to_rerank_text(&p), "the body");

        let p2 = serde_json::json!({"title": "just a title"});
        assert_eq!(payload_to_rerank_text(&p2), "just a title");

        let p3 = serde_json::json!({"foo": "bar"});
        // Falls through to JSON stringification
        assert!(payload_to_rerank_text(&p3).contains("foo"));
    }
}
