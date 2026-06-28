//! `nlr_math_lookup` — 3-way RRF math search.
//!
//! Ranks `02-KB-main/math/*.md` pages across three signals:
//!   L1: prose embedding similarity (Qdrant `nlr_wiki` collection; falls back to
//!       BM25 over body+title when embedding backend is unavailable)
//!   L2: BM25 over the raw `$$...$$` blocks extracted from the page
//!   L3: BM25 over the `canonical_srepr:` frontmatter field (written by E2 ingest)
//!
//! Results are fused via Reciprocal Rank Fusion (k=60). If `prefer_latex=true`
//! and the query contains a `$$` block, L2's contribution is doubled.
//!
//! Hard failure: empty `query` is rejected (no silent default).

use anyhow::{anyhow, bail, Result};
use regex::Regex;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::bm25;
use crate::embed;

pub fn tool_defs() -> Vec<Value> {
    vec![json!({
        "name": "nlr_math_lookup",
        "description": "3-way RRF math search: prose embed + raw LaTeX BM25 + canonical sympy srepr BM25. Use for advanced math topics indexed via E2 ingest.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "query": {"type": "string"},
                "prefer_latex": {"type": "boolean", "description": "If true and query contains $$...$$, weight raw-LaTeX BM25 2x"},
                "limit": {"type": "integer", "default": 10}
            },
            "required": ["query"]
        }
    })]
}

pub fn call(name: &str, args: &Value, root: &Path) -> Result<String> {
    match name {
        "nlr_math_lookup" => {
            let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
            if query.trim().is_empty() {
                return Err(anyhow!(
                    "nlr_math_lookup: 'query' is required (non-empty string)"
                ));
            }
            let prefer_latex = args
                .get("prefer_latex")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let limit = args
                .get("limit")
                .and_then(|v| v.as_u64())
                .unwrap_or(10) as usize;

            let out = lookup(root, query, prefer_latex, limit)?;
            Ok(serde_json::to_string_pretty(&out)?)
        }
        _ => bail!("Unknown math tool: {name}"),
    }
}

/// One entry per math page, derived from `02-KB-main/math/*.md`.
#[derive(Debug, Clone)]
struct MathDoc {
    /// Relative path from `02-KB-main/` (e.g. `math/manifolds.md`).
    rel_path: String,
    /// Frontmatter title if present, else file stem.
    title: String,
    /// Full prose body (frontmatter stripped).
    body: String,
    /// Concatenated raw `$$...$$` block contents.
    latex: String,
    /// Value of the `canonical_srepr:` frontmatter field (may be empty).
    srepr: String,
}

/// Walk `02-KB-main/math/` and parse every `.md` page into a `MathDoc`.
fn collect_math_docs(root: &Path) -> Vec<MathDoc> {
    let math_dir = root.join("02-KB-main").join("math");
    let mut docs = Vec::new();
    let read_dir = match fs::read_dir(&math_dir) {
        Ok(rd) => rd,
        Err(_) => return docs,
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if !path.extension().is_some_and(|e| e == "md") {
            continue;
        }
        let Ok(raw) = fs::read_to_string(&path) else {
            continue;
        };
        let (frontmatter, body) = split_frontmatter(&raw);
        let fm_map = parse_frontmatter(&frontmatter);
        let title = fm_map
            .get("title")
            .cloned()
            .unwrap_or_else(|| {
                path.file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .replace('-', " ")
            });
        let srepr = fm_map.get("canonical_srepr").cloned().unwrap_or_default();
        let latex = extract_latex_blocks(body);
        let rel_path = format!(
            "math/{}",
            path.file_name().unwrap_or_default().to_string_lossy()
        );
        docs.push(MathDoc {
            rel_path,
            title,
            body: body.to_string(),
            latex,
            srepr,
        });
    }
    docs
}

/// Split a markdown file into (frontmatter, body). Returns empty frontmatter
/// when the file does not start with `---`.
fn split_frontmatter(raw: &str) -> (String, &str) {
    if let Some(rest) = raw.strip_prefix("---\n") {
        if let Some(end) = rest.find("\n---") {
            let fm = &rest[..end];
            let after = &rest[end + 4..];
            let body = after.strip_prefix('\n').unwrap_or(after);
            return (fm.to_string(), body);
        }
    }
    (String::new(), raw)
}

