//! P08: `neuro-link curate-backlog` — drain the oldest N entries from
//! `state/curation_queue.jsonl` through `claude --print` invocations of the
//! `/wiki-curate <slug>` skill.

use anyhow::{bail, Result};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct CurateReport {
    pub attempted: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub remaining: usize,
}

/// Abstraction so the unit test can inject an `echo`-backed subprocess.
pub trait Curator {
    fn curate(&self, slug: &str) -> Result<String>;
}

pub struct ClaudeCurator;

impl Curator for ClaudeCurator {
    fn curate(&self, slug: &str) -> Result<String> {
        let out = Command::new("claude")
            .args([
                "--print",
                "--model",
                "opus",
                "--effort",
                "high",
                &format!("/wiki-curate {slug}"),
            ])
            .output()?;
        if !out.status.success() {
            bail!(
                "claude exited with {}: {}",
                out.status,
                String::from_utf8_lossy(&out.stderr)
            );
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }
}

fn queue_path(root: &Path) -> PathBuf {
    root.join("state").join("curation_queue.jsonl")
}

fn slug_of(line: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    v.get("slug").and_then(|s| s.as_str()).map(|s| s.to_string())
}

pub fn drain<C: Curator>(root: &Path, limit: usize, curator: &C) -> Result<CurateReport> {
    let path = queue_path(root);
    if !path.exists() {
        return Ok(CurateReport::default());
    }
    let content = fs::read_to_string(&path)?;
    let lines: Vec<String> = content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|s| s.to_string())
        .collect();
    if lines.is_empty() {
        return Ok(CurateReport::default());
    }

    let mut report = CurateReport {
        remaining: lines.len(),
        ..Default::default()
    };

    // Oldest-first: lines are appended on enqueue, so the head of the file
    // is the oldest. Take up to `limit` from the front.
    let take_n = limit.min(lines.len());
    let (head, tail) = lines.split_at(take_n);

    let mut surviving: Vec<String> = tail.to_vec();

    for line in head {
        report.attempted += 1;
        let slug = match slug_of(line) {
            Some(s) => s,
            None => {
                // Malformed entry: drop it.
                report.failed += 1;
                continue;
            }
        };
        match curator.curate(&slug) {
            Ok(_) => report.succeeded += 1,
            Err(e) => {
                eprintln!("[curate-backlog] {slug} failed: {e}");
                report.failed += 1;
                // On failure, keep the line in the queue for a later retry.
                surviving.insert(report.failed - 1, line.clone());
            }
        }
    }

    // Atomic tmp+rename write of the surviving queue.
    let tmp = path.with_extension("jsonl.tmp");
    {
        let mut f = fs::File::create(&tmp)?;
        for l in &surviving {
            writeln!(f, "{l}")?;
        }
    }
    fs::rename(&tmp, &path)?;
    report.remaining = surviving.len();
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct OkCurator;
    impl Curator for OkCurator {
        fn curate(&self, slug: &str) -> Result<String> {
            // Simulate `echo` subprocess succeeding.
            Ok(format!("curated {slug}"))
        }
    }

    struct FailCurator;
    impl Curator for FailCurator {
        fn curate(&self, _slug: &str) -> Result<String> {
            bail!("simulated failure");
        }
    }

    fn seed_queue(root: &Path, entries: &[&str]) {
        fs::create_dir_all(root.join("state")).unwrap();
        let path = queue_path(root);
        let mut f = fs::File::create(path).unwrap();
        for slug in entries {
            let line = serde_json::json!({
                "ts": "2026-01-01T00:00:00Z",
                "slug": slug,
                "domain": "docs",
            });
            writeln!(f, "{}", line).unwrap();
        }
    }

    #[test]
    fn drain_shrinks_queue_on_success() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        seed_queue(root, &["a", "b", "c"]);

        let report = drain(root, 2, &OkCurator).unwrap();
        assert_eq!(report.attempted, 2);
        assert_eq!(report.succeeded, 2);
        assert_eq!(report.failed, 0);
        assert_eq!(report.remaining, 1);

        let remaining = fs::read_to_string(queue_path(root)).unwrap();
        assert!(remaining.contains("\"slug\":\"c\""));
        assert!(!remaining.contains("\"slug\":\"a\""));
        assert!(!remaining.contains("\"slug\":\"b\""));
    }

    #[test]
    fn drain_retains_failed_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        seed_queue(root, &["x", "y"]);

        let report = drain(root, 2, &FailCurator).unwrap();
        assert_eq!(report.attempted, 2);
        assert_eq!(report.failed, 2);
        assert_eq!(report.succeeded, 0);
        // Both lines survive on failure.
        assert_eq!(report.remaining, 2);
        let remaining = fs::read_to_string(queue_path(root)).unwrap();
        assert!(remaining.contains("\"slug\":\"x\""));
        assert!(remaining.contains("\"slug\":\"y\""));
    }

    #[test]
    fn drain_on_empty_queue_is_noop() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let report = drain(root, 5, &OkCurator).unwrap();
        assert_eq!(report, CurateReport::default());
    }
}
