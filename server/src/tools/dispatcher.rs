//! Job-scanner dispatcher (A-fu1).
//!
//! Takes a task markdown file from `07-neuro-link-task/`, parses its YAML
//! frontmatter, and routes by `type:` field to the right executor. Writes
//! state transitions (pending → running → completed|failed|waiting_for_*)
//! back into the task frontmatter in-place, and appends a structured event
//! to `state/job_log.jsonl` for every transition.
//!
//! Routing rules (per A-fu1 spec):
//!   - `ingest`    → in-process via `crate::tools::ingest` helpers
//!   - `ontology`  → in-process via `nlr_ontology_generate`
//!   - `report`    → internal helper stub (logs only)
//!   - `research`  → `waiting_for_agent` (enqueue to `state/agent_queue.jsonl`)
//!   - `code-fix`  → `waiting_for_human`
//!   - anything else → failed with "unknown type"

use anyhow::{Context, Result};
use chrono::Utc;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchOutcome {
    pub status: String,
    pub error: Option<String>,
    pub logs: Vec<String>,
}

impl DispatchOutcome {
    fn new(status: &str) -> Self {
        Self { status: status.into(), error: None, logs: Vec::new() }
    }
    fn failed(err: impl Into<String>) -> Self {
        Self { status: "failed".into(), error: Some(err.into()), logs: Vec::new() }
    }
    fn log(&mut self, line: impl Into<String>) {
        self.logs.push(line.into());
    }
}

/// Parse YAML frontmatter from a task file, returning (map, body_after_frontmatter).
/// Preserves the raw body so we can rewrite the file in-place with an updated FM.
pub fn parse_task_file(path: &Path) -> Result<(HashMap<String, String>, String)> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("reading task file {}", path.display()))?;
    let re = Regex::new(r"(?s)^---\n(.+?)\n---\n?")?;
    let Some(caps) = re.captures(&content) else {
        // No frontmatter — treat as empty map + full content as body.
        return Ok((HashMap::new(), content));
    };
    let yaml_str = &caps[1];
    let mut map = HashMap::new();
    for line in yaml_str.lines() {
        if let Some((key, val)) = line.split_once(':') {
            let k = key.trim().to_string();
            let v = val.trim().trim_matches('"').to_string();
            if !k.is_empty() {
                map.insert(k, v);
            }
        }
    }
    let body = content[caps.get(0).unwrap().end()..].to_string();
    Ok((map, body))
}

/// Serialize a frontmatter map deterministically (canonical key order first,
/// then remaining keys sorted alphabetically). Keeps diffs minimal when we
/// rewrite the file in-place.
fn render_frontmatter(fm: &HashMap<String, String>) -> String {
    const CANONICAL_ORDER: &[&str] = &[
        "type", "status", "priority", "created", "depends_on",
        "assigned_harness", "dispatched_at", "completed_at",
        "dispatch_error",
    ];
    let mut out = String::from("---\n");
    let mut seen = std::collections::HashSet::new();
    for key in CANONICAL_ORDER {
        if let Some(val) = fm.get(*key) {
            out.push_str(&format!("{key}: {val}\n"));
            seen.insert((*key).to_string());
        }
    }
    let mut extras: Vec<_> = fm.keys().filter(|k| !seen.contains(k.as_str())).collect();
    extras.sort();
    for key in extras {
        out.push_str(&format!("{key}: {}\n", fm[key]));
    }
    out.push_str("---\n");
    out
}

/// Rewrite the task file with an updated frontmatter (body preserved verbatim).
pub fn write_task_file(path: &Path, fm: &HashMap<String, String>, body: &str) -> Result<()> {
    let rendered = format!("{}{}", render_frontmatter(fm), body);
    // Atomic write: tmp + rename so a mid-write crash can't leave a truncated file.
    let tmp = path.with_extension("md.tmp");
    fs::write(&tmp, rendered.as_bytes())
        .with_context(|| format!("writing tmp task file {}", tmp.display()))?;
    fs::rename(&tmp, path)
        .with_context(|| format!("renaming tmp → {}", path.display()))?;
    Ok(())
}

