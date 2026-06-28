use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

/// Relations allowed on extracted triples. Must stay in sync with the prompt.
const ALLOWED_RELATIONS: &[&str] = &[
    "IMPLIES",
    "CONTRADICTS",
    "REQUIRES",
    "PRECEDES",
    "SUBCLASS_OF",
    "INSTANCE_OF",
    "CAUSES",
    "MITIGATES",
    "ALTERNATIVE_TO",
    "EVIDENCED_BY",
];

const EXTRACTOR_VERSION: &str = "ontology-v2.0";
const DEFAULT_MODEL: &str = "anthropic/claude-3.5-haiku";

pub fn tool_defs() -> Vec<Value> {
    vec![
        json!({
            "name": "nlr_ontology_generate",
            "description": "Extract reasoning triples (head/relation/tail) from a wiki page using an LLM, with blake3 IDs + provenance",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "page": {"type": "string", "description": "Wiki page path relative to 02-KB-main/"},
                    "dry_run": {"type": "boolean", "description": "Validate inputs and return the prompt without calling the LLM", "default": false},
                    "model": {"type": "string", "description": "OpenRouter model id", "default": DEFAULT_MODEL}
                },
                "required": ["page"]
            }
        }),
        json!({"name":"nlr_ontology_query","description":"Read an existing ontology","inputSchema":{"type":"object","properties":{"name":{"type":"string"},"tier":{"type":"string","enum":["summary","detailed"]}},"required":["name"]}}),
        json!({"name":"nlr_ontology_gaps","description":"Find gaps in ontology coverage","inputSchema":{"type":"object","properties":{}}}),
    ]
}

pub fn call(name: &str, args: &Value, root: &Path) -> Result<String> {
    let ont_dir = root.join("03-ontology-main");
    match name {
        "nlr_ontology_generate" => generate(args, root),
        "nlr_ontology_query" => {
            let ont_name = args["name"].as_str().unwrap_or("");
            let tier = args.get("tier").and_then(|v| v.as_str()).unwrap_or("summary");
            for ont_type in ["domain", "agent", "workflow"] {
                let path = ont_dir.join(ont_type).join(ont_name).join(format!("{tier}.md"));
                if path.exists() {
                    return Ok(fs::read_to_string(&path)?);
                }
            }
            bail!("Ontology not found: {ont_name}")
        }
        "nlr_ontology_gaps" => {
            let mut ontologies = Vec::new();
            for entry in WalkDir::new(&ont_dir).max_depth(3).into_iter().filter_map(|e| e.ok()) {
                if entry.file_name() == "metadata.json" {
                    let meta = fs::read_to_string(entry.path())?;
                    let v: Value = serde_json::from_str(&meta)?;
                    ontologies.push(v);
                }
            }
            let kb = root.join("02-KB-main");
            let mut domains = std::collections::HashSet::new();
            for entry in WalkDir::new(&kb).max_depth(1).into_iter().filter_map(|e| e.ok()) {
                if entry.file_type().is_dir() && entry.depth() == 1 {
                    domains.insert(entry.file_name().to_string_lossy().to_string());
                }
            }
            let covered: std::collections::HashSet<String> = ontologies
                .iter()
                .filter_map(|v| v["name"].as_str().map(String::from))
                .collect();
            let gaps: Vec<String> = domains.difference(&covered).cloned().collect();
            Ok(json!({
                "total_ontologies": ontologies.len(),
                "wiki_domains": domains.len(),
                "uncovered_domains": gaps
            })
            .to_string())
        }
        _ => bail!("Unknown ontology tool: {name}"),
    }
}

