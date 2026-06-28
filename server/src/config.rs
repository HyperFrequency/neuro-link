//! Config resolution and YAML frontmatter parsing.

use anyhow::{Context, Result};
use regex::Regex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub fn resolve_nlr_root() -> Result<PathBuf> {
    if let Ok(val) = std::env::var("NLR_ROOT") {
        let p = PathBuf::from(&val);
        if p.is_dir() { return Ok(p); }
    }
    if let Ok(home) = std::env::var("HOME") {
        let root_file = PathBuf::from(&home).join(".claude/state/nlr_root");
        if root_file.exists() {
            let content = std::fs::read_to_string(&root_file).context("Reading nlr_root")?;
            let p = PathBuf::from(content.trim());
            if p.is_dir() { return Ok(p); }
        }
    }
    let cwd = std::env::current_dir()?;
    if cwd.join("CLAUDE.md").exists() && cwd.join("02-KB-main").is_dir() {
        return Ok(cwd);
    }
    anyhow::bail!("Cannot resolve NLR_ROOT. Set NLR_ROOT env var or run scripts/init.sh.")
}

/// Default folders the MCP server exposes. Users can customize via config/neuro-link.md
/// by setting `allowed_paths` in YAML frontmatter.
const DEFAULT_ALLOWED_PATHS: &[&str] = &[
    "00-raw",
    "01-sorted",
    "02-KB-main",
    "03-ontology-main",
    "04-KB-agents-workflows",
    "05-insights-gaps",
    "05-self-improvement-HITL",
    "06-self-improvement-recursive",
    "06-progress-reports",
    "07-neuro-link-task",
    "08-code-docs",
    "09-business-docs",
    "config",
];

/// Read allowed_paths from config/neuro-link.md, defaulting to all KB folders.
pub fn allowed_paths(root: &Path) -> Vec<String> {
    let config_path = root.join("config/neuro-link.md");
    if let Ok(content) = std::fs::read_to_string(&config_path) {
        // Parse allowed_paths from YAML frontmatter (comma-separated or YAML list)
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("allowed_paths:") {
                let val = trimmed.strip_prefix("allowed_paths:").unwrap_or("").trim();
                if !val.is_empty() && val != "all" {
                    // Comma-separated: "00-raw, 02-KB-main, 07-neuro-link-task"
                    return val.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                }
            }
        }
    }
    DEFAULT_ALLOWED_PATHS.iter().map(|s| s.to_string()).collect()
}

/// Check if a relative path (from NLR_ROOT) is within an allowed directory.
pub fn is_path_allowed(root: &Path, rel_path: &str) -> bool {
    let allowed = allowed_paths(root);
    // "all" or empty means everything
    if allowed.is_empty() {
        return true;
    }
    for prefix in &allowed {
        if rel_path.starts_with(prefix.as_str()) {
            return true;
        }
    }
    false
}

/// Diagnostic snapshot of how NLR_ROOT would resolve.
///
/// - `env`     — current `NLR_ROOT` environment variable (empty if unset)
/// - `file`    — contents of `~/.claude/state/nlr_root` (empty if missing or unreadable)
/// - `chosen`  — the path the resolver would actually pick, or None if none resolves
/// - `dir_exists` — whether `chosen` is an existing directory
/// - `mismatch`   — true if both `env` and `file` are set and differ (non-empty, non-equal)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NlrRootDiag {
    pub env: String,
    pub file: String,
    pub chosen: Option<PathBuf>,
    pub dir_exists: bool,
    pub mismatch: bool,
}

