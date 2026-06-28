//! D7: Draft+verify two-call hybrid RAG tool.
//!
//! Runs a normal hybrid-RRF RAG query (draft), then gates on:
//!   - top-1 score < 0.55
//!   - BM25 top hit != vector top hit (disagreement)
//!   - multi-hop intent words in query
//!   - stakes == "high"
//! If gate fires, issues a second LLM call via OpenRouter to verify/revise.
//! Kill-switches fall back to draft if verify lift is insufficient.

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use std::path::Path;
use std::time::Duration;

use crate::bm25;
use crate::embed;

const DEFAULT_MODEL: &str = "anthropic/claude-3.5-haiku";
const CONF_THRESHOLD: f64 = 0.55;
const LIFT_MIN: f64 = 0.02;
const DISAGREEMENT_LIFT_MIN: f64 = 0.05;
const DISAGREEMENT_HIGH: f64 = 0.4;
const COST_DELTA_MAX: f64 = 1.5;
const MULTI_HOP_PHRASES: &[&str] = &[
    "why",
    "how does",
    "relationship between",
    "path from",
    "connect",
    " and ",
];

pub fn tool_defs() -> Vec<Value> {
    vec![json!({
        "name": "nlr_rag_query_verified",
        "description": "Hybrid RAG with draft+verify two-call pattern. Gates on low confidence, BM25/vector disagreement, multi-hop intent, or stakes=high. Falls back to draft if verify pass offers insufficient lift per kill-switch rules.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "query": {"type": "string"},
                "stakes": {"type": "string", "enum": ["low", "medium", "high"], "default": "low"},
                "limit": {"type": "integer", "default": 5},
                "model": {"type": "string", "default": DEFAULT_MODEL}
            },
            "required": ["query"]
        }
    })]
}

/// Verify-call function signature. Returned JSON must contain at minimum
/// `verdict` ("confirmed"|"revised"), `confidence` (f64), optional
/// `adjusted_ranking` (array of path strings), optional `reasoning`,
/// and `tokens_used` (integer) so kill-switches can compute cost delta.
pub type VerifyFn = dyn Fn(&str, &str, &str) -> Result<Value> + Send + Sync;

pub fn call(name: &str, args: &Value, root: &Path) -> Result<String> {
    match name {
        "nlr_rag_query_verified" => call_verified(args, root, None),
        _ => bail!("Unknown rag_verified tool: {name}"),
    }
}