/// Parse a minimal `key: value` frontmatter. Quoted values get unquoted.
fn parse_frontmatter(fm: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in fm.lines() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once(':') {
            let key = k.trim().to_string();
            let mut val = v.trim().to_string();
            if (val.starts_with('"') && val.ends_with('"') && val.len() >= 2)
                || (val.starts_with('\'') && val.ends_with('\'') && val.len() >= 2)
            {
                val = val[1..val.len() - 1].to_string();
            }
            map.insert(key, val);
        }
    }
    map
}

/// Extract the contents of every `$$...$$` block, joined by spaces.
fn extract_latex_blocks(body: &str) -> String {
    let re = Regex::new(r"(?s)\$\$(.*?)\$\$").unwrap();
    let mut pieces = Vec::new();
    for cap in re.captures_iter(body) {
        if let Some(m) = cap.get(1) {
            pieces.push(m.as_str().trim().to_string());
        }
    }
    pieces.join("\n")
}

/// Did the query include any `$$...$$` block?
fn query_has_latex_block(query: &str) -> bool {
    let re = Regex::new(r"(?s)\$\$.*?\$\$").unwrap();
    re.is_match(query)
}

/// Tokenize the same way `bm25::tokenize` does (alphanumeric, >2 chars, lowered).
/// Duplicated here because the crate-level function is private — matches exactly.
fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2)
        .map(|w| w.to_string())
        .collect()
}

/// Build a one-shot, in-memory BM25 index over a parallel slice of field
/// strings, one per document, and return the list of `(doc_index, score)`
/// sorted descending. Uses the same K1/B constants as `crate::bm25`.
fn bm25_rank(fields: &[String], query: &str) -> Vec<(usize, f64)> {
    const K1: f64 = 1.2;
    const B: f64 = 0.75;
    let n_docs = fields.len();
    if n_docs == 0 {
        return Vec::new();
    }

    let mut doc_lens: Vec<usize> = Vec::with_capacity(n_docs);
    let mut tfs: Vec<HashMap<String, usize>> = Vec::with_capacity(n_docs);
    let mut total_tokens = 0usize;

    for field in fields {
        let toks = tokenize(field);
        total_tokens += toks.len();
        doc_lens.push(toks.len());
        let mut tf: HashMap<String, usize> = HashMap::new();
        for t in toks {
            *tf.entry(t).or_default() += 1;
        }
        tfs.push(tf);
    }
    let avg_len = if n_docs > 0 {
        (total_tokens as f64) / (n_docs as f64)
    } else {
        1.0
    };
    let avg_len = if avg_len <= 0.0 { 1.0 } else { avg_len };

    // Document frequency per term.
    let mut df: HashMap<String, usize> = HashMap::new();
    for tf in &tfs {
        for term in tf.keys() {
            *df.entry(term.clone()).or_default() += 1;
        }
    }

    let query_tokens = tokenize(query);
    let mut scores = vec![0.0f64; n_docs];
    let n_f = n_docs as f64;
    for qt in &query_tokens {
        let Some(&df_t) = df.get(qt) else {
            continue;
        };
        let idf = ((n_f - df_t as f64 + 0.5) / (df_t as f64 + 0.5) + 1.0).ln();
        if idf <= 0.0 {
            continue;
        }
        for (doc_id, tf_map) in tfs.iter().enumerate() {
            let tf = *tf_map.get(qt).unwrap_or(&0);
            if tf == 0 {
                continue;
            }
            let dl = doc_lens[doc_id] as f64;
            let tf_norm =
                (tf as f64 * (K1 + 1.0)) / (tf as f64 + K1 * (1.0 - B + B * dl / avg_len));
            scores[doc_id] += idf * tf_norm;
        }
    }

    let mut ranked: Vec<(usize, f64)> = scores
        .into_iter()
        .enumerate()
        .filter(|(_, s)| *s > 0.0)
        .collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    ranked
}

