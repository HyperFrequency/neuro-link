//! D5 — nlr_session_context: reports current Claude Code session state.
//!
//! Returns the current session id, phase (EXPLORE/BUILD/VERIFY), and gated
//! concepts derived from recent auto-RAG injections. The phase is a heuristic
//! bucketing of the last N hook_log entries by tool name.

use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Number of most-recent hook_log entries considered for phase detection.
const PHASE_WINDOW: usize = 40;
/// Number of most-recent user_prompt entries scanned for gated concepts.
const CONCEPT_WINDOW: usize = 20;
/// Max concepts returned.
const CONCEPT_LIMIT: usize = 5;

pub fn tool_defs() -> Vec<Value> {
    vec![json!({
        "name": "nlr_session_context",
        "description": "Current Claude Code session: id, phase (EXPLORE/BUILD/VERIFY), and concepts gated from recent auto-RAG hits",
        "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false}
    })]
}

pub fn call(name: &str, _args: &Value, root: &Path) -> Result<String> {
    if name != "nlr_session_context" {
        bail!("Unknown session_state tool: {name}");
    }

    let session_id = resolve_session_id(root);
    let (phase, recent_tools) = compute_phase(root);
    let concepts = extract_concepts(root);

    Ok(serde_json::to_string_pretty(&json!({
        "session_id": session_id,
        "phase": phase,
        "concepts": concepts,
        "recent_tools": recent_tools,
    }))?)
}

/// Resolve session_id: env var first, then latest hooks_log entry's session_id,
/// then the newest file stem under state/sessions/, else "unknown".
fn resolve_session_id(root: &Path) -> String {
    if let Ok(id) = std::env::var("CLAUDE_SESSION_ID") {
        if !id.is_empty() {
            return id;
        }
    }

    // Newest session_id in hooks_log.jsonl
    let log_path = root.join("state/hooks_log.jsonl");
    if let Ok(content) = fs::read_to_string(&log_path) {
        for line in content.lines().rev() {
            if let Ok(entry) = serde_json::from_str::<Value>(line) {
                if let Some(s) = entry.get("session_id").and_then(|v| v.as_str()) {
                    if !s.is_empty() {
                        return s.to_string();
                    }
                }
            }
        }
    }

    // Newest file stem in state/sessions/
    let sessions_dir = root.join("state/sessions");
    if let Ok(rd) = fs::read_dir(&sessions_dir) {
        let mut entries: Vec<_> = rd.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|e| {
            e.metadata().and_then(|m| m.modified()).ok()
        });
        if let Some(newest) = entries.last() {
            if let Some(stem) = newest.path().file_stem().map(|s| s.to_string_lossy().to_string()) {
                return stem;
            }
        }
    }

    "unknown".to_string()
}

/// Bucket the last PHASE_WINDOW entries in hooks_log.jsonl by tool name.
/// Returns (phase, recent_tools).
fn compute_phase(root: &Path) -> (String, Vec<Value>) {
    let log_path = root.join("state/hooks_log.jsonl");
    let content = fs::read_to_string(&log_path).unwrap_or_default();

    // Collect tail N entries.
    let lines: Vec<&str> = content.lines().collect();
    let tail_start = lines.len().saturating_sub(PHASE_WINDOW);
    let tail = &lines[tail_start..];

    let mut tool_counts: HashMap<String, u64> = HashMap::new();
    let mut explore: u64 = 0;
    let mut build: u64 = 0;
    let mut verify: u64 = 0;

    for line in tail {
        let entry: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let event_type = entry.get("event_type").and_then(|v| v.as_str()).unwrap_or("");
        // Only pre/post tool events count toward phase.
        if event_type != "pre_tool" && event_type != "post_tool" {
            continue;
        }

        let tool = entry
            .get("tool_name")
            .and_then(|v| v.as_str())
            .or_else(|| entry.get("tool").and_then(|v| v.as_str()))
            .unwrap_or("");
        if tool.is_empty() {
            continue;
        }

        *tool_counts.entry(tool.to_string()).or_default() += 1;

        match tool {
            "Read" | "Grep" | "Glob" | "LS" | "WebFetch" | "WebSearch" => explore += 1,
            "Edit" | "Write" | "NotebookEdit" | "MultiEdit" => build += 1,
            "Bash" => {
                // Bash with non-zero exit_code, or bash events where exit_code is present at all,
                // are treated as verification activity. Otherwise bucket as explore.
                let has_exit = entry.get("exit_code").is_some()
                    || entry.get("result").and_then(|r| r.get("exit_code")).is_some();
                if has_exit {
                    verify += 1;
                } else {
                    explore += 1;
                }
            }
            _ => {}
        }
    }

    let phase = if build == 0 && verify == 0 && explore == 0 {
        "EXPLORE".to_string()
    } else if build >= explore && build >= verify {
        "BUILD".to_string()
    } else if verify > explore && verify >= build {
        "VERIFY".to_string()
    } else {
        "EXPLORE".to_string()
    };

    let mut recent: Vec<(String, u64)> = tool_counts.into_iter().collect();
    recent.sort_by(|a, b| b.1.cmp(&a.1));
    let recent_tools: Vec<Value> = recent
        .into_iter()
        .map(|(name, count)| json!({"name": name, "count": count}))
        .collect();

    (phase, recent_tools)
}