/// Append a structured event to `state/job_log.jsonl`.
pub fn append_job_log(root: &Path, event: &str, file: &str, extra: Value) -> Result<()> {
    let state_dir = root.join("state");
    fs::create_dir_all(&state_dir)?;
    let path = state_dir.join("job_log.jsonl");
    let mut entry = json!({
        "timestamp": Utc::now().to_rfc3339(),
        "event": event,
        "file": file,
    });
    if let Value::Object(ref mut base) = entry {
        if let Value::Object(extras) = extra {
            for (k, v) in extras {
                base.insert(k, v);
            }
        }
    }
    let mut f = fs::OpenOptions::new().create(true).append(true).open(&path)?;
    writeln!(f, "{}", serde_json::to_string(&entry)?)?;
    Ok(())
}

/// Append to `state/agent_queue.jsonl` — external LLM-backed agents (e.g.
/// claude-code subagent loop) poll this queue for research-type tasks.
fn append_agent_queue(root: &Path, file: &str, fm: &HashMap<String, String>) -> Result<()> {
    let state_dir = root.join("state");
    fs::create_dir_all(&state_dir)?;
    let path = state_dir.join("agent_queue.jsonl");
    let entry = json!({
        "timestamp": Utc::now().to_rfc3339(),
        "file": file,
        "type": fm.get("type").cloned().unwrap_or_default(),
        "priority": fm.get("priority").cloned().unwrap_or_else(|| "3".into()),
        "assigned_harness": fm.get("assigned_harness").cloned().unwrap_or_default(),
        "status": "waiting_for_agent",
    });
    let mut f = fs::OpenOptions::new().create(true).append(true).open(&path)?;
    writeln!(f, "{}", serde_json::to_string(&entry)?)?;
    Ok(())
}

/// Append a failure to `state/job_errors.jsonl` for operator visibility.
pub fn append_job_error(root: &Path, file: &str, err: &str) -> Result<()> {
    let state_dir = root.join("state");
    fs::create_dir_all(&state_dir)?;
    let path = state_dir.join("job_errors.jsonl");
    let entry = json!({
        "timestamp": Utc::now().to_rfc3339(),
        "file": file,
        "error": err,
    });
    let mut f = fs::OpenOptions::new().create(true).append(true).open(&path)?;
    writeln!(f, "{}", serde_json::to_string(&entry)?)?;
    Ok(())
}

/// Dispatch a single task file. Returns the outcome + also writes the task
/// file in-place and appends events to `state/job_log.jsonl`.
pub fn dispatch_task(root: &Path, task_path: &Path) -> Result<DispatchOutcome> {
    let fname = task_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_string();

    // Skip templates/examples by convention.
    if fname.starts_with("example-") || fname.starts_with('.') || fname.starts_with('_') {
        let mut o = DispatchOutcome::new("skipped");
        o.log(format!("skipped template file: {fname}"));
        return Ok(o);
    }

    let (mut fm, body) = parse_task_file(task_path)?;
    let task_type = fm.get("type").cloned().unwrap_or_default();
    let now = Utc::now().to_rfc3339();

    // Transition: pending|<other> → running
    fm.insert("status".into(), "running".into());
    fm.insert("dispatched_at".into(), now.clone());
    fm.remove("completed_at");
    fm.remove("dispatch_error");
    write_task_file(task_path, &fm, &body)?;
    append_job_log(
        root,
        "task_dispatched",
        &fname,
        json!({ "type": task_type, "dispatched_at": now }),
    )?;

    let outcome = match task_type.as_str() {
        "ingest" => dispatch_ingest(root, &fm, &body, &fname),
        "ontology" => dispatch_ontology(root, &fm, &body, &fname),
        "report" => dispatch_report(root, &fm, &body, &fname),
        "research" => dispatch_research(root, &fm, &fname),
        "code-fix" => dispatch_code_fix(root, &fname),
        "" => DispatchOutcome::failed("missing type field"),
        other => DispatchOutcome::failed(format!("unknown type: {other}")),
    };

    // Finalize: write terminal status + completion timestamp back to file.
    let completed_at = Utc::now().to_rfc3339();
    fm.insert("status".into(), outcome.status.clone());
    fm.insert("completed_at".into(), completed_at.clone());
    if let Some(err) = &outcome.error {
        // Preserve error inline in frontmatter; newlines become spaces to stay yaml-safe.
        let safe = err.replace('\n', " | ").replace('"', "'");
        fm.insert("dispatch_error".into(), safe);
    }
    write_task_file(task_path, &fm, &body)?;

    let event = match outcome.status.as_str() {
        "completed" => "task_completed",
        "failed" => "task_failed",
        "waiting_for_agent" => "task_enqueued_for_agent",
        "waiting_for_human" => "task_needs_human_review",
        _ => "task_terminal",
    };
    append_job_log(
        root,
        event,
        &fname,
        json!({
            "type": task_type,
            "status": outcome.status,
            "error": outcome.error,
            "completed_at": completed_at,
        }),
    )?;
    if outcome.status == "failed" {
        if let Some(err) = &outcome.error {
            let _ = append_job_error(root, &fname, err);
        }
    }

    Ok(outcome)
}

