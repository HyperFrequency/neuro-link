// Export API passthrough logs (state/llm_logs/*/*.jsonl) to Obsidian markdown.
// One markdown file per client_hash+session, grouped by session_id.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub fn export_llm_sessions(llm_logs_dir: &Path, vault_path: &Path) -> Result<Vec<PathBuf>> {
    let out_dir = vault_path.join("llm-sessions");
    fs::create_dir_all(&out_dir)?;

    // Group entries by (client_hash, session_id)
    let mut groups: HashMap<(String, String), Vec<Value>> = HashMap::new();
    for entry in WalkDir::new(llm_logs_dir).into_iter().filter_map(|e| e.ok()) {
        let p = entry.path();
        if !p.extension().is_some_and(|e| e == "jsonl") { continue; }
        let content = fs::read_to_string(p).unwrap_or_default();
        for line in content.lines() {
            let v: Value = match serde_json::from_str(line) { Ok(v) => v, Err(_) => continue };
            let client = v.get("client_token_hash").and_then(|x| x.as_str()).unwrap_or("unknown").to_string();
            let session = v.get("session_id").and_then(|x| x.as_str()).unwrap_or("no-session").to_string();
            groups.entry((client, session)).or_default().push(v);
        }
    }

    let mut written = Vec::new();
    for ((client, session), mut entries) in groups {
        entries.sort_by(|a, b| {
            a.get("timestamp").and_then(|v| v.as_str()).unwrap_or("").cmp(
                b.get("timestamp").and_then(|v| v.as_str()).unwrap_or("")
            )
        });
        let md = format_llm_session(&client, &session, &entries);
        let date = entries.first()
            .and_then(|e| e.get("timestamp"))
            .and_then(|v| v.as_str())
            .map(|s| s.get(..10).unwrap_or("undated").to_string())
            .unwrap_or_else(|| "undated".into());
        let filename = format!("{}-{}-{}.md", date, client, &session[..session.len().min(12)]);
        let path = out_dir.join(filename);
        fs::write(&path, md).with_context(|| format!("writing {}", path.display()))?;
        written.push(path);
    }
    Ok(written)
}

fn format_llm_session(client_hash: &str, session_id: &str, entries: &[Value]) -> String {
    let mut out = String::new();

    let first_ts = entries.first().and_then(|e| e.get("timestamp")).and_then(|v| v.as_str()).unwrap_or("");
    let last_ts = entries.last().and_then(|e| e.get("timestamp")).and_then(|v| v.as_str()).unwrap_or("");

    let mut total_input = 0u64;
    let mut total_output = 0u64;
    let mut total_cost = 0.0f64;
    let mut providers: HashMap<String, u32> = HashMap::new();
    let mut models: HashMap<String, u32> = HashMap::new();
    let mut errors = 0u32;
    let mut rag_injected = 0u32;

    for e in entries {
        total_input += e["tokens"]["input"].as_u64().unwrap_or(0);
        total_output += e["tokens"]["output"].as_u64().unwrap_or(0);
        total_cost += e["cost_usd"].as_f64().unwrap_or(0.0);
        if let Some(p) = e.get("provider").and_then(|v| v.as_str()) {
            *providers.entry(p.to_string()).or_default() += 1;
        }
        if let Some(m) = e.get("model").and_then(|v| v.as_str()) {
            *models.entry(m.to_string()).or_default() += 1;
        }
        if !e.get("error").map(|x| x.is_null()).unwrap_or(true) { errors += 1; }
        if e.get("rag_context_injected").and_then(|v| v.as_bool()).unwrap_or(false) { rag_injected += 1; }
    }

    // Frontmatter
    out.push_str("---\n");
    out.push_str(&format!("title: \"LLM Session {} — {}\"\n", session_id, client_hash));
    out.push_str(&format!("session_id: {}\n", session_id));
    out.push_str(&format!("client_hash: {}\n", client_hash));
    out.push_str("source: api-passthrough\n");
    out.push_str(&format!("started: {}\n", first_ts));
    out.push_str(&format!("ended: {}\n", last_ts));
    out.push_str(&format!("calls: {}\n", entries.len()));
    out.push_str(&format!("errors: {}\n", errors));
    out.push_str(&format!("rag_injected: {}\n", rag_injected));
    out.push_str("tokens:\n");
    out.push_str(&format!("  input: {}\n", total_input));
    out.push_str(&format!("  output: {}\n", total_output));
    out.push_str(&format!("cost_usd_estimate: {:.4}\n", total_cost));
    let providers_list: Vec<String> = providers.iter().map(|(p, n)| format!("{}: {}", p, n)).collect();
    out.push_str(&format!("providers: {{{}}}\n", providers_list.join(", ")));
    out.push_str("tags: [llm-session, api-passthrough]\n");
    out.push_str("---\n\n");

    out.push_str(&format!("# LLM Session `{}`\n\n", &session_id[..session_id.len().min(16)]));
    out.push_str(&format!("**Client:** `{}`\n", client_hash));
    out.push_str(&format!("**Calls:** {} ({} errors, {} with RAG context)\n\n", entries.len(), errors, rag_injected));

    out.push_str("## Calls\n\n");
    for (i, e) in entries.iter().enumerate() {
        let ts = e.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
        let provider = e.get("provider").and_then(|v| v.as_str()).unwrap_or("?");
        let model = e.get("model").and_then(|v| v.as_str()).unwrap_or("?");
        let endpoint = e.get("endpoint").and_then(|v| v.as_str()).unwrap_or("?");
        let latency = e.get("latency_ms").and_then(|v| v.as_u64()).unwrap_or(0);
        let in_tok = e["tokens"]["input"].as_u64().unwrap_or(0);
        let out_tok = e["tokens"]["output"].as_u64().unwrap_or(0);
        let cost = e.get("cost_usd").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let error = e.get("error").and_then(|v| v.as_str());
        let status = if error.is_some() { "❌" } else { "✓" };

        out.push_str(&format!("### Call {} — {} `{}/{}`\n", i + 1, status, provider, model));
        out.push_str(&format!("- Time: {}\n", ts));
        out.push_str(&format!("- Endpoint: `{}`\n", endpoint));
        out.push_str(&format!("- Tokens: {} in / {} out\n", in_tok, out_tok));
        out.push_str(&format!("- Latency: {}ms\n", latency));
        out.push_str(&format!("- Cost: ${:.4}\n", cost));
        if let Some(err) = error {
            out.push_str(&format!("- **Error:** {}\n", err));
        }
        if e.get("rag_context_injected").and_then(|v| v.as_bool()).unwrap_or(false) {
            if let Some(pages) = e.get("rag_pages").and_then(|v| v.as_array()) {
                let page_list: Vec<String> = pages.iter().filter_map(|p| p.as_str().map(|s| format!("[[{}]]", s))).collect();
                out.push_str(&format!("- RAG pages: {}\n", page_list.join(", ")));
            }
        }
        out.push('\n');
    }
    out
}

pub fn parse_ts(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&Utc))
}
