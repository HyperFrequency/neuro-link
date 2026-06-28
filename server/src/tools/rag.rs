use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

use crate::bm25;
use crate::embed;
use crate::tools::external;

pub fn tool_defs() -> Vec<Value> {
    vec![
        json!({"name":"nlr_rag_query","description":"Hybrid search the knowledge base (BM25 + vector via RRF merge). Falls back to BM25-only if Qdrant is unavailable.","inputSchema":{"type":"object","properties":{"query":{"type":"string"},"limit":{"type":"integer"}},"required":["query"]}}),
        json!({"name":"nlr_rag_rebuild_index","description":"Rebuild the keyword index AND BM25 index from wiki pages","inputSchema":{"type":"object","properties":{}}}),
        json!({"name":"nlr_rag_embed","description":"Embed wiki pages into Qdrant for vector search. Set recreate=true to rebuild the collection.","inputSchema":{"type":"object","properties":{"recreate":{"type":"boolean"}}}}),
    ]
}

pub fn call(name: &str, args: &Value, root: &Path) -> Result<String> {
    match name {
        "nlr_rag_query" => {
            let query = args["query"].as_str().unwrap_or("").to_string();
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(5) as usize;

            // 1) BM25 search
            let bm25_results = match bm25::load_index(root) {
                Some(index) => bm25::search(&index, &query, limit * 2),
                None => {
                    // Build on-the-fly if no index exists
                    let index = bm25::build_index(root);
                    let _ = bm25::save_index(&index, root);
                    bm25::search(&index, &query, limit * 2)
                }
            };

            // 2) Vector search via Qdrant (graceful fallback)
            let qdrant_url = std::env::var("QDRANT_URL")
                .unwrap_or_else(|_| "http://localhost:6333".into());

            let vector_results = tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    embed::search_wiki(&query, &qdrant_url, limit * 2).await
                })
            })
            .ok()
            .unwrap_or_default();

            // 3) RRF merge of local sources (BM25 + vector)
            let local_merged = rrf_merge(&bm25_results, &vector_results, limit * 2);

            // 4) Optional external-doc enrichment: only if the flag is on AND
            //    local confidence is low (top local RRF score < 0.6). The
            //    threshold matches the spec; keep in sync with E1 docs.
            let top_local_score = local_merged.first().map(|(_, s, _)| *s).unwrap_or(0.0);
            let want_external = top_local_score < 0.6;
            let external_hits: Vec<external::ExternalHit> = if want_external {
                tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async {
                        external::query(root, &query, limit * 2).await.unwrap_or_default()
                    })
                })
            } else {
                Vec::new()
            };

            // Split external hits back into per-source lists so each feeds
            // RRF as its own ranked input (3rd + 4th lists, k=60).
            let (c7_hits, auggie_hits): (Vec<_>, Vec<_>) = external_hits
                .iter()
                .cloned()
                .partition(|h| h.source == "context7");

            let merged = rrf_merge_with_external(
                &bm25_results,
                &vector_results,
                &c7_hits,
                &auggie_hits,
                limit,
            );

            // Response header stays "hybrid-rrf" whenever vector OR external
            // contributed — external-only-on-top shouldn't degrade the label.
            let source = if vector_results.is_empty() && external_hits.is_empty() {
                "bm25-only"
            } else {
                "hybrid-rrf"
            };

            let out: Vec<Value> = merged
                .into_iter()
                .map(|(path, score, preview)| {
                    json!({"path": path, "score": score, "preview": preview, "source": source})
                })
                .collect();

            Ok(serde_json::to_string_pretty(&out)?)
        }

        "nlr_rag_rebuild_index" => {
            // Rebuild keyword index (existing behavior)
            let kb = root.join("02-KB-main");
            let skip = ["schema.md", "index.md", "log.md"];
            let mut keywords: HashMap<String, Vec<String>> = HashMap::new();
            let mut pages: HashMap<String, Value> = HashMap::new();
            for entry in WalkDir::new(&kb).into_iter().filter_map(|e| e.ok()) {
                let path = entry.path();
                if !path.extension().is_some_and(|e| e == "md") || skip.iter().any(|s| path.file_name().is_some_and(|f| f == *s)) { continue; }
                let content = fs::read_to_string(path).unwrap_or_default();
                let rel = path.strip_prefix(root).unwrap_or(path).display().to_string();
                let stem = path.file_stem().unwrap_or_default().to_string_lossy().replace('-', " ");
                let overview: String = content.lines().filter(|l| !l.starts_with("---") && !l.starts_with('#') && !l.is_empty()).take(3).collect::<Vec<_>>().join(" ");
                for word in stem.split_whitespace().chain(content.split_whitespace().take(100)) {
                    let w = word.to_lowercase().trim_matches(|c: char| !c.is_alphanumeric()).to_string();
                    if w.len() > 3 { keywords.entry(w).or_default().push(rel.clone()); }
                }
                pages.insert(rel, json!({"title": stem, "overview": &overview[..overview.len().min(200)]}));
            }
            let index = json!({"keywords": keywords, "pages": pages});
            fs::write(root.join("state/auto-rag-index.json"), serde_json::to_string_pretty(&index)?)?;

            // Rebuild BM25 index
            let bm25_index = bm25::build_index(root);
            bm25::save_index(&bm25_index, root)?;

            Ok(format!(
                "Index rebuilt: {} pages, {} keywords, BM25 index ({} docs, {} terms)",
                pages.len(),
                keywords.len(),
                bm25_index.doc_count,
                bm25_index.postings.len()
            ))
        }

        "nlr_rag_embed" => {
            let recreate = args.get("recreate").and_then(|v| v.as_bool()).unwrap_or(false);
            let qdrant_url = std::env::var("QDRANT_URL")
                .unwrap_or_else(|_| "http://localhost:6333".into());

            let count = tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    embed::embed_wiki(root, &qdrant_url, recreate).await
                })
            })?;

            Ok(format!("Embedded {count} pages into Qdrant (recreate={recreate})"))
        }

        _ => bail!("Unknown rag tool: {name}"),
    }
}

