use anyhow::Result;
use serde::Serialize;
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

#[derive(Debug, Serialize)]
pub struct HealthStatus {
    pub status: String,
    pub errors: Vec<String>,
    pub pending_tasks: usize,
    pub wiki_pages: usize,
    pub state_files: Vec<String>,
}

pub fn check_health(root: &Path) -> Result<HealthStatus> {
    let mut errors = Vec::new();

    for d in [
        "00-raw",
        "01-sorted",
        "02-KB-main",
        "07-neuro-link-task",
        "config",
        "state",
    ] {
        if !root.join(d).is_dir() {
            errors.push(format!("Missing dir: {d}"));
        }
    }
    for f in ["CLAUDE.md", "02-KB-main/schema.md", "config/neuro-link.md"] {
        if !root.join(f).exists() {
            errors.push(format!("Missing file: {f}"));
        }
    }

    let task_dir = root.join("07-neuro-link-task");
    let mut pending = 0;
    if task_dir.is_dir() {
        for entry in fs::read_dir(&task_dir)? {
            let path = entry?.path();
            if path.extension().is_some_and(|e| e == "md") {
                if fs::read_to_string(&path)
                    .unwrap_or_default()
                    .contains("status: pending")
                {
                    pending += 1;
                }
            }
        }
    }

    let skip = ["schema.md", "index.md", "log.md"];
    let wiki_pages = WalkDir::new(root.join("02-KB-main"))
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().extension().is_some_and(|x| x == "md")
                && !skip
                    .iter()
                    .any(|s| e.file_name().to_string_lossy() == *s)
        })
        .count();

    let state_dir = root.join("state");
    let state_files = if state_dir.is_dir() {
        fs::read_dir(&state_dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect()
    } else {
        Vec::new()
    };

    let status = if errors.is_empty() {
        "ok".into()
    } else {
        "error".into()
    };

    Ok(HealthStatus {
        status,
        errors,
        pending_tasks: pending,
        wiki_pages,
        state_files,
    })
}

pub async fn run_daemon(root: &Path, interval_secs: u64) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
    loop {
        interval.tick().await;
        match check_health(root) {
            Ok(health) => {
                let hb = serde_json::json!({
                    "status": health.status,
                    "last_check": chrono::Utc::now().to_rfc3339(),
                    "errors": health.errors,
                    "pending_tasks": health.pending_tasks,
                    "wiki_pages": health.wiki_pages,
                });
                let _ = fs::write(
                    root.join("state/heartbeat.json"),
                    serde_json::to_string_pretty(&hb).unwrap_or_default(),
                );
                eprintln!(
                    "[heartbeat] status={} pending={} wiki={}",
                    health.status, health.pending_tasks, health.wiki_pages
                );
            }
            Err(e) => eprintln!("[heartbeat] error: {e}"),
        }
    }
}
