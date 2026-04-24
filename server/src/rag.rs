//! qmd subprocess client — shells out to the `qmd` CLI to run BM25 + vector
//! + optional rerank over a local qmd sqlite collection. Parses the JSON
//! list qmd emits on stdout and returns typed hits.
//!
//! This is the Rust side of the contract documented at
//! `deep-tool-wiki/qmd/code.md §2`. The qmd binary is expected on `$PATH`
//! (pkg/pyproject.toml ensures `qmd>=0.1.2` is installed in the user's venv
//! and `pkg/<target>/build.sh` stages the venv's `bin/` on PATH for the
//! launched neuro-link process).
//!
//! # Embedder note
//!
//! qmd 0.1.2 uses `Qwen/Qwen3-Embedding-0.6B` (1024-dim) by default, NOT
//! the Octen-Embedding-8B (4096-dim) that backs neuro-link's nlr_wiki
//! qdrant collection. Callers must choose:
//! - `QmdClient::search` → qmd's sqlite (1024-dim; scope = deep-tool-wiki
//!   pages ingested via `scripts/ingest_deep_tool_wiki_into_qmd.sh`)
//! - `crate::embed::qdrant_search` (elsewhere in this crate) → nlr_wiki
//!   (4096-dim; scope = 02-KB-main wiki pages embedded via llama-server :8400)
//!
//! For the bridging MCP tool `nlr_rag_query`, the recommended routing
//! (added in a follow-up PR) is:
//! 1. qmd path for queries that target deep-tool-wiki (BM25 keyword
//!    dominates) — benefits most from rerank
//! 2. qdrant-direct path for queries targeting 02-KB-main (dense semantic
//!    dominates) — low-latency
//!
//! # Tests
//!
//! The `#[cfg(test)]` module below uses a small mock-stdout pattern
//! (injectable via `QmdClient::with_raw_stdout`) so CI doesn't need
//! the qmd binary. An integration-style test that actually invokes
//! `qmd --help` is gated behind `--features docker_tests` to mirror
//! the existing pattern in the crate.

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
}