/// Extract up to CONCEPT_LIMIT distinct concepts from the RAG-injection blobs
/// recorded on the latest CONCEPT_WINDOW user_prompt entries. A concept is a
/// wiki filename stem derived from strings like "02-KB-main/topic/slug.md".
fn extract_concepts(root: &Path) -> Vec<String> {
    let log_path = root.join("state/hooks_log.jsonl");
    let content = fs::read_to_string(&log_path).unwrap_or_default();

    // Walk lines newest-first, collect up to CONCEPT_WINDOW user_prompt entries.
    let user_prompts: Vec<Value> = content
        .lines()
        .rev()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|e| e.get("event_type").and_then(|v| v.as_str()) == Some("user_prompt"))
        .take(CONCEPT_WINDOW)
        .collect();

    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out: Vec<String> = Vec::new();
    let page_re = regex::Regex::new(r"02-KB-main/[\w\-/]+/([\w\-]+)\.md").unwrap();

    for entry in user_prompts {
        // Candidate fields that may contain injection blobs.
        let blobs: Vec<String> = [
            entry.get("rag_injection").and_then(|v| v.as_str()).map(String::from),
            entry.get("auto_rag").and_then(|v| v.as_str()).map(String::from),
            entry.get("injected_context").and_then(|v| v.as_str()).map(String::from),
            entry.get("context").and_then(|v| v.as_str()).map(String::from),
        ]
        .into_iter()
        .flatten()
        .collect();

        // Also check a top-level concepts array if present.
        if let Some(arr) = entry.get("concepts").and_then(|v| v.as_array()) {
            for c in arr {
                if let Some(s) = c.as_str() {
                    let key = s.to_string();
                    if seen.insert(key.clone()) {
                        out.push(key);
                        if out.len() >= CONCEPT_LIMIT {
                            return out;
                        }
                    }
                }
            }
        }

        for blob in blobs {
            for caps in page_re.captures_iter(&blob) {
                let slug = caps[1].to_string();
                if seen.insert(slug.clone()) {
                    out.push(slug);
                    if out.len() >= CONCEPT_LIMIT {
                        return out;
                    }
                }
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_root() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("state")).unwrap();
        fs::create_dir_all(tmp.path().join("state/sessions")).unwrap();
        tmp
    }

    #[test]
    fn session_context_default_phase_is_explore_on_empty_logs() {
        let tmp = setup_root();
        // Make sure the env var does not contaminate.
        std::env::remove_var("CLAUDE_SESSION_ID");
        let out = call("nlr_session_context", &json!({}), tmp.path()).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["phase"], "EXPLORE");
        assert!(v["recent_tools"].as_array().unwrap().is_empty());
        assert!(v["concepts"].as_array().unwrap().is_empty());
    }

    #[test]
    fn session_context_detects_build_phase() {
        let tmp = setup_root();
        let log_path = tmp.path().join("state/hooks_log.jsonl");
        let mut body = String::new();
        for i in 0..10 {
            let entry = json!({
                "timestamp": format!("2026-04-17T12:00:{:02}Z", i),
                "event_type": "pre_tool",
                "tool_name": "Edit",
                "session_id": "sess-build",
            });
            body.push_str(&entry.to_string());
            body.push('\n');
        }
        fs::write(&log_path, body).unwrap();

        std::env::remove_var("CLAUDE_SESSION_ID");
        let out = call("nlr_session_context", &json!({}), tmp.path()).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["phase"], "BUILD");
        assert_eq!(v["session_id"], "sess-build");
    }

    #[test]
    fn session_context_returns_session_id_from_env_var() {
        let tmp = setup_root();
        std::env::set_var("CLAUDE_SESSION_ID", "test123");
        let out = call("nlr_session_context", &json!({}), tmp.path()).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["session_id"], "test123");
        std::env::remove_var("CLAUDE_SESSION_ID");
    }

    #[test]
    fn session_context_detects_verify_phase_from_bash_exits() {
        let tmp = setup_root();
        let log_path = tmp.path().join("state/hooks_log.jsonl");
        let mut body = String::new();
        for i in 0..8 {
            let entry = json!({
                "timestamp": format!("2026-04-17T12:00:{:02}Z", i),
                "event_type": "post_tool",
                "tool_name": "Bash",
                "exit_code": 0,
                "session_id": "sess-verify",
            });
            body.push_str(&entry.to_string());
            body.push('\n');
        }
        fs::write(&log_path, body).unwrap();

        std::env::remove_var("CLAUDE_SESSION_ID");
        let out = call("nlr_session_context", &json!({}), tmp.path()).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["phase"], "VERIFY");
    }

    #[test]
    fn session_context_extracts_concepts_from_rag_injection() {
        let tmp = setup_root();
        let log_path = tmp.path().join("state/hooks_log.jsonl");
        let entry = json!({
            "timestamp": "2026-04-17T12:00:00Z",
            "event_type": "user_prompt",
            "session_id": "sess-concept",
            "rag_injection": "See 02-KB-main/rust/ownership.md and 02-KB-main/rust/lifetimes.md",
        });
        fs::write(&log_path, format!("{}\n", entry)).unwrap();

        std::env::remove_var("CLAUDE_SESSION_ID");
        let out = call("nlr_session_context", &json!({}), tmp.path()).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        let concepts = v["concepts"].as_array().unwrap();
        let flat: Vec<&str> = concepts.iter().filter_map(|c| c.as_str()).collect();
        assert!(flat.contains(&"ownership"));
        assert!(flat.contains(&"lifetimes"));
    }
}
