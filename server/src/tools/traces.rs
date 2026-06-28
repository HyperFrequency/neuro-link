//! Raw-trace MCP tool (F2).
//!
//! Reads gzipped NDJSON files written by `hooks/trace_logger.py` under
//! `state/traces/<session>/` and exposes a single MCP tool, `nlr_trace_read`,
//! that returns either a specific turn (by timestamp order) or the most
//! recent five turns summarised.

use anyhow::{bail, Context, Result};
use flate2::read::GzDecoder;
use serde_json::{json, Value};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

const PREVIEW_LEN: usize = 500;
const DEFAULT_TAIL: usize = 5;

pub fn tool_defs() -> Vec<Value> {
    vec![json!({
        "name": "nlr_trace_read",
        "description": "Read raw hook traces for a Claude Code session. Without `turn` returns the last 5 turns summarised; with `turn` (0-indexed into chronological order) returns that specific turn.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "session": {"type": "string", "description": "Session id (directory name under state/traces/)"},
                "turn": {"type": "integer", "description": "0-indexed chronological turn; omit for last 5"}
            },
            "required": ["session"]
        }
    })]
}

pub fn call(name: &str, args: &Value, root: &Path) -> Result<String> {
    match name {
        "nlr_trace_read" => trace_read(args, root),
        _ => bail!("Unknown trace tool: {name}"),
    }
}

/// Sanitize a session id to match the Python writer's allowlist
/// (`hooks/trace_logger.py` uses `re.sub(r'[^A-Za-z0-9_\-]', '_', ...)[:128]`).
/// Keeps only `[A-Za-z0-9_-]`; all other chars (including `/`, `.`, `\`) become `_`.
fn sanitize_session(raw: &str) -> String {
    raw.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .take(128)
        .collect()
}

fn trace_read(args: &Value, root: &Path) -> Result<String> {
    let raw_session = args
        .get("session")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if raw_session.is_empty() {
        bail!("session is required");
    }
    let session = sanitize_session(raw_session);
    if session.is_empty() {
        bail!("invalid session id");
    }

    let traces_root = root.join("state").join("traces");
    let dir = traces_root.join(&session);
    if !dir.is_dir() {
        bail!("no traces for session '{session}'");
    }

    // Defense-in-depth: canonicalize and assert `dir` is inside `state/traces/`.
    let traces_root_canon = traces_root
        .canonicalize()
        .with_context(|| format!("canonicalize {}", traces_root.display()))?;
    let dir_canon = dir
        .canonicalize()
        .with_context(|| format!("canonicalize {}", dir.display()))?;
    if !dir_canon.starts_with(&traces_root_canon) {
        bail!("Access denied: path outside state/traces");
    }

    let mut files = collect_trace_files(&dir_canon)?;
    if files.is_empty() {
        bail!("no traces for session '{session}'");
    }
    // Chronological order (oldest first).
    files.sort_by(|a, b| a.0.cmp(&b.0));

    let turn_arg = args.get("turn").and_then(|v| v.as_i64());

    let selected_paths: Vec<&PathBuf> = if let Some(turn) = turn_arg {
        if turn < 0 || (turn as usize) >= files.len() {
            bail!(
                "turn {turn} out of range (session has {} turns)",
                files.len()
            );
        }
        vec![&files[turn as usize].1]
    } else {
        let start = files.len().saturating_sub(DEFAULT_TAIL);
        files[start..].iter().map(|(_, p)| p).collect()
    };

    let mut turns = Vec::with_capacity(selected_paths.len());
    for path in selected_paths {
        match read_trace(path) {
            Ok(v) => turns.push(v),
            Err(err) => turns.push(json!({
                "ts": null,
                "error": format!("failed to read {}: {err}", path.display())
            })),
        }
    }

    Ok(serde_json::to_string_pretty(
        &json!({ "session": session, "turns": turns }),
    )?)
}

/// Returns `(sort_key, path)` for every `*.jsonl.gz` file in `dir`.
/// The sort key is the filename; `trace_logger.py` names files
/// `<ms>-<pid>.jsonl.gz`, so lexicographic order is chronological.
fn collect_trace_files(dir: &Path) -> Result<Vec<(String, PathBuf)>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).with_context(|| format!("read_dir {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = match path.file_name().and_then(|s| s.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        if !name.ends_with(".jsonl.gz") {
            continue;
        }
        out.push((name, path));
    }
    Ok(out)
}

