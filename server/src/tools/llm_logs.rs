use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

pub fn tool_defs() -> Vec<Value> {
    vec![
        json!({
            "name": "nlr_llm_logs_list",
            "description": "List recent LLM proxy calls with filters (provider, model, date range, client token hash)",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "provider": {"type": "string", "description": "Filter by provider (anthropic, openrouter, openai)"},
                    "model": {"type": "string", "description": "Filter by model name substring"},
                    "client_hash": {"type": "string", "description": "Filter by specific client token hash"},
                    "date": {"type": "string", "description": "Filter by date (YYYY-MM-DD)"},
                    "limit": {"type": "integer", "description": "Max entries to return (default 50)"}
                }
            }
        }),
        json!({
            "name": "nlr_llm_logs_summary",
            "description": "Aggregate stats across all LLM calls: total calls, tokens, cost, provider/model distribution",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "days": {"type": "integer", "description": "Look back N days (default 7)"}
                }
            }
        }),
        json!({
            "name": "nlr_llm_logs_grade",
            "description": "Return grading metrics from LLM logs: success rate, avg latency, RAG hit rate",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "days": {"type": "integer", "description": "Look back N days (default 7)"}
                }
            }
        }),
    ]
}

pub fn call(name: &str, args: &Value, root: &Path) -> Result<String> {
    let logs_dir = root.join("state/llm_logs");
    match name {
        "nlr_llm_logs_list" => {
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
            let filter_provider = args.get("provider").and_then(|v| v.as_str());
            let filter_model = args.get("model").and_then(|v| v.as_str());
            let filter_hash = args.get("client_hash").and_then(|v| v.as_str());
            let filter_date = args.get("date").and_then(|v| v.as_str());

            let mut entries: Vec<Value> = Vec::new();
            for file in WalkDir::new(&logs_dir).into_iter().filter_map(|e| e.ok()) {
                let path = file.path();
                if !path.extension().is_some_and(|e| e == "jsonl") { continue; }
                if let Some(d) = filter_date {
                    if !path.file_stem().map(|s| s.to_string_lossy().contains(d)).unwrap_or(false) { continue; }
                }
                let content = fs::read_to_string(path).unwrap_or_default();
                for line in content.lines() {
                    let entry: Value = match serde_json::from_str(line) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };
                    if let Some(p) = filter_provider {
                        if entry.get("provider").and_then(|v| v.as_str()) != Some(p) { continue; }
                    }
                    if let Some(m) = filter_model {
                        if !entry.get("model").and_then(|v| v.as_str()).unwrap_or("").contains(m) { continue; }
                    }
                    if let Some(h) = filter_hash {
                        if entry.get("client_token_hash").and_then(|v| v.as_str()) != Some(h) { continue; }
                    }
                    // Compact entry for listing
                    entries.push(json!({
                        "timestamp": entry.get("timestamp"),
                        "client_hash": entry.get("client_token_hash"),
                        "provider": entry.get("provider"),
                        "model": entry.get("model"),
                        "endpoint": entry.get("endpoint"),
                        "tokens": entry.get("tokens"),
                        "latency_ms": entry.get("latency_ms"),
                        "cost_usd": entry.get("cost_usd"),
                        "rag_injected": entry.get("rag_context_injected"),
                        "error": entry.get("error"),
                    }));
                }
            }
            entries.sort_by(|a, b| b["timestamp"].as_str().cmp(&a["timestamp"].as_str()));
            entries.truncate(limit);
            Ok(serde_json::to_string_pretty(&entries)?)
        }

        "nlr_llm_logs_summary" => {
            let days = args.get("days").and_then(|v| v.as_u64()).unwrap_or(7);
            let cutoff = chrono::Utc::now() - chrono::Duration::days(days as i64);
            let mut total_calls: u64 = 0;
            let mut total_input_tokens: u64 = 0;
            let mut total_output_tokens: u64 = 0;
            let mut total_cost: f64 = 0.0;
            let mut provider_counts: std::collections::HashMap<String, u64> = Default::default();
            let mut model_counts: std::collections::HashMap<String, u64> = Default::default();
            let mut errors: u64 = 0;

            for file in WalkDir::new(&logs_dir).into_iter().filter_map(|e| e.ok()) {
                let path = file.path();
                if !path.extension().is_some_and(|e| e == "jsonl") { continue; }
                let content = fs::read_to_string(path).unwrap_or_default();
                for line in content.lines() {
                    let entry: Value = match serde_json::from_str(line) { Ok(v) => v, Err(_) => continue };
                    let ts = entry.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
                    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) {
                        if dt.with_timezone(&chrono::Utc) < cutoff { continue; }
                    }
                    total_calls += 1;
                    total_input_tokens += entry["tokens"]["input"].as_u64().unwrap_or(0);
                    total_output_tokens += entry["tokens"]["output"].as_u64().unwrap_or(0);
                    total_cost += entry["cost_usd"].as_f64().unwrap_or(0.0);
                    if !entry.get("error").map(|e| e.is_null()).unwrap_or(true) { errors += 1; }
                    if let Some(p) = entry.get("provider").and_then(|v| v.as_str()) {
                        *provider_counts.entry(p.to_string()).or_default() += 1;
                    }
                    if let Some(m) = entry.get("model").and_then(|v| v.as_str()) {
                        *model_counts.entry(m.to_string()).or_default() += 1;
                    }
                }
            }

            Ok(serde_json::to_string_pretty(&json!({
                "days": days,
                "total_calls": total_calls,
                "errors": errors,
                "error_rate": if total_calls > 0 { errors as f64 / total_calls as f64 } else { 0.0 },
                "total_input_tokens": total_input_tokens,
                "total_output_tokens": total_output_tokens,
                "total_cost_usd": total_cost,
                "providers": provider_counts,
                "models": model_counts,
            }))?)
        }

        "nlr_llm_logs_grade" => {
            let days = args.get("days").and_then(|v| v.as_u64()).unwrap_or(7);
            let cutoff = chrono::Utc::now() - chrono::Duration::days(days as i64);
            let mut total: u64 = 0;
            let mut successes: u64 = 0;
            let mut rag_injected: u64 = 0;
            let mut latency_sum: u64 = 0;
            let mut latency_max: u64 = 0;

            for file in WalkDir::new(&logs_dir).into_iter().filter_map(|e| e.ok()) {
                let path = file.path();
                if !path.extension().is_some_and(|e| e == "jsonl") { continue; }
                let content = fs::read_to_string(path).unwrap_or_default();
                for line in content.lines() {
                    let entry: Value = match serde_json::from_str(line) { Ok(v) => v, Err(_) => continue };
                    let ts = entry.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
                    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) {
                        if dt.with_timezone(&chrono::Utc) < cutoff { continue; }
                    }
                    total += 1;
                    if entry.get("error").map(|e| e.is_null()).unwrap_or(true) { successes += 1; }
                    if entry.get("rag_context_injected").and_then(|v| v.as_bool()).unwrap_or(false) { rag_injected += 1; }
                    let latency = entry["latency_ms"].as_u64().unwrap_or(0);
                    latency_sum += latency;
                    if latency > latency_max { latency_max = latency; }
                }
            }

            let avg_latency = if total > 0 { latency_sum / total } else { 0 };
            Ok(serde_json::to_string_pretty(&json!({
                "days": days,
                "total_calls": total,
                "success_rate": if total > 0 { successes as f64 / total as f64 } else { 0.0 },
                "rag_injection_rate": if total > 0 { rag_injected as f64 / total as f64 } else { 0.0 },
                "avg_latency_ms": avg_latency,
                "max_latency_ms": latency_max,
            }))?)
        }

        _ => bail!("Unknown llm_logs tool: {name}"),
    }
}