fn generate(args: &Value, root: &Path) -> Result<String> {
    // 1. Validate required `page` arg — no silent accept.
    let page = args
        .get("page")
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("nlr_ontology_generate: 'page' is required (non-empty string)"))?
        .to_string();

    let dry_run = args.get("dry_run").and_then(|v| v.as_bool()).unwrap_or(false);
    let model = args
        .get("model")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_MODEL)
        .to_string();

    // 2. Read page content.
    let kb = root.join("02-KB-main");
    let page_path = kb.join(&page);
    let page_content = fs::read_to_string(&page_path)
        .with_context(|| format!("failed to read wiki page: {}", page_path.display()))?;
    let page_sha = sha256_hex(&page_content);

    // 3. Build prompt (identical in dry-run + real mode).
    let prompt = build_prompt(&page_content);

    if dry_run {
        return Ok(serde_json::to_string(&json!({
            "triples": [],
            "dry_run": true,
            "model": model,
            "count": 0,
            "prompt": prompt,
            "source_page": page,
            "source_page_sha256": page_sha,
        }))?);
    }

    // 4. Real LLM call.
    let api_key = std::env::var("OPENROUTER_API_KEY")
        .ok()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            anyhow!("OPENROUTER_API_KEY not set — cannot call OpenRouter. Set it in secrets/.env")
        })?;

    let raw_triples = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            call_openrouter(&api_key, &model, &prompt).await
        })
    })?;

    // 5. Validate + enrich with provenance.
    let now = chrono::Utc::now().to_rfc3339();
    let mut triples = Vec::new();
    for raw in raw_triples {
        let Some(valid) = validate_triple(&raw) else {
            tracing::warn!(triple = ?raw, "dropping invalid ontology triple");
            continue;
        };
        let triple_id = compute_triple_id(&valid.head, &valid.relation, &valid.tail, &page_sha);
        let confidence = raw
            .get("confidence")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.5);
        triples.push(json!({
            "triple_id": triple_id,
            "head": valid.head,
            "relation": valid.relation,
            "tail": valid.tail,
            "confidence": confidence,
            "extractor_version": EXTRACTOR_VERSION,
            "model_id": model,
            "created_at": now,
            "source_page": page,
            "source_page_sha256": page_sha,
        }));
    }

    let count = triples.len();
    Ok(serde_json::to_string(&json!({
        "triples": triples,
        "dry_run": false,
        "model": model,
        "count": count,
    }))?)
}

// ── Helpers ──

fn sha256_hex(s: &str) -> String {
    let digest = Sha256::digest(s.as_bytes());
    hex::encode(digest)
}

fn compute_triple_id(head: &str, relation: &str, tail: &str, page_sha: &str) -> String {
    let material = format!("{head}::{relation}::{tail}::{page_sha}");
    let hash = blake3::hash(material.as_bytes());
    hex::encode(&hash.as_bytes()[..8]) // 16 hex chars
}

fn build_prompt(page_content: &str) -> String {
    format!(
        "Extract reasoning triples from this content. Output JSON array of {{head, relation, tail}}. \
Relations must be one of: IMPLIES, CONTRADICTS, REQUIRES, PRECEDES, SUBCLASS_OF, INSTANCE_OF, \
CAUSES, MITIGATES, ALTERNATIVE_TO, EVIDENCED_BY. head/tail must be noun phrases present in the text. \
Return an object of the form {{\"triples\": [{{\"head\": \"...\", \"relation\": \"...\", \"tail\": \"...\", \"confidence\": 0.0-1.0}}]}} — nothing else.\n\n\
<content>\n{page_content}\n</content>"
    )
}

struct ValidTriple {
    head: String,
    relation: String,
    tail: String,
}

fn validate_triple(raw: &Value) -> Option<ValidTriple> {
    let head = raw.get("head").and_then(|v| v.as_str())?.trim().to_string();
    let relation = raw.get("relation").and_then(|v| v.as_str())?.trim().to_string();
    let tail = raw.get("tail").and_then(|v| v.as_str())?.trim().to_string();
    if head.is_empty() || tail.is_empty() {
        return None;
    }
    if head == tail {
        return None;
    }
    if !ALLOWED_RELATIONS.iter().any(|r| *r == relation) {
        return None;
    }
    Some(ValidTriple { head, relation, tail })
}