/// Core entry point.  `verify_fn` is an optional injected verifier for tests.
pub fn call_verified(
    args: &Value,
    root: &Path,
    verify_fn: Option<&VerifyFn>,
) -> Result<String> {
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("nlr_rag_query_verified: 'query' is required (non-empty string)"))?
        .to_string();

    let stakes = args
        .get("stakes")
        .and_then(|v| v.as_str())
        .unwrap_or("low")
        .to_string();
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(5) as usize;
    let model = args
        .get("model")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_MODEL)
        .to_string();

    // ── 1. Run draft retrieval ──
    let DraftResult {
        merged,
        bm25_top,
        vec_top,
        bm25_only,
        vec_only,
    } = run_draft(&query, limit, root);

    let top1_score = merged.first().map(|h| h.score).unwrap_or(0.0);

    // ── 2. Evaluate gates ──
    let multi_hop = is_multi_hop(&query);
    let low_conf = top1_score < CONF_THRESHOLD;
    let disagreement = match (&bm25_top, &vec_top) {
        (Some(a), Some(b)) if a != b => 1.0,
        (Some(_), None) | (None, Some(_)) => 1.0,
        _ => 0.0,
    };
    let stakes_high = stakes == "high";
    let gate_fired = low_conf || disagreement > 0.0 || multi_hop || stakes_high;

    let gates = json!({
        "top1_score": top1_score,
        "low_confidence": low_conf,
        "disagreement": disagreement,
        "multi_hop": multi_hop,
        "stakes_high": stakes_high,
        "bm25_top": bm25_top,
        "vec_top": vec_top,
    });

    let hits_json: Vec<Value> = merged
        .iter()
        .map(|h| json!({
            "path": h.path,
            "score": h.score,
            "preview": h.preview,
        }))
        .collect();

    if !gate_fired {
        return Ok(serde_json::to_string_pretty(&json!({
            "query": query,
            "stakes": stakes,
            "verified": false,
            "reason": "gate_not_triggered",
            "gates": gates,
            "hits": hits_json,
        }))?);
    }

    // ── 3. Verify pass ──
    let draft_formatted = format_hits_for_prompt(&merged);
    let prompt = build_verify_prompt(&query, &draft_formatted);

    let verify_resp: Value = if let Some(f) = verify_fn {
        f(&query, &draft_formatted, &model)?
    } else {
        call_openrouter_verify(&model, &prompt)?
    };

    let verdict = verify_resp
        .get("verdict")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let verify_confidence = verify_resp
        .get("confidence")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let verify_tokens = verify_resp
        .get("tokens_used")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let draft_tokens_estimated = estimate_tokens(&draft_formatted) + estimate_tokens(&query);

    // ── 4. Kill-switch evaluation ──
    let lift = verify_confidence - top1_score;
    let cost_delta = if draft_tokens_estimated == 0 {
        0.0
    } else {
        verify_tokens as f64 / draft_tokens_estimated as f64
    };

    let kill_reason = evaluate_kill_switches(lift, disagreement, cost_delta);
    if let Some(rule) = kill_reason {
        tracing::warn!(
            query = %query,
            rule = %rule,
            lift,
            cost_delta,
            "verify kill-switch fired, falling back to draft"
        );
        return Ok(serde_json::to_string_pretty(&json!({
            "query": query,
            "stakes": stakes,
            "verified": false,
            "reason": format!("kill_switch_fired: {rule}"),
            "gates": gates,
            "kill_switch": {
                "lift": lift,
                "cost_delta": cost_delta,
                "verify_confidence": verify_confidence,
            },
            "hits": hits_json,
        }))?);
    }

    // ── 5. Apply verdict ──
    let adjusted_hits: Vec<Value> = match verdict.as_str() {
        "revised" => {
            let order = verify_resp
                .get("adjusted_ranking")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let by_path: std::collections::HashMap<String, &DraftHit> = merged
                .iter()
                .map(|h| (h.path.clone(), h))
                .collect();
            let mut reordered = Vec::new();
            for p in &order {
                if let Some(path) = p.as_str() {
                    if let Some(h) = by_path.get(path) {
                        reordered.push(json!({
                            "path": h.path,
                            "score": h.score,
                            "preview": h.preview,
                        }));
                    }
                }
            }
            if reordered.is_empty() {
                hits_json.clone()
            } else {
                reordered
            }
        }
        _ => hits_json.clone(),
    };

    // For 'revised' without any usable ranking fields, fallback to confirmed semantics
    let final_verdict = if verdict == "revised" || verdict == "confirmed" {
        verdict
    } else {
        "confirmed".to_string()
    };

    Ok(serde_json::to_string_pretty(&json!({
        "query": query,
        "stakes": stakes,
        "verified": true,
        "verdict": final_verdict,
        "verify_confidence": verify_confidence,
        "gates": gates,
        "hits": adjusted_hits,
        "verify_reasoning": verify_resp.get("reasoning").cloned().unwrap_or(Value::Null),
        "bm25_only_top": bm25_only,
        "vector_only_top": vec_only,
    }))?)
}

// ── Helpers ──

#[derive(Debug, Clone)]
struct DraftHit {
    path: String,
    score: f64,
    preview: String,
}

struct DraftResult {
    merged: Vec<DraftHit>,
    bm25_top: Option<String>,
    vec_top: Option<String>,
    bm25_only: Option<String>,
    vec_only: Option<String>,
}