fn dispatch_ingest(
    root: &Path,
    fm: &HashMap<String, String>,
    body: &str,
    fname: &str,
) -> DispatchOutcome {
    let mut o = DispatchOutcome::new("completed");
    // Slug preference: explicit `slug:` frontmatter key → else filename stem
    // without the numeric prefix + type segment.
    let slug = fm
        .get("slug")
        .cloned()
        .unwrap_or_else(|| slug_from_filename(fname));
    let content = body.trim().to_string();
    if content.is_empty() {
        return DispatchOutcome::failed("ingest task body is empty");
    }

    if let Err(e) = crate::tools::ingest::ingest_loose_file(root, &slug, &content) {
        return DispatchOutcome::failed(format!("ingest_loose_file: {e}"));
    }
    o.log(format!("ingested loose file for slug={slug}"));

    // Classification + stub curation is async — run on current runtime if any,
    // otherwise spin a scoped mini-runtime.
    let curate_res = match tokio::runtime::Handle::try_current() {
        Ok(handle) => handle.block_on(crate::tools::ingest::auto_classify_and_curate(root, &slug)),
        Err(_) => {
            // No ambient runtime: build one. dispatcher is sync so this is fine.
            match tokio::runtime::Builder::new_current_thread().enable_all().build() {
                Ok(rt) => rt.block_on(crate::tools::ingest::auto_classify_and_curate(root, &slug)),
                Err(e) => Err(anyhow::anyhow!("runtime build failed: {e}")),
            }
        }
    };
    match curate_res {
        Ok(()) => {
            o.log(format!("auto_classify_and_curate ok for slug={slug}"));
            o
        }
        Err(e) => DispatchOutcome::failed(format!("auto_classify_and_curate: {e}")),
    }
}

fn dispatch_ontology(
    root: &Path,
    fm: &HashMap<String, String>,
    body: &str,
    fname: &str,
) -> DispatchOutcome {
    let mut o = DispatchOutcome::new("completed");
    let name = fm
        .get("name")
        .cloned()
        .unwrap_or_else(|| slug_from_filename(fname));
    let ont_type = fm.get("ontology_type").cloned().unwrap_or_else(|| "domain".into());
    let args = json!({ "name": name, "type": ont_type, "text": body });
    match crate::tools::ontology::call("nlr_ontology_generate", &args, root) {
        Ok(msg) => {
            o.log(msg);
            o
        }
        Err(e) => DispatchOutcome::failed(format!("nlr_ontology_generate: {e}")),
    }
}

fn dispatch_report(
    root: &Path,
    _fm: &HashMap<String, String>,
    body: &str,
    fname: &str,
) -> DispatchOutcome {
    // Stub: record the report body to 06-progress-reports/<fname> so it
    // survives and operators can inspect. No further synthesis — that's
    // the responsibility of the reports agent (future work).
    let mut o = DispatchOutcome::new("completed");
    let reports_dir = root.join("06-progress-reports");
    if let Err(e) = fs::create_dir_all(&reports_dir) {
        return DispatchOutcome::failed(format!("mkdir reports: {e}"));
    }
    // Prefix with timestamp so repeated dispatches of the same task don't collide.
    let stamp = Utc::now().format("%Y%m%dT%H%M%S");
    let out_path = reports_dir.join(format!("{stamp}-{fname}"));
    if let Err(e) = fs::write(&out_path, body.as_bytes()) {
        return DispatchOutcome::failed(format!("write report: {e}"));
    }
    o.log(format!("report stub written to {}", out_path.display()));
    o
}

fn dispatch_research(
    root: &Path,
    fm: &HashMap<String, String>,
    fname: &str,
) -> DispatchOutcome {
    let mut o = DispatchOutcome::new("waiting_for_agent");
    if let Err(e) = append_agent_queue(root, fname, fm) {
        return DispatchOutcome::failed(format!("agent_queue append: {e}"));
    }
    o.log("enqueued for external LLM agent (state/agent_queue.jsonl)");
    o
}