async fn call_openrouter(api_key: &str, model: &str, prompt: &str) -> Result<Vec<Value>> {
    let client = reqwest::Client::new();
    let body = json!({
        "model": model,
        "messages": [
            {"role": "system", "content": "You extract reasoning triples from text. Return only valid JSON matching the requested schema."},
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
        .context("OpenRouter request failed")?;

    let status = resp.status();
    let text = resp.text().await.context("reading OpenRouter response body")?;
    if !status.is_success() {
        bail!("OpenRouter HTTP {status}: {text}");
    }

    let v: Value = serde_json::from_str(&text).context("OpenRouter response is not JSON")?;
    let content = v
        .pointer("/choices/0/message/content")
        .and_then(|c| c.as_str())
        .ok_or_else(|| anyhow!("OpenRouter response missing choices[0].message.content: {text}"))?;

    let parsed: Value = serde_json::from_str(content)
        .with_context(|| format!("LLM content is not JSON: {content}"))?;
    let arr = parsed
        .get("triples")
        .and_then(|t| t.as_array())
        .ok_or_else(|| anyhow!("LLM JSON missing 'triples' array: {content}"))?;
    Ok(arr.clone())
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_page(content: &str) -> (TempDir, std::path::PathBuf) {
        let tmp = TempDir::new().unwrap();
        let kb = tmp.path().join("02-KB-main");
        fs::create_dir_all(&kb).unwrap();
        let page = kb.join("test.md");
        fs::write(&page, content).unwrap();
        (tmp, std::path::PathBuf::from("test.md"))
    }

    #[test]
    fn generate_rejects_empty_page_arg() {
        let tmp = TempDir::new().unwrap();
        for args in [
            json!({}),
            json!({"page": ""}),
            json!({"page": "   "}),
            json!({"page": null}),
        ] {
            let err = generate(&args, tmp.path()).expect_err("empty page must fail");
            assert!(
                err.to_string().contains("'page' is required"),
                "unexpected error: {err}"
            );
        }
    }

    #[test]
    fn generate_dry_run_returns_prompt_without_llm_call() {
        // Ensure no key is required for dry-run.
        std::env::remove_var("OPENROUTER_API_KEY");

        let (tmp, rel) = setup_page("Neuro-link uses qdrant for vector search.");
        let out = generate(
            &json!({"page": rel.to_string_lossy(), "dry_run": true}),
            tmp.path(),
        )
        .expect("dry-run must succeed without an API key");

        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["dry_run"], json!(true));
        assert_eq!(v["count"], json!(0));
        assert!(v["triples"].as_array().unwrap().is_empty());
        let prompt = v["prompt"].as_str().unwrap();
        assert!(prompt.contains("Extract reasoning triples"));
        assert!(prompt.contains("qdrant for vector search"));
        assert!(v["source_page_sha256"].as_str().unwrap().len() == 64);
    }

    #[test]
    fn generate_validates_relation_allowlist() {
        // Valid
        assert!(validate_triple(&json!({
            "head": "A", "relation": "IMPLIES", "tail": "B"
        }))
        .is_some());

        // Lowercase relation — rejected
        assert!(validate_triple(&json!({
            "head": "A", "relation": "implies", "tail": "B"
        }))
        .is_none());

        // Unknown relation — rejected
        assert!(validate_triple(&json!({
            "head": "A", "relation": "RELATED_TO", "tail": "B"
        }))
        .is_none());

        // Empty head — rejected
        assert!(validate_triple(&json!({
            "head": "", "relation": "IMPLIES", "tail": "B"
        }))
        .is_none());

        // head == tail — rejected
        assert!(validate_triple(&json!({
            "head": "A", "relation": "IMPLIES", "tail": "A"
        }))
        .is_none());

        // Missing field — rejected
        assert!(validate_triple(&json!({
            "head": "A", "relation": "IMPLIES"
        }))
        .is_none());
    }

    #[test]
    fn triple_id_is_deterministic() {
        let a = compute_triple_id("foo", "IMPLIES", "bar", "abc123");
        let b = compute_triple_id("foo", "IMPLIES", "bar", "abc123");
        assert_eq!(a, b);
        assert_eq!(a.len(), 16);

        // Different inputs -> different ids.
        let c = compute_triple_id("foo", "IMPLIES", "baz", "abc123");
        assert_ne!(a, c);
        let d = compute_triple_id("foo", "IMPLIES", "bar", "different_page");
        assert_ne!(a, d);
    }
}
