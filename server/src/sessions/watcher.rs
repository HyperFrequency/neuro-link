// Periodic session-log watcher. Polls ~/.claude/projects/ every 30s and re-parses
// any session whose file has been modified since last poll.
// Simpler + more reliable than filesystem notification for JSONL append semantics.

use super::*;
use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use tokio::time;
use walkdir::WalkDir;

pub async fn run(claude_root: PathBuf, vault_path: PathBuf, interval_sec: u64) -> Result<()> {
    eprintln!("Session watcher starting");
    eprintln!("  Source:   {}", claude_root.join("projects").display());
    eprintln!("  Vault:    {}", vault_path.display());
    eprintln!("  Interval: {}s", interval_sec);

    let mut last_mtimes: HashMap<PathBuf, SystemTime> = HashMap::new();
    let projects = claude_root.join("projects");

    loop {
        if !projects.is_dir() {
            eprintln!("[watcher] projects dir missing, sleeping");
            time::sleep(Duration::from_secs(interval_sec)).await;
            continue;
        }

        let mut updated_count = 0;
        for entry in WalkDir::new(&projects).into_iter().filter_map(|e| e.ok()) {
            let p = entry.path();
            if !p.extension().is_some_and(|e| e == "jsonl") { continue; }
            let mtime = match p.metadata().and_then(|m| m.modified()) {
                Ok(t) => t,
                Err(_) => continue,
            };
            let path_buf = p.to_path_buf();
            let changed = match last_mtimes.get(&path_buf) {
                Some(prev) => mtime > *prev,
                None => true, // first time seeing this file
            };
            if !changed { continue; }

            last_mtimes.insert(path_buf.clone(), mtime);

            // Re-parse + write
            match parser::parse_session(p) {
                Ok(mut summary) => {
                    summary.quality_flags = quality::analyze(&summary);
                    if let Err(e) = markdown::write_session_markdown(&summary, &vault_path) {
                        eprintln!("[watcher] write failed for {}: {e}", p.display());
                    } else {
                        updated_count += 1;
                    }
                }
                Err(e) => eprintln!("[watcher] parse failed for {}: {e}", p.display()),
            }
        }

        if updated_count > 0 {
            eprintln!(
                "[{}] Updated {} session markdown files",
                chrono::Utc::now().format("%H:%M:%S"),
                updated_count
            );
        }

        time::sleep(Duration::from_secs(interval_sec)).await;
    }
}
