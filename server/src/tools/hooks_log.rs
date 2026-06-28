use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

pub fn tool_defs() -> Vec<Value> {
    vec![json!({
        "name": "nlr_hooks_log_list",
        "description": "List recent CLI client hook events (pre_tool, post_tool, llm_response, user_prompt, session_start, session_end) with optional filters.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "client": {"type": "string", "description": "Filter by client: claude-code, cline, cursor, forge-code, claw-code, openclaw"},
                "event_type": {"type": "string", "description": "Filter by event type"},
                "session_id": {"type": "string", "description": "Filter by session id"},
                "since": {"type": "string", "description": "Only entries on/after this ISO8601 timestamp"},
                "limit": {"type": "integer", "description": "Max entries (default 50)"}
            }
        }
    })]
}

pub fn call(name: &str, args: &Value, root: &Path) -> Result<String> {
    match name {
        "nlr_hooks_log_list" => {
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
            let filter_client = args.get("client").and_then(|v| v.as_str());
            let filter_event = args.get("event_type").and_then(|v| v.as_str());
            let filter_session = args.get("session_id").and_then(|v| v.as_str());
            let filter_since = args.get("since").and_then(|v| v.as_str());

            let log_path = root.join("state/hooks_log.jsonl");
            let content = fs::read_to_string(&log_path).unwrap_or_default();
            let mut entries: Vec<Value> = Vec::new();

            for line in content.lines() {
                let entry: Value = match serde_json::from_str(line) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                if let Some(c) = filter_client {
                    if entry.get("client").and_then(|v| v.as_str()) != Some(c) {
                        continue;
                    }
                }
                if let Some(e) = filter_event {
                    if entry.get("event_type").and_then(|v| v.as_str()) != Some(e) {
                        continue;
                    }
                }
                if let Some(s) = filter_session {
                    if entry.get("session_id").and_then(|v| v.as_str()) != Some(s) {
                        continue;
                    }
                }
                if let Some(since) = filter_since {
                    let ts = entry.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
                    if ts < since {
                        continue;
                    }
                }
                entries.push(entry);
            }

            // newest first
            entries.sort_by(|a, b| {
                b.get("timestamp").and_then(|v| v.as_str())
                    .cmp(&a.get("timestamp").and_then(|v| v.as_str()))
            });
            entries.truncate(limit);
            Ok(serde_json::to_string_pretty(&entries)?)
        }

        _ => bail!("Unknown hooks_log tool: {name}"),
    }
}