/// Reciprocal Rank Fusion: merges two ranked lists by path.
/// RRF score = sum over lists of 1/(k + rank), where k=60 (standard constant).
fn rrf_merge(
    bm25: &[bm25::SearchResult],
    vector: &[embed::SearchResult],
    limit: usize,
) -> Vec<(String, f64, String)> {
    const K: f64 = 60.0;
    let mut scores: HashMap<String, (f64, String)> = HashMap::new();

    for (rank, r) in bm25.iter().enumerate() {
        let entry = scores.entry(r.path.clone()).or_insert((0.0, r.preview.clone()));
        entry.0 += 1.0 / (K + rank as f64 + 1.0);
    }

    for (rank, r) in vector.iter().enumerate() {
        let entry = scores.entry(r.path.clone()).or_insert((0.0, r.preview.clone()));
        entry.0 += 1.0 / (K + rank as f64 + 1.0);
    }

    let mut results: Vec<_> = scores
        .into_iter()
        .map(|(path, (score, preview))| (path, score, preview))
        .collect();

    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(limit);
    results
}

/// 4-way RRF: BM25 + vector + Context7 external + Auggie external. Same
/// k=60 constant as the 2-way fusion. Each external list is folded in via
/// `external::rrf_apply`, which keys entries by `"source::url"` so external
/// hits never collide with local KB paths.
fn rrf_merge_with_external(
    bm25: &[bm25::SearchResult],
    vector: &[embed::SearchResult],
    context7: &[external::ExternalHit],
    auggie: &[external::ExternalHit],
    limit: usize,
) -> Vec<(String, f64, String)> {
    const K: f64 = 60.0;
    let mut scores: HashMap<String, (f64, String)> = HashMap::new();

    for (rank, r) in bm25.iter().enumerate() {
        let entry = scores.entry(r.path.clone()).or_insert((0.0, r.preview.clone()));
        entry.0 += 1.0 / (K + rank as f64 + 1.0);
    }
    for (rank, r) in vector.iter().enumerate() {
        let entry = scores.entry(r.path.clone()).or_insert((0.0, r.preview.clone()));
        entry.0 += 1.0 / (K + rank as f64 + 1.0);
    }
    external::rrf_apply(context7, &mut scores);
    external::rrf_apply(auggie, &mut scores);

    let mut results: Vec<_> = scores
        .into_iter()
        .map(|(path, (score, preview))| (path, score, preview))
        .collect();
    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(limit);
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bm25_hit(path: &str) -> bm25::SearchResult {
        bm25::SearchResult {
            path: path.into(),
            score: 1.0,
            preview: format!("preview of {path}"),
        }
    }

    fn vector_hit(path: &str) -> embed::SearchResult {
        embed::SearchResult {
            path: path.into(),
            score: 1.0,
            preview: format!("preview of {path}"),
        }
    }

    fn ext_hit(source: &str, title: &str) -> external::ExternalHit {
        external::ExternalHit {
            source: source.into(),
            title: title.into(),
            snippet: format!("snippet {title}"),
            url: Some(format!("https://example.com/{title}")),
            score: 1.0,
        }
    }

    #[test]
    fn rrf_merge_still_works_without_external() {
        let bm = vec![bm25_hit("a"), bm25_hit("b")];
        let vec = vec![vector_hit("a"), vector_hit("c")];
        let out = rrf_merge(&bm, &vec, 5);
        assert!(!out.is_empty());
        assert_eq!(out[0].0, "a", "doc present in both lists must rank first");
    }

    #[test]
    fn four_way_rrf_includes_external_hits_in_output() {
        let bm = vec![bm25_hit("a")];
        let vec = vec![vector_hit("b")];
        let c7 = vec![ext_hit("context7", "Ext-A")];
        let au = vec![ext_hit("auggie", "Ext-B")];
        let out = rrf_merge_with_external(&bm, &vec, &c7, &au, 10);
        assert!(out.iter().any(|(k, _, _)| k == "context7::https://example.com/Ext-A"));
        assert!(out.iter().any(|(k, _, _)| k == "auggie::https://example.com/Ext-B"));
        assert!(out.iter().any(|(k, _, _)| k == "a"));
        assert!(out.iter().any(|(k, _, _)| k == "b"));
    }

    #[test]
    fn four_way_rrf_with_empty_external_matches_two_way() {
        let bm = vec![bm25_hit("a"), bm25_hit("b")];
        let vec = vec![vector_hit("a"), vector_hit("c")];
        let two = rrf_merge(&bm, &vec, 5);
        let four = rrf_merge_with_external(&bm, &vec, &[], &[], 5);
        assert_eq!(two.len(), four.len(), "same set size");
        // Compare as HashMap keyed by path — RRF ties can reorder equally
        // ranked items; the scores must still match pairwise by path.
        let two_map: HashMap<&str, f64> =
            two.iter().map(|(p, s, _)| (p.as_str(), *s)).collect();
        let four_map: HashMap<&str, f64> =
            four.iter().map(|(p, s, _)| (p.as_str(), *s)).collect();
        assert_eq!(two_map.keys().collect::<Vec<_>>().len(), four_map.keys().collect::<Vec<_>>().len());
        for (path, score_two) in &two_map {
            let score_four = four_map.get(path).copied().unwrap_or(f64::NAN);
            assert!(
                (score_two - score_four).abs() < 1e-9,
                "path {path} score mismatch: {score_two} vs {score_four}"
            );
        }
    }
}