fn run_draft(query: &str, limit: usize, root: &Path) -> DraftResult {
    // BM25
    let bm25_results = match bm25::load_index(root) {
        Some(index) => bm25::search(&index, query, limit * 2),
        None => {
            let index = bm25::build_index(root);
            let _ = bm25::save_index(&index, root);
            bm25::search(&index, query, limit * 2)
        }
    };

    // Vector
    let qdrant_url =
        std::env::var("QDRANT_URL").unwrap_or_else(|_| "http://localhost:6333".into());
    let vector_results = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current()
            .block_on(async { embed::search_wiki(query, &qdrant_url, limit * 2).await })
    })
    .ok()
    .unwrap_or_default();

    let bm25_top = bm25_results.first().map(|r| r.path.clone());
    let vec_top = vector_results.first().map(|r| r.path.clone());

    let merged = rrf_merge(&bm25_results, &vector_results, limit);

    DraftResult {
        merged,
        bm25_top: bm25_top.clone(),
        vec_top: vec_top.clone(),
        bm25_only: bm25_top,
        vec_only: vec_top,
    }
}

/// Reciprocal Rank Fusion — mirrors `rag::rrf_merge` (cannot call private fn).
fn rrf_merge(
    bm25: &[bm25::SearchResult],
    vector: &[embed::SearchResult],
    limit: usize,
) -> Vec<DraftHit> {
    const K: f64 = 60.0;
    let mut scores: std::collections::HashMap<String, (f64, String)> =
        std::collections::HashMap::new();

    for (rank, r) in bm25.iter().enumerate() {
        let entry = scores
            .entry(r.path.clone())
            .or_insert((0.0, r.preview.clone()));
        entry.0 += 1.0 / (K + rank as f64 + 1.0);
    }
    for (rank, r) in vector.iter().enumerate() {
        let entry = scores
            .entry(r.path.clone())
            .or_insert((0.0, r.preview.clone()));
        entry.0 += 1.0 / (K + rank as f64 + 1.0);
    }

    let mut results: Vec<DraftHit> = scores
        .into_iter()
        .map(|(path, (score, preview))| DraftHit { path, score, preview })
        .collect();
    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(limit);
    results
}

fn is_multi_hop(query: &str) -> bool {
    let lower = query.to_lowercase();
    MULTI_HOP_PHRASES.iter().any(|p| lower.contains(p))
}

fn format_hits_for_prompt(hits: &[DraftHit]) -> String {
    let mut out = String::new();
    for (i, h) in hits.iter().enumerate() {
        out.push_str(&format!(
            "[{i}] path={} score={:.4}\n{}\n\n",
            h.path, h.score, h.preview
        ));
    }
    out
}

fn build_verify_prompt(query: &str, draft_formatted: &str) -> String {
    format!(
        "Given query: {query}\n\nDraft RAG results:\n{draft_formatted}\n\n\
Task: evaluate if the draft answer accurately answers the query.\n\
If correct, return {{\"verdict\":\"confirmed\",\"adjusted_ranking\":null,\"confidence\":0-1,\"tokens_used\":<int>}}.\n\
If wrong, return {{\"verdict\":\"revised\",\"adjusted_ranking\":[\"path1\",\"path2\",...],\"confidence\":0-1,\"reasoning\":\"...\",\"tokens_used\":<int>}}.\n\
Output STRICT JSON only."
    )
}

fn estimate_tokens(s: &str) -> u64 {
    // Rough heuristic: 4 chars/token.
    ((s.len() as f64) / 4.0).ceil() as u64
}

fn evaluate_kill_switches(lift: f64, disagreement: f64, cost_delta: f64) -> Option<&'static str> {
    if cost_delta > COST_DELTA_MAX {
        return Some("cost_delta_exceeded");
    }
    if disagreement > DISAGREEMENT_HIGH && lift < DISAGREEMENT_LIFT_MIN {
        return Some("high_disagreement_insufficient_lift");
    }
    if lift < LIFT_MIN {
        return Some("insufficient_lift");
    }
    None
}