fn read_trace(path: &Path) -> Result<Value> {
    let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut decoder = GzDecoder::new(file);
    let mut buf = String::new();
    decoder
        .read_to_string(&mut buf)
        .with_context(|| format!("gunzip {}", path.display()))?;
    // NDJSON: take the first non-empty line (hook logger writes a single line per file,
    // but handle multi-line defensively).
    let line = buf
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or_default();
    let record: Value =
        serde_json::from_str(line).with_context(|| format!("parse JSON from {}", path.display()))?;

    let ts = record.get("ts").cloned().unwrap_or(Value::Null);
    let tool_name = record
        .get("tool_name")
        .cloned()
        .unwrap_or(Value::Null);
    let args_preview = preview(record.get("tool_input"));
    let result_preview = preview(record.get("tool_response"));
    Ok(json!({
        "ts": ts,
        "tool_name": tool_name,
        "args_preview": args_preview,
        "result_preview": result_preview,
    }))
}

fn preview(value: Option<&Value>) -> Value {
    match value {
        None | Some(Value::Null) => Value::Null,
        Some(v) => {
            let s = match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            let trimmed: String = s.chars().take(PREVIEW_LEN).collect();
            Value::String(if s.chars().count() > PREVIEW_LEN {
                format!("{trimmed}…")
            } else {
                trimmed
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;
    use tempfile::TempDir;

    fn write_gz(path: &Path, value: &Value) {
        let file = File::create(path).unwrap();
        let mut enc = GzEncoder::new(file, Compression::default());
        enc.write_all(serde_json::to_string(value).unwrap().as_bytes())
            .unwrap();
        enc.write_all(b"\n").unwrap();
        enc.finish().unwrap();
    }

    fn sample(ts: f64, tool: &str) -> Value {
        json!({
            "ts": ts,
            "tool_name": tool,
            "tool_input": {"x": 1, "text": format!("input-for-{tool}")},
            "tool_response": format!("response-for-{tool}"),
            "session_id": "s1"
        })
    }

    fn setup(session: &str, count: usize) -> TempDir {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("state").join("traces").join(session);
        fs::create_dir_all(&dir).unwrap();
        for i in 0..count {
            // Lexicographic == chronological.
            let fname = format!("{:05}-1.jsonl.gz", i);
            write_gz(&dir.join(fname), &sample(i as f64, &format!("tool{i}")));
        }
        tmp
    }

    #[test]
    fn trace_read_returns_last_5_turns() {
        let tmp = setup("s1", 10);
        let out = trace_read(&json!({"session": "s1"}), tmp.path()).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        let turns = v["turns"].as_array().unwrap();
        assert_eq!(turns.len(), 5);
        // First of the tail should be turn index 5 (tool5), last is tool9.
        assert_eq!(turns[0]["tool_name"], "tool5");
        assert_eq!(turns[4]["tool_name"], "tool9");
    }

    #[test]
    fn trace_read_specific_turn_by_timestamp() {
        let tmp = setup("s1", 3);
        let out = trace_read(&json!({"session": "s1", "turn": 1}), tmp.path()).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        let turns = v["turns"].as_array().unwrap();
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0]["tool_name"], "tool1");
    }

    #[test]
    fn trace_read_with_no_session_returns_error() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("state/traces/s1")).unwrap();
        let err = trace_read(&json!({"session": "s1"}), tmp.path()).unwrap_err();
        assert!(err.to_string().contains("no traces for session"));
    }

    #[test]
    fn trace_read_rejects_empty_session_arg() {
        let tmp = TempDir::new().unwrap();
        let err = trace_read(&json!({"session": ""}), tmp.path()).unwrap_err();
        assert_eq!(err.to_string(), "session is required");
    }

    #[test]
    fn trace_read_rejects_path_traversal() {
        // Layout: <tmp>/state/traces/ (safe root) and <tmp>/secret/leak.jsonl.gz (outside).
        let tmp = TempDir::new().unwrap();
        let traces_root = tmp.path().join("state").join("traces");
        fs::create_dir_all(&traces_root).unwrap();
        let secret_dir = tmp.path().join("secret");
        fs::create_dir_all(&secret_dir).unwrap();
        write_gz(&secret_dir.join("00001-1.jsonl.gz"), &sample(1.0, "leak"));

        // 1. Classic `../` traversal: chars are sanitized to `_`, so the
        //    resulting path is `state/traces/_________` — guaranteed not to
        //    exist, so we bail with "no traces for session".
        let err = trace_read(&json!({"session": "../../secret"}), tmp.path()).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("no traces for session") || msg.contains("invalid session id"),
            "expected traversal rejection, got: {msg}"
        );

        // 2. Symlink escape: `state/traces/escape -> ../../secret` — the
        //    sanitized name passes the allowlist but canonicalize() resolves
        //    outside state/traces/, so the starts_with check must reject.
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            symlink(&secret_dir, traces_root.join("escape")).unwrap();
            let err = trace_read(&json!({"session": "escape"}), tmp.path()).unwrap_err();
            assert!(
                err.to_string().contains("Access denied: path outside state/traces"),
                "expected access-denied via canonicalize check, got: {err}"
            );
        }
    }
}