fn dispatch_code_fix(_root: &Path, fname: &str) -> DispatchOutcome {
    let mut o = DispatchOutcome::new("waiting_for_human");
    o.log(format!("code-fix task {fname} needs human review — no auto-apply"));
    o
}

/// Build a slug from a task filename. Strips leading priority number + type
/// segment if present (e.g. `2-research-multi-head-attention.md` → `multi-head-attention`).
fn slug_from_filename(fname: &str) -> String {
    let stem = fname.trim_end_matches(".md");
    // Strip leading numeric prefix (e.g. "2-").
    let without_prio = stem.splitn(2, '-').nth(1).unwrap_or(stem);
    // Strip type prefix (ingest/research/report/ontology/code-fix) if it matches.
    for prefix in ["ingest-", "research-", "report-", "ontology-", "code-fix-"] {
        if let Some(rest) = without_prio.strip_prefix(prefix) {
            return rest.to_string();
        }
    }
    without_prio.to_string()
}

/// Backfill sweep: scan `07-neuro-link-task/` for `status: pending` task files
/// older than `min_age`, and dispatch each. Returns the list of dispatched
/// filenames. Filters out templates/examples + completed tasks.
pub fn backfill_pending_tasks(root: &Path, min_age: Duration) -> Result<Vec<String>> {
    let task_dir = root.join("07-neuro-link-task");
    if !task_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut dispatched = Vec::new();
    let now = SystemTime::now();
    for entry in fs::read_dir(&task_dir)? {
        let entry = entry?;
        let path = entry.path();
        let Some(ext) = path.extension() else { continue };
        if ext != "md" {
            continue;
        }
        let Some(fname) = path.file_name().and_then(|s| s.to_str()) else { continue };
        if fname.starts_with("example-") || fname.starts_with('.') || fname.starts_with('_') {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        if let Ok(modified) = meta.modified() {
            if let Ok(age) = now.duration_since(modified) {
                if age < min_age {
                    continue;
                }
            }
        }
        // Only dispatch pending tasks — avoid re-running running/completed/failed.
        let Ok((fm, _body)) = parse_task_file(&path) else { continue };
        let status = fm.get("status").map(String::as_str).unwrap_or("");
        if status != "pending" {
            continue;
        }
        match dispatch_task(root, &path) {
            Ok(outcome) => {
                tracing::info!("backfill dispatched {fname} → {}", outcome.status);
                dispatched.push(fname.to_string());
            }
            Err(e) => {
                tracing::warn!("backfill dispatch failed for {fname}: {e}");
                let _ = append_job_error(root, fname, &format!("backfill: {e}"));
            }
        }
    }
    Ok(dispatched)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_root() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        for dir in ["00-raw", "01-sorted", "02-KB-main", "06-progress-reports",
                    "07-neuro-link-task", "state", "03-ontology-main"] {
            fs::create_dir_all(tmp.path().join(dir)).unwrap();
        }
        tmp
    }

    #[test]
    fn parse_and_render_roundtrip() {
        let tmp = setup_root();
        let path = tmp.path().join("07-neuro-link-task/t.md");
        fs::write(&path, "---\ntype: report\nstatus: pending\npriority: 2\n---\nbody here\n").unwrap();
        let (fm, body) = parse_task_file(&path).unwrap();
        assert_eq!(fm.get("type"), Some(&"report".to_string()));
        assert_eq!(body, "body here\n");
        write_task_file(&path, &fm, &body).unwrap();
        let read = fs::read_to_string(&path).unwrap();
        assert!(read.starts_with("---\ntype: report\nstatus: pending\n"));
        assert!(read.ends_with("body here\n"));
    }

    #[test]
    fn report_task_completes_and_writes_stub() {
        let tmp = setup_root();
        let root = tmp.path();
        let path = root.join("07-neuro-link-task/3-test-job-scanner.md");
        fs::write(
            &path,
            "---\ntype: report\nstatus: pending\npriority: 2\n---\n# hello\n",
        ).unwrap();

        let o = dispatch_task(root, &path).unwrap();
        assert_eq!(o.status, "completed", "outcome: {:?}", o);
        let updated = fs::read_to_string(&path).unwrap();
        assert!(updated.contains("status: completed"));
        assert!(updated.contains("dispatched_at:"));
        assert!(updated.contains("completed_at:"));

        let log = fs::read_to_string(root.join("state/job_log.jsonl")).unwrap();
        assert!(log.contains("task_dispatched"));
        assert!(log.contains("task_completed"));

        // Stub report file created under 06-progress-reports/.
        let entries: Vec<_> = fs::read_dir(root.join("06-progress-reports"))
            .unwrap().filter_map(|e| e.ok()).collect();
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn research_task_enqueues_for_agent() {
        let tmp = setup_root();
        let root = tmp.path();
        let path = root.join("07-neuro-link-task/2-research-topic.md");
        fs::write(
            &path,
            "---\ntype: research\nstatus: pending\npriority: 2\nassigned_harness: claude-code\n---\n# research\n",
        ).unwrap();

        let o = dispatch_task(root, &path).unwrap();
        assert_eq!(o.status, "waiting_for_agent");
        let updated = fs::read_to_string(&path).unwrap();
        assert!(updated.contains("status: waiting_for_agent"));
        let q = fs::read_to_string(root.join("state/agent_queue.jsonl")).unwrap();
        assert!(q.contains("2-research-topic.md"));
    }

    #[test]
    fn code_fix_task_waits_for_human() {
        let tmp = setup_root();
        let root = tmp.path();
        let path = root.join("07-neuro-link-task/5-wiki-frontmatter.md");
        fs::write(
            &path,
            "---\ntype: code-fix\nstatus: pending\npriority: 3\n---\nfix the thing\n",
        ).unwrap();

        let o = dispatch_task(root, &path).unwrap();
        assert_eq!(o.status, "waiting_for_human");
        let updated = fs::read_to_string(&path).unwrap();
        assert!(updated.contains("status: waiting_for_human"));
    }

    #[test]
    fn unknown_type_fails_and_logs() {
        let tmp = setup_root();
        let root = tmp.path();
        let path = root.join("07-neuro-link-task/t.md");
        fs::write(
            &path,
            "---\ntype: martian\nstatus: pending\n---\nbody\n",
        ).unwrap();

        let o = dispatch_task(root, &path).unwrap();
        assert_eq!(o.status, "failed");
        assert!(o.error.as_deref().unwrap_or("").contains("unknown type"));
        let errs = fs::read_to_string(root.join("state/job_errors.jsonl")).unwrap();
        assert!(errs.contains("unknown type"));
    }

    #[test]
    fn template_files_are_skipped() {
        let tmp = setup_root();
        let root = tmp.path();
        let path = root.join("07-neuro-link-task/example-foo.md");
        fs::write(&path, "---\ntype: ingest\nstatus: pending\n---\n# template\n").unwrap();
        let o = dispatch_task(root, &path).unwrap();
        assert_eq!(o.status, "skipped");
    }

    #[test]
    fn slug_helper_strips_priority_and_type() {
        assert_eq!(slug_from_filename("2-research-multi-head.md"), "multi-head");
        assert_eq!(slug_from_filename("3-test-job-scanner.md"), "test-job-scanner");
        assert_eq!(slug_from_filename("no-prefix.md"), "prefix");
    }

    #[tokio::test]
    async fn backfill_skips_recent_and_dispatches_old() {
        let tmp = setup_root();
        let root = tmp.path();
        let path = root.join("07-neuro-link-task/5-old.md");
        fs::write(
            &path,
            "---\ntype: code-fix\nstatus: pending\npriority: 3\n---\nfix me\n",
        ).unwrap();
        // Age it 5s into the past so backfill's 2s threshold picks it up.
        let long_ago = SystemTime::now() - Duration::from_secs(5);
        let file = std::fs::File::open(&path).unwrap();
        let _ = file.set_modified(long_ago);
        drop(file);

        // Recent file — should NOT be dispatched.
        let fresh = root.join("07-neuro-link-task/6-fresh.md");
        fs::write(
            &fresh,
            "---\ntype: code-fix\nstatus: pending\npriority: 3\n---\nfix me\n",
        ).unwrap();

        let dispatched = backfill_pending_tasks(root, Duration::from_secs(2)).unwrap();
        assert!(dispatched.contains(&"5-old.md".to_string()));
        assert!(!dispatched.contains(&"6-fresh.md".to_string()));
    }
}
