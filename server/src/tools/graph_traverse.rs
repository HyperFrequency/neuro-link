//! D6 — nlr_graph_traverse: intent-gated Neo4j ontology traversal.
//!
//! Single-point traversal over an allowlisted edge set (IMPLIES, REQUIRES,
//! CAUSES, SUBCLASS_OF). Only triggers when the query has >=2 named-entity-like
//! tokens OR contains a multi-hop marker phrase. Returns up to 50 paths with a
//! hard 200-node cap. No arbitrary Cypher — the query is built from bound
//! parameters and a fixed edge allowlist.

use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::path::Path;

const MAX_PATHS: usize = 50;
const MAX_NODES: usize = 200;
const DEFAULT_MAX_HOPS: i64 = 2;
const MAX_HOPS_CEILING: i64 = 4;

/// Phrases that unambiguously signal a multi-hop/relational query.
const MULTI_HOP_MARKERS: &[&str] = &[
    "path from",
    "how does",
    "how do",
    "relate",
    "related to",
    "relationship between",
    "connect",
    "connects",
    "connection between",
    "depends on",
    "dependency",
    "implies",
    "causes",
    "subclass",
];

pub fn tool_defs() -> Vec<Value> {
    vec![json!({
        "name": "nlr_graph_traverse",
        "description": "Neo4j ontology traversal gated by intent classifier. Returns up to 50 paths over allowlisted edges (IMPLIES, REQUIRES, CAUSES, SUBCLASS_OF). Uses a 200-node cap. Skips traversal and returns empty result when query doesn't meet gate criteria.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "query": {"type": "string"},
                "max_hops": {"type": "integer", "default": 2, "maximum": 4}
            },
            "required": ["query"]
        }
    })]
}

pub fn call(name: &str, args: &Value, _root: &Path) -> Result<String> {
    if name != "nlr_graph_traverse" {
        bail!("Unknown graph_traverse tool: {name}");
    }

    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if query.is_empty() {
        bail!("query is required and must be non-empty");
    }

    let max_hops = match args.get("max_hops") {
        Some(v) => match v.as_i64() {
            Some(n) => n,
            None => bail!("max_hops must be an integer"),
        },
        None => DEFAULT_MAX_HOPS,
    };
    if !(1..=MAX_HOPS_CEILING).contains(&max_hops) {
        bail!(
            "max_hops must be between 1 and {} (got {})",
            MAX_HOPS_CEILING,
            max_hops
        );
    }

    // Intent gate.
    let gate = classify_intent(&query);
    if !gate.triggered {
        return Ok(serde_json::to_string_pretty(&json!({
            "triggered": false,
            "reason": gate.reason,
            "query": query,
            "paths": [],
            "node_count": 0,
            "path_count": 0,
        }))?);
    }

    // Run traversal (blocking-over-async per codebase convention).
    let result = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            run_traversal(&query, max_hops).await
        })
    });

    match result {
        Ok((paths, node_count)) => Ok(serde_json::to_string_pretty(&json!({
            "triggered": true,
            "reason": gate.reason,
            "query": query,
            "max_hops": max_hops,
            "paths": paths,
            "node_count": node_count,
            "path_count": paths.len(),
        }))?),
        Err(e) => Ok(serde_json::to_string_pretty(&json!({
            "triggered": true,
            "reason": gate.reason,
            "query": query,
            "max_hops": max_hops,
            "paths": [],
            "node_count": 0,
            "path_count": 0,
            "error": e.to_string(),
        }))?),
    }
}

struct GateDecision {
    triggered: bool,
    reason: String,
}

/// Heuristic: count distinct entity-like tokens in the query. An entity-like
/// token is either a Capitalized word (>=2 chars) or a quoted phrase. Also
/// triggers on multi-hop marker phrases.
fn classify_intent(query: &str) -> GateDecision {
    let lower = query.to_lowercase();
    for marker in MULTI_HOP_MARKERS {
        if lower.contains(marker) {
            return GateDecision {
                triggered: true,
                reason: format!("multi-hop-marker:{marker}"),
            };
        }
    }

    let entities = extract_entities(query);
    if entities.len() >= 2 {
        GateDecision {
            triggered: true,
            reason: format!("multi-entity:{}", entities.len()),
        }
    } else {
        GateDecision {
            triggered: false,
            reason: "single-entity query".to_string(),
        }
    }
}