/// Pure helper: analyze env + file-contents pair and return the diagnostic.
///
/// `file` is the raw content (or empty string) of `~/.claude/state/nlr_root`.
/// Uses the same precedence as [`resolve_nlr_root`]: env wins, file is fallback.
///
/// The `dir_check` closure decides whether a path exists on disk; in tests this
/// is a stub, in production callers use the wrapper [`diagnose_nlr_root`] which
/// checks the real filesystem.
pub fn analyze_nlr_root(env: &str, file: &str, dir_check: impl Fn(&Path) -> bool) -> NlrRootDiag {
    let env_trim = env.trim();
    let file_trim = file.trim();
    let chosen: Option<PathBuf> = if !env_trim.is_empty() && dir_check(Path::new(env_trim)) {
        Some(PathBuf::from(env_trim))
    } else if !file_trim.is_empty() && dir_check(Path::new(file_trim)) {
        Some(PathBuf::from(file_trim))
    } else {
        None
    };
    let dir_exists = chosen
        .as_deref()
        .map(|p| dir_check(p))
        .unwrap_or(false);
    let mismatch = !env_trim.is_empty() && !file_trim.is_empty() && env_trim != file_trim;
    NlrRootDiag {
        env: env_trim.to_string(),
        file: file_trim.to_string(),
        chosen,
        dir_exists,
        mismatch,
    }
}

/// Production wrapper: reads env, reads `~/.claude/state/nlr_root`, checks
/// directories on disk.
pub fn diagnose_nlr_root() -> NlrRootDiag {
    let env = std::env::var("NLR_ROOT").unwrap_or_default();
    let file = std::env::var("HOME")
        .ok()
        .map(|h| PathBuf::from(h).join(".claude/state/nlr_root"))
        .and_then(|p| std::fs::read_to_string(&p).ok())
        .unwrap_or_default();
    analyze_nlr_root(&env, &file, |p| p.is_dir())
}

pub fn parse_frontmatter(path: &Path) -> Result<HashMap<String, String>> {
    let content = std::fs::read_to_string(path)?;
    let re = Regex::new(r"(?s)^---\n(.+?)\n---")?;
    let caps = re.captures(&content).context("No frontmatter found")?;
    let yaml_str = &caps[1];
    let mut map = HashMap::new();
    for line in yaml_str.lines() {
        if let Some((key, val)) = line.split_once(':') {
            let k = key.trim().to_string();
            let v = val.trim().trim_matches('"').to_string();
            if !k.is_empty() && !v.is_empty() { map.insert(k, v); }
        }
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyze_env_only_resolves_when_dir_exists() {
        let d = analyze_nlr_root("/tmp/nlr-a", "", |p| p == Path::new("/tmp/nlr-a"));
        assert_eq!(d.env, "/tmp/nlr-a");
        assert_eq!(d.file, "");
        assert_eq!(d.chosen.as_deref(), Some(Path::new("/tmp/nlr-a")));
        assert!(d.dir_exists);
        assert!(!d.mismatch);
    }

    #[test]
    fn analyze_file_fallback_when_env_unset() {
        let d = analyze_nlr_root("", "/tmp/nlr-b\n", |p| p == Path::new("/tmp/nlr-b"));
        assert_eq!(d.file, "/tmp/nlr-b");
        assert_eq!(d.chosen.as_deref(), Some(Path::new("/tmp/nlr-b")));
        assert!(d.dir_exists);
        assert!(!d.mismatch);
    }

    #[test]
    fn analyze_env_wins_when_both_set_and_valid() {
        let dirs = |p: &Path| p == Path::new("/a") || p == Path::new("/b");
        let d = analyze_nlr_root("/a", "/b", dirs);
        assert_eq!(d.chosen.as_deref(), Some(Path::new("/a")));
        assert!(d.dir_exists);
        assert!(d.mismatch, "env and file differ => mismatch=true");
    }

    #[test]
    fn analyze_matching_env_and_file_is_not_mismatch() {
        let d = analyze_nlr_root("/x", "/x", |p| p == Path::new("/x"));
        assert!(!d.mismatch);
    }

    #[test]
    fn analyze_nothing_resolves_when_both_missing_on_disk() {
        let d = analyze_nlr_root("/no1", "/no2", |_| false);
        assert_eq!(d.chosen, None);
        assert!(!d.dir_exists);
        assert!(d.mismatch);
    }

    #[test]
    fn analyze_empty_inputs_yield_no_resolution_no_mismatch() {
        let d = analyze_nlr_root("", "", |_| true);
        assert_eq!(d.env, "");
        assert_eq!(d.file, "");
        assert_eq!(d.chosen, None);
        assert!(!d.dir_exists);
        assert!(!d.mismatch);
    }
}