/// Try embedding-backed prose search first. If it returns zero results (no
/// Qdrant, no embedding server, or no hits), fall back to in-memory BM25 over
/// `title + body`. Results are returned as indices into `docs`.
fn prose_rank(docs: &[MathDoc], query: &str) -> Vec<(usize, f64)> {
    // First try Qdrant/embedding route. Qdrant stores paths relative to
    // `02-KB-main/`, so match on `rel_path`.
    let qdrant_url =
        std::env::var("QDRANT_URL").unwrap_or_else(|_| "http://localhost:6333".into());
    let embed_results_opt: Option<Vec<embed::SearchResult>> =
        match tokio::runtime::Handle::try_current() {
            Ok(h) => tokio::task::block_in_place(|| {
                h.block_on(async { embed::search_wiki(query, &qdrant_url, docs.len().max(1) * 2).await })
            })
            .ok(),
            Err(_) => None,
        };

    if let Some(embed_results) = embed_results_opt {
        if !embed_results.is_empty() {
            let mut out = Vec::new();
            for r in embed_results {
                if let Some(idx) = docs.iter().position(|d| d.rel_path == r.path) {
                    out.push((idx, r.score));
                }
            }
            if !out.is_empty() {
                return out;
            }
        }
    }

    // Fallback: BM25 over title + body.
    let fields: Vec<String> = docs
        .iter()
        .map(|d| format!("{}\n{}", d.title, d.body))
        .collect();
    bm25_rank(&fields, query)
}

#[derive(Debug, Clone)]
struct Hit {
    path: String,
    title: String,
    score: f64,
    hit_prose: bool,
    hit_latex: bool,
    hit_srepr: bool,
}