/// Extract entity-like tokens: Capitalized (first-char uppercase, len>=2,
/// alphabetic-first) OR quoted spans. Deduplicated case-insensitively.
fn extract_entities(query: &str) -> Vec<String> {
    let mut ents: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Quoted spans first.
    let quote_re = regex::Regex::new(r#""([^"]{2,})"|'([^']{2,})'"#).unwrap();
    for caps in quote_re.captures_iter(query) {
        let m = caps.get(1).or_else(|| caps.get(2)).map(|m| m.as_str());
        if let Some(s) = m {
            let key = s.to_lowercase();
            if seen.insert(key) {
                ents.push(s.to_string());
            }
        }
    }

    // Capitalized words, but skip the very first word (sentence-initial cap is
    // not a reliable entity signal) and skip single-letter caps.
    let mut words = query.split_whitespace();
    let _ = words.next(); // skip first
    for raw in words {
        let w = raw.trim_matches(|c: char| !c.is_alphanumeric());
        if w.len() < 2 {
            continue;
        }
        let first = w.chars().next().unwrap();
        if !first.is_uppercase() || !first.is_alphabetic() {
            continue;
        }
        let key = w.to_lowercase();
        if seen.insert(key) {
            ents.push(w.to_string());
        }
    }

    ents
}

async fn run_traversal(query: &str, max_hops: i64) -> Result<(Vec<Value>, usize)> {
    // Allowlist is hardcoded in the Cypher (no user input touches the edge
    // type filter). Only $q and $max_hops are bound.
    //
    // Use a quantified path expression; depth capped by $max_hops. We LIMIT
    // paths to MAX_PATHS at the DB level, then enforce the node cap client-side.
    let cypher = format!(
        "MATCH p=(n:Entity)-[r:IMPLIES|REQUIRES|CAUSES|SUBCLASS_OF*1..{hops}]->(m:Entity) \
         WHERE toLower(n.canonical_name) CONTAINS $q OR toLower(m.canonical_name) CONTAINS $q \
         RETURN p LIMIT {limit}",
        hops = max_hops,
        limit = MAX_PATHS,
    );

    let params = json!({
        "q": query.to_lowercase(),
    });

    let url = format!(
        "{}/db/neo4j/tx/commit",
        std::env::var("NEO4J_HTTP_URL").unwrap_or_else(|_| "http://localhost:7474".into())
    );
    let user = std::env::var("NEO4J_USER").unwrap_or_else(|_| "neo4j".into());
    let pass = std::env::var("NEO4J_PASSWORD").ok();

    let client = reqwest::Client::new();
    let body = json!({
        "statements": [{
            "statement": cypher,
            "parameters": params,
            "resultDataContents": ["graph"]
        }]
    });

    let mut req = client.post(&url).json(&body);
    if let Some(p) = pass {
        req = req.basic_auth(user, Some(p));
    }
    let resp = req
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Neo4j HTTP request failed: {e}"))?;
    let result: Value = resp
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("Neo4j response parse failed: {e}"))?;

    if let Some(errors) = result["errors"].as_array() {
        if !errors.is_empty() {
            anyhow::bail!("Neo4j errors: {:?}", errors);
        }
    }

    // Build paths as arrays of {source, edge, target} triples. We enforce
    // MAX_NODES by truncating additional paths once the unique-node count
    // exceeds the cap.
    let mut paths: Vec<Value> = Vec::new();
    let mut unique_nodes: std::collections::HashSet<String> = std::collections::HashSet::new();

    if let Some(rows) = result["results"][0]["data"].as_array() {
        for row in rows {
            if paths.len() >= MAX_PATHS {
                break;
            }
            let graph = &row["graph"];
            let nodes = graph["nodes"].as_array().cloned().unwrap_or_default();
            let rels = graph["relationships"].as_array().cloned().unwrap_or_default();

            // Map node_id -> canonical_name.
            let mut id_to_name: std::collections::HashMap<String, String> =
                std::collections::HashMap::new();
            for n in &nodes {
                let id = n["id"].as_str().map(String::from).unwrap_or_default();
                let name = n["properties"]["canonical_name"]
                    .as_str()
                    .or_else(|| n["properties"]["name"].as_str())
                    .unwrap_or("")
                    .to_string();
                if !id.is_empty() {
                    id_to_name.insert(id.clone(), name.clone());
                }
                if !name.is_empty() {
                    unique_nodes.insert(name);
                }
            }

            // Enforce the 200-node cap across the aggregate result.
            if unique_nodes.len() > MAX_NODES {
                break;
            }

            let mut triples: Vec<Value> = Vec::new();
            for r in &rels {
                let start = r["startNode"].as_str().unwrap_or("");
                let end = r["endNode"].as_str().unwrap_or("");
                let rel_type = r["type"].as_str().unwrap_or("");
                // Re-enforce allowlist client-side as defense in depth.
                if !matches!(rel_type, "IMPLIES" | "REQUIRES" | "CAUSES" | "SUBCLASS_OF") {
                    continue;
                }
                let src = id_to_name.get(start).cloned().unwrap_or_default();
                let dst = id_to_name.get(end).cloned().unwrap_or_default();
                triples.push(json!({
                    "source": src,
                    "edge": rel_type,
                    "target": dst,
                }));
            }

            if !triples.is_empty() {
                paths.push(json!({ "triples": triples }));
            }
        }
    }

    Ok((paths, unique_nodes.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_traverse_rejects_empty_query() {
        let tmp = tempfile::tempdir().unwrap();
        let err = call("nlr_graph_traverse", &json!({"query": ""}), tmp.path()).unwrap_err();
        assert!(err.to_string().contains("query is required"));

        let err2 =
            call("nlr_graph_traverse", &json!({"query": "   "}), tmp.path()).unwrap_err();
        assert!(err2.to_string().contains("query is required"));
    }

    #[test]
    fn graph_traverse_gate_rejects_single_entity() {
        let tmp = tempfile::tempdir().unwrap();
        let out = call(
            "nlr_graph_traverse",
            &json!({"query": "rust"}),
            tmp.path(),
        )
        .unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["triggered"], false);
        assert_eq!(v["paths"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn graph_traverse_gate_classifier_accepts_multi_entity() {
        // Direct classifier test — avoids the tokio-runtime dependency of
        // the full call() path.
        let gate = classify_intent("how does Rust borrow checker relate to lifetime");
        assert!(gate.triggered);
    }

    #[test]
    fn graph_traverse_gate_classifier_accepts_two_capitalized_entities() {
        // No marker phrase — exercises the entity-count path directly.
        let gate = classify_intent("examine Rust and Tokio together");
        assert!(gate.triggered);
        assert!(gate.reason.starts_with("multi-entity") || gate.reason.starts_with("multi-hop-marker"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn graph_traverse_accepts_multi_entity_full_path() {
        // Full call() path — gate passes, Neo4j call fails (connection refused),
        // and we get triggered=true with an embedded error.
        let tmp = tempfile::tempdir().unwrap();
        let out = call(
            "nlr_graph_traverse",
            &json!({"query": "how does Rust borrow checker relate to lifetime"}),
            tmp.path(),
        )
        .unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["triggered"], true);
    }

    #[test]
    fn graph_traverse_rejects_max_hops_out_of_range() {
        let tmp = tempfile::tempdir().unwrap();
        let err_zero = call(
            "nlr_graph_traverse",
            &json!({"query": "how does X relate to Y", "max_hops": 0}),
            tmp.path(),
        )
        .unwrap_err();
        assert!(err_zero.to_string().contains("max_hops"));

        let err_high = call(
            "nlr_graph_traverse",
            &json!({"query": "how does X relate to Y", "max_hops": 10}),
            tmp.path(),
        )
        .unwrap_err();
        assert!(err_high.to_string().contains("max_hops"));
    }

    #[test]
    #[ignore]
    fn graph_traverse_returns_paths_against_live_neo4j() {
        // Opt-in live test: requires a running Neo4j with ontology data.
        let tmp = tempfile::tempdir().unwrap();
        let out = call(
            "nlr_graph_traverse",
            &json!({
                "query": "how does rust relate to ownership",
                "max_hops": 2
            }),
            tmp.path(),
        )
        .unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["triggered"], true);
        assert!(v["paths"].is_array());
    }
}
