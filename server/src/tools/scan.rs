use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use walkdir::WalkDir;
use crate::state::StateManager;

pub fn tool_defs() -> Vec<Value> {
    vec![
        json!({"name":"nlr_scan_health","description":"Run a full health check","inputSchema":{"type":"object","properties":{}}}),
        json!({"name":"nlr_scan_staleness","description":"Check for stale wiki pages","inputSchema":{"type":"object","properties":{"threshold_days":{"type":"integer"}}}}),
    ]
}

pub fn call(name: &str, args: &Value, root: &Path, state: &StateManager) -> Result<String> {
    match name {
        "nlr_scan_health" => {
            let mut errors = Vec::new();
            for d in ["00-raw","01-sorted","02-KB-main","07-neuro-link-task","config","state"] {
                if !root.join(d).is_dir() { errors.push(format!("Missing: {d}")); }
            }
            for f in ["CLAUDE.md","02-KB-main/schema.md","config/neuro-link.md"] {
                if !root.join(f).exists() { errors.push(format!("Missing: {f}")); }
            }
            let pending = state.count_pending_tasks().unwrap_or(0);
            let skip = ["schema.md","index.md","log.md"];
            let wiki_pages = WalkDir::new(root.join("02-KB-main")).into_iter().filter_map(|e| e.ok()).filter(|e| e.path().extension().is_some_and(|x| x == "md") && !skip.iter().any(|s| e.file_name().to_string_lossy() == *s)).count();
            let status = if errors.is_empty() { "ok" } else { "error" };
            state.update_heartbeat(status, &errors)?;
            Ok(json!({"status":status,"errors":errors,"pending_tasks":pending,"wiki_pages":wiki_pages}).to_string())
        }
        "nlr_scan_staleness" => {
            let threshold = args.get("threshold_days").and_then(|v| v.as_u64()).unwrap_or(30);
            let kb = root.join("02-KB-main");
            let skip = ["schema.md","index.md","log.md"];
            let now = chrono::Utc::now().date_naive();
            let mut stale = Vec::new();
            for entry in WalkDir::new(&kb).into_iter().filter_map(|e| e.ok()) {
                let path = entry.path();
                if !path.extension().is_some_and(|e| e == "md") || skip.iter().any(|s| path.file_name().is_some_and(|f| f == *s)) { continue; }
                let content = fs::read_to_string(path).unwrap_or_default();
                if let Some(date_str) = content.lines().find(|l| l.starts_with("last_updated:")) {
                    let date_part = date_str.split(':').nth(1).unwrap_or("").trim();
                    if let Ok(date) = chrono::NaiveDate::parse_from_str(date_part, "%Y-%m-%d") {
                        let age = (now - date).num_days();
                        if age > threshold as i64 {
                            let rel = path.strip_prefix(&kb).unwrap_or(path).display().to_string();
                            stale.push(json!({"path": rel, "last_updated": date_part, "age_days": age}));
                        }
                    }
                }
            }
            Ok(serde_json::to_string_pretty(&stale)?)
        }
        _ => bail!("Unknown scan tool: {name}"),
    }
}