/// Core lookup used by both `call` and the unit tests.
fn lookup(root: &Path, query: &str, prefer_latex: bool, limit: usize) -> Result<Vec<Value>> {
    let docs = collect_math_docs(root);
    if docs.is_empty() {
        return Ok(Vec::new());
    }

    let l1 = prose_rank(&docs, query);
    let latex_fields: Vec<String> = docs.iter().map(|d| d.latex.clone()).collect();
    let l2 = bm25_rank(&latex_fields, query);
    let srepr_fields: Vec<String> = docs.iter().map(|d| d.srepr.clone()).collect();
    let l3 = bm25_rank(&srepr_fields, query);

    let latex_weight = if prefer_latex && query_has_latex_block(query) {
        2.0
    } else {
        1.0
    };
    const K: f64 = 60.0;

    let mut fused: HashMap<usize, Hit> = HashMap::new();
    let mut touch = |doc_idx: usize, contribution: f64, lane: &str| {
        let doc = &docs[doc_idx];
        let entry = fused.entry(doc_idx).or_insert_with(|| Hit {
            path: doc.rel_path.clone(),
            title: doc.title.clone(),
            score: 0.0,
            hit_prose: false,
            hit_latex: false,
            hit_srepr: false,
        });
        entry.score += contribution;
        match lane {
            "prose" => entry.hit_prose = true,
            "latex" => entry.hit_latex = true,
            "srepr" => entry.hit_srepr = true,
            _ => {}
        }
    };

    for (rank, (doc_idx, _)) in l1.iter().enumerate() {
        touch(*doc_idx, 1.0 / (K + rank as f64 + 1.0), "prose");
    }
    for (rank, (doc_idx, _)) in l2.iter().enumerate() {
        touch(
            *doc_idx,
            latex_weight / (K + rank as f64 + 1.0),
            "latex",
        );
    }
    for (rank, (doc_idx, _)) in l3.iter().enumerate() {
        touch(*doc_idx, 1.0 / (K + rank as f64 + 1.0), "srepr");
    }

    let mut hits: Vec<Hit> = fused.into_values().collect();
    hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    hits.truncate(limit);

    let out: Vec<Value> = hits
        .into_iter()
        .map(|h| {
            let reason = match (h.hit_prose, h.hit_latex, h.hit_srepr) {
                (true, false, false) => "prose",
                (false, true, false) => "latex",
                (false, false, true) => "srepr",
                _ => "multi",
            };
            json!({
                "path": h.path,
                "title": h.title,
                "score": h.score,
                "reason": reason,
            })
        })
        .collect();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_math_root() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("02-KB-main").join("math")).unwrap();
        tmp
    }

    fn write_math_page(
        root: &Path,
        slug: &str,
        title: &str,
        canonical_srepr: &str,
        body: &str,
    ) {
        let page = format!(
            "---\ntitle: \"{title}\"\ncanonical_srepr: \"{canonical_srepr}\"\n---\n{body}\n"
        );
        fs::write(
            root.join("02-KB-main").join("math").join(format!("{slug}.md")),
            page,
        )
        .unwrap();
    }

    #[test]
    fn lookup_rejects_empty_query() {
        let tmp = setup_math_root();
        let err = call(
            "nlr_math_lookup",
            &json!({"query": ""}),
            tmp.path(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("'query' is required"), "got: {err}");

        let err2 = call(
            "nlr_math_lookup",
            &json!({"query": "   "}),
            tmp.path(),
        )
        .unwrap_err();
        assert!(err2.to_string().contains("'query' is required"));
    }

    #[test]
    fn lookup_returns_empty_when_no_math_pages() {
        let tmp = setup_math_root();
        let out = call(
            "nlr_math_lookup",
            &json!({"query": "riemann manifold"}),
            tmp.path(),
        )
        .unwrap();
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed.as_array().unwrap().len(), 0);
    }

    #[test]
    fn lookup_surfaces_latex_match_when_prefer_latex_true() {
        let tmp = setup_math_root();
        let root = tmp.path();

        // Page A: prose mentions "quadratic" heavily — will win the prose lane.
        // LaTeX block is trivial/unrelated and srepr is unrelated.
        write_math_page(
            root,
            "prose-winner",
            "Quadratic Forms Survey",
            "Pow(Symbol('qqq'), Integer(3))",
            "A discussion of quadratic quadratic quadratic forms without the specific identity.\n$$trivial$$",
        );
        // Page B: LaTeX block contains the exact "xsquared" token we will search for.
        // Prose body has zero mentions of quadratic/xsquared outside the block.
        write_math_page(
            root,
            "latex-winner",
            "Key Identity",
            "Pow(Symbol('aaa'), Integer(7))",
            "The display below is what matters.\n$$xsquared xsquared xsquared$$\nEnd.",
        );
        // Page C: srepr-only match on a distinct token.
        write_math_page(
            root,
            "srepr-winner",
            "Other Note",
            "Pow(Symbol('sreprtoken'), Integer(2))",
            "Irrelevant body.\n$$boring$$",
        );

        // Query contains both a LaTeX block (so prefer_latex doubling kicks in)
        // AND a prose word + srepr word. Without prefer_latex, tied lanes would
        // break ties arbitrarily; with prefer_latex, the latex lane's contribution
        // is doubled and must win the top slot.
        let query = "$$xsquared$$ quadratic sreprtoken";
        let results = lookup(root, query, true, 10).unwrap();
        assert!(!results.is_empty(), "expected at least one result; got {results:?}");
        let top = &results[0];
        assert_eq!(
            top["path"].as_str().unwrap(),
            "math/latex-winner.md",
            "prefer_latex must surface the LaTeX-matching page first; got {results:?}"
        );
    }

    #[test]
    fn lookup_rrf_fuses_three_lists() {
        let tmp = setup_math_root();
        let root = tmp.path();

        // Query: "manifold curvature Integer(42)" — one page matches each lane only.
        // Page A: prose-only on "manifold curvature". No LaTeX blocks. Unrelated srepr.
        write_math_page(
            root,
            "prose-alpha",
            "Alpha",
            "Pow(Symbol('alpha'), Integer(1))",
            "A long discussion of manifold curvature and differential geometry topics.\nNo display math here.",
        );
        // Page B: LaTeX-only — block contains "manifold curvature".
        write_math_page(
            root,
            "latex-beta",
            "Beta",
            "Pow(Symbol('beta'), Integer(2))",
            "See the form below.\n$$manifold curvature tensor$$\nEnd of note.",
        );
        // Page C: srepr-only — frontmatter contains "Integer(42)".
        write_math_page(
            root,
            "srepr-gamma",
            "Gamma",
            "Integer(42)",
            "Just a short unrelated essay about numbers.",
        );

        let results = lookup(root, "manifold curvature Integer(42)", false, 10).unwrap();
        assert!(
            results.len() >= 3,
            "expected all three pages in results, got {results:?}"
        );
        let paths: Vec<&str> = results
            .iter()
            .map(|r| r["path"].as_str().unwrap())
            .collect();
        assert!(paths.contains(&"math/prose-alpha.md"), "paths: {paths:?}");
        assert!(paths.contains(&"math/latex-beta.md"), "paths: {paths:?}");
        assert!(paths.contains(&"math/srepr-gamma.md"), "paths: {paths:?}");
    }
}