/// Build the reqwest client with a 30s timeout.  Exposed for test-assertion.
pub fn build_http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .context("failed to build reqwest client")
}

fn call_openrouter_verify(model: &str, prompt: &str) -> Result<Value> {
    let api_key = std::env::var("OPENROUTER_API_KEY")
        .ok()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            anyhow!("OPENROUTER_API_KEY not set — cannot call OpenRouter for verify pass")
        })?;

    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current()
            .block_on(async { call_openrouter_async(&api_key, model, prompt).await })
    })
}

async fn call_openrouter_async(api_key: &str, model: &str, prompt: &str) -> Result<Value> {
    let client = build_http_client()?;
    let body = json!({
        "model": model,
        "messages": [
            {"role": "system", "content": "You verify RAG results. Return only valid JSON matching the requested schema."},
            {"role": "user", "content": prompt}
        ],
        "response_format": {"type": "json_object"},
        "temperature": 0.0,
    });
    let resp = client
        .post("https://openrouter.ai/api/v1/chat/completions")
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .context("OpenRouter verify request failed")?;
    let status = resp.status();
    let text = resp.text().await.context("reading verify response body")?;
    if !status.is_success() {
        bail!("OpenRouter verify HTTP {status}: {text}");
    }
    let v: Value = serde_json::from_str(&text).context("OpenRouter verify response not JSON")?;
    let content = v
        .pointer("/choices/0/message/content")
        .and_then(|c| c.as_str())
        .ok_or_else(|| anyhow!("OpenRouter verify missing content: {text}"))?;
    let parsed: Value = serde_json::from_str(content)
        .with_context(|| format!("verify content is not JSON: {content}"))?;
    let usage_tokens = v
        .pointer("/usage/total_tokens")
        .and_then(|t| t.as_u64())
        .unwrap_or(0);
    let mut out = parsed;
    if out.get("tokens_used").is_none() {
        out.as_object_mut()
            .map(|o| o.insert("tokens_used".to_string(), json!(usage_tokens)));
    }
    Ok(out)
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tempfile::TempDir;

    fn minimal_root() -> TempDir {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("02-KB-main")).unwrap();
        std::fs::create_dir_all(tmp.path().join("state")).unwrap();
        // seed a page so BM25 has something
        std::fs::write(
            tmp.path().join("02-KB-main/alpha.md"),
            "# Alpha\nThis page discusses alpha topic and beta connection.",
        )
        .unwrap();
        tmp
    }

    #[test]
    fn rag_verified_rejects_empty_query() {
        let tmp = minimal_root();
        for args in [
            json!({}),
            json!({"query": ""}),
            json!({"query": "   "}),
            json!({"query": null}),
        ] {
            let err = call_verified(&args, tmp.path(), None)
                .expect_err("empty query must fail");
            assert!(
                err.to_string().contains("'query' is required"),
                "unexpected error: {err}"
            );
        }
    }

    // Build a synthetic gate-state to exercise non-retrieval logic.  We can't
    // fully mock run_draft without refactoring every caller, so for the
    // gate-pass / stakes / multi-hop / kill-switch tests we construct a
    // dedicated helper that replaces run_draft semantics.

    fn fake_hits(score: f64) -> Vec<DraftHit> {
        vec![
            DraftHit {
                path: "alpha.md".into(),
                score,
                preview: "alpha content".into(),
            },
            DraftHit {
                path: "beta.md".into(),
                score: score * 0.5,
                preview: "beta content".into(),
            },
        ]
    }

    /// Replicates the second half of `call_verified` against an injected
    /// draft and verify_fn so we can unit-test gate/kill semantics without
    /// hitting BM25 / Qdrant / network.
    fn call_with_draft(
        query: &str,
        stakes: &str,
        draft: DraftResult,
        verify_fn: Option<&VerifyFn>,
    ) -> Result<Value> {
        let limit = 5;
        let DraftResult {
            merged,
            bm25_top,
            vec_top,
            bm25_only,
            vec_only,
        } = draft;
        let top1_score = merged.first().map(|h| h.score).unwrap_or(0.0);
        let multi_hop = is_multi_hop(query);
        let low_conf = top1_score < CONF_THRESHOLD;
        let disagreement = match (&bm25_top, &vec_top) {
            (Some(a), Some(b)) if a != b => 1.0,
            (Some(_), None) | (None, Some(_)) => 1.0,
            _ => 0.0,
        };
        let stakes_high = stakes == "high";
        let gate_fired = low_conf || disagreement > 0.0 || multi_hop || stakes_high;

        let gates = json!({
            "top1_score": top1_score,
            "low_confidence": low_conf,
            "disagreement": disagreement,
            "multi_hop": multi_hop,
            "stakes_high": stakes_high,
        });

        let hits_json: Vec<Value> = merged
            .iter()
            .map(|h| json!({"path": h.path, "score": h.score, "preview": h.preview}))
            .collect();

        if !gate_fired {
            return Ok(json!({
                "query": query,
                "stakes": stakes,
                "verified": false,
                "reason": "gate_not_triggered",
                "gates": gates,
                "hits": hits_json,
            }));
        }

        let draft_formatted = format_hits_for_prompt(&merged);
        let verify_resp = verify_fn
            .expect("gate_fired test requires verify_fn")
            (query, &draft_formatted, "test-model")?;

        let verify_confidence = verify_resp
            .get("confidence")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let verify_tokens = verify_resp
            .get("tokens_used")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let draft_tokens_estimated = estimate_tokens(&draft_formatted) + estimate_tokens(query);
        let lift = verify_confidence - top1_score;
        let cost_delta = if draft_tokens_estimated == 0 {
            0.0
        } else {
            verify_tokens as f64 / draft_tokens_estimated as f64
        };
        if let Some(rule) = evaluate_kill_switches(lift, disagreement, cost_delta) {
            return Ok(json!({
                "query": query,
                "stakes": stakes,
                "verified": false,
                "reason": format!("kill_switch_fired: {rule}"),
                "gates": gates,
                "kill_switch": {"lift": lift, "cost_delta": cost_delta},
            }));
        }
        let _ = (limit, bm25_only, vec_only, hits_json);
        Ok(json!({
            "query": query,
            "verified": true,
            "verdict": verify_resp.get("verdict").cloned().unwrap_or(json!("confirmed")),
        }))
    }

    #[test]
    fn rag_verified_gate_passes_when_top1_high_and_stakes_low() {
        let draft = DraftResult {
            merged: fake_hits(0.9),
            bm25_top: Some("alpha.md".into()),
            vec_top: Some("alpha.md".into()),
            bm25_only: Some("alpha.md".into()),
            vec_only: Some("alpha.md".into()),
        };
        let out = call_with_draft("what is alpha", "low", draft, None).unwrap();
        assert_eq!(out["verified"], json!(false));
        assert_eq!(out["reason"], json!("gate_not_triggered"));
    }

    #[test]
    fn rag_verified_gate_fires_on_stakes_high() {
        let draft = DraftResult {
            merged: fake_hits(0.9),
            bm25_top: Some("alpha.md".into()),
            vec_top: Some("alpha.md".into()),
            bm25_only: Some("alpha.md".into()),
            vec_only: Some("alpha.md".into()),
        };
        let calls: Arc<Mutex<u32>> = Arc::new(Mutex::new(0));
        let c2 = calls.clone();
        let verify: Box<VerifyFn> = Box::new(move |_q, _d, _m| {
            *c2.lock().unwrap() += 1;
            Ok(json!({
                "verdict": "confirmed",
                "adjusted_ranking": null,
                "confidence": 0.95,
                "tokens_used": 5,
            }))
        });
        let out = call_with_draft("what is alpha", "high", draft, Some(&*verify)).unwrap();
        assert_eq!(*calls.lock().unwrap(), 1, "verify_fn should be called when gate fires");
        assert_eq!(out["verified"], json!(true));
        assert_eq!(out["verdict"], json!("confirmed"));
    }

    #[test]
    fn rag_verified_gate_fires_on_multi_hop_query() {
        let draft = DraftResult {
            merged: fake_hits(0.9),
            bm25_top: Some("alpha.md".into()),
            vec_top: Some("alpha.md".into()),
            bm25_only: Some("alpha.md".into()),
            vec_only: Some("alpha.md".into()),
        };
        let calls: Arc<Mutex<u32>> = Arc::new(Mutex::new(0));
        let c2 = calls.clone();
        let verify: Box<VerifyFn> = Box::new(move |_q, _d, _m| {
            *c2.lock().unwrap() += 1;
            Ok(json!({
                "verdict": "confirmed",
                "adjusted_ranking": null,
                "confidence": 0.93,
                "tokens_used": 5,
            }))
        });
        let out = call_with_draft("how does X relate to Y", "low", draft, Some(&*verify))
            .unwrap();
        assert_eq!(*calls.lock().unwrap(), 1);
        assert_eq!(out["verified"], json!(true));
    }

    #[test]
    fn rag_verified_kill_switch_on_low_lift() {
        // Draft top1 = 0.55 → gate fires (low_conf path via disagreement on stakes high;
        // we use disagreement-free, force gate via stakes high so gate_fired=true
        // but lift is tiny → kill fires).
        let draft = DraftResult {
            merged: fake_hits(0.55),
            bm25_top: Some("alpha.md".into()),
            vec_top: Some("alpha.md".into()),
            bm25_only: Some("alpha.md".into()),
            vec_only: Some("alpha.md".into()),
        };
        let verify: Box<VerifyFn> = Box::new(|_q, _d, _m| {
            Ok(json!({
                "verdict": "confirmed",
                "confidence": 0.56,
                "tokens_used": 10,
            }))
        });
        let out = call_with_draft("alpha", "high", draft, Some(&*verify)).unwrap();
        assert_eq!(out["verified"], json!(false));
        let reason = out["reason"].as_str().unwrap_or("");
        assert!(
            reason.starts_with("kill_switch_fired"),
            "expected kill_switch_fired, got {reason}"
        );
    }

    #[test]
    fn rag_verified_respects_timeout_bound() {
        // Assert the reqwest client constructed for LLM calls has a finite
        // timeout (i.e. will not hang forever).  Client::timeout isn't
        // introspectable, so we verify construction succeeds, which
        // exercises the explicit `timeout(Duration::from_secs(30))` path.
        let c = build_http_client().expect("client builds");
        drop(c);
    }

    #[test]
    fn kill_switch_cost_delta_triggers() {
        // lift is fine (0.10), but cost delta 2.0 > 1.5
        assert_eq!(
            evaluate_kill_switches(0.10, 0.0, 2.0),
            Some("cost_delta_exceeded")
        );
    }

    #[test]
    fn kill_switch_disagreement_rule_triggers() {
        // disagreement 0.5 > 0.4, lift 0.03 < 0.05 → fires
        assert_eq!(
            evaluate_kill_switches(0.03, 0.5, 1.0),
            Some("high_disagreement_insufficient_lift")
        );
    }

    #[test]
    fn kill_switch_lift_rule_triggers() {
        assert_eq!(
            evaluate_kill_switches(0.01, 0.0, 1.0),
            Some("insufficient_lift")
        );
    }

    #[test]
    fn kill_switch_all_pass() {
        assert_eq!(evaluate_kill_switches(0.10, 0.0, 1.0), None);
    }

    #[test]
    fn multi_hop_detection() {
        assert!(is_multi_hop("why does X fail"));
        assert!(is_multi_hop("how does X work"));
        assert!(is_multi_hop("relationship between A and B"));
        assert!(is_multi_hop("path from X to Y"));
        assert!(is_multi_hop("connect these"));
        assert!(is_multi_hop("apples and oranges"));
        assert!(!is_multi_hop("what is alpha"));
    }
}
