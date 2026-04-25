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
    // Gate-44 JJJ3: accept either vault root (canonical vaults/ or
    // legacy 02-KB-main/) so a vault-only fresh checkout can boot
    // without requiring NLR_ROOT to be exported.
    if cwd.join("CLAUDE.md").exists()
        && (cwd.join("vaults").is_dir() || cwd.join("02-KB-main").is_dir())
    {
        return Ok(cwd);
    }
    anyhow::bail!("Cannot resolve NLR_ROOT. Set NLR_ROOT env var or run scripts/init.sh.")
}

/// Default folders the MCP server exposes. Users can customize via config/neuro-link.md
/// by setting `allowed_paths` in YAML frontmatter.
const DEFAULT_ALLOWED_PATHS: &[&str] = &[
    // Gate-44 JJJ3: vaults/ is the canonical knowledge root; allow it
    // by default so vault-only installs don't need user config.
    "vaults",
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

/// Read allowed_paths from config/neuro-link.md frontmatter using
/// real YAML parsing via serde_yaml. Supports every valid spelling:
///   - inline scalar:           `allowed_paths: 00-raw, 02-KB-main`
///   - block sequence:          `allowed_paths:\n  - vaults\n  - 02-KB-main`
///   - flow sequence:           `allowed_paths: [vaults, 02-KB-main]`
///   - quoted "all" or scalar:  `allowed_paths: "all"` / `'02-KB-main'`
///   - empty sequence:          `allowed_paths: []`  → deny all
///   - literal "all":           `allowed_paths: all` → DEFAULT
///
/// Gate-54 TTT1: replaces the hand-rolled line parser (Gate-52 RRR1)
/// with serde_yaml. Hand parser silently mangled flow sequences and
/// inline comments into deny-all outages. Real YAML parsing eliminates
/// that class of bug. On parse failure (malformed YAML, missing
/// frontmatter), prints a single-line warning to stderr and returns
/// DEFAULT — preferring availability for syntax errors over silent
/// lockout, since the operator hasn't expressed an explicit policy.
/// Explicit empty-list (`allowed_paths: []`) is honored as deny-all.
pub fn allowed_paths(root: &Path) -> Vec<String> {
    let config_path = root.join("config/neuro-link.md");
    let content = match std::fs::read_to_string(&config_path) {
        Ok(c) => c,
        Err(_) => return DEFAULT_ALLOWED_PATHS.iter().map(|s| s.to_string()).collect(),
    };
    // Extract the YAML frontmatter block (between leading --- markers).
    let frontmatter: Option<&str> = (|| -> Option<&str> {
        let after_first = content.strip_prefix("---")?;
        let nl = after_first.find('\n')?;
        let body = &after_first[nl + 1..];
        let end = body.find("\n---")?;
        Some(&body[..end])
    })();
    let yaml_text = match frontmatter {
        Some(f) => f,
        None => return DEFAULT_ALLOWED_PATHS.iter().map(|s| s.to_string()).collect(),
    };
    let parsed: serde_yaml::Value = match serde_yaml::from_str(yaml_text) {
        Ok(v) => v,
        Err(e) => {
            eprintln!(
                "[config] WARN: malformed YAML frontmatter in {} ({}); falling back to DEFAULT_ALLOWED_PATHS",
                config_path.display(),
                e
            );
            return DEFAULT_ALLOWED_PATHS.iter().map(|s| s.to_string()).collect();
        }
    };
    let val = match parsed.get("allowed_paths") {
        Some(v) => v,
        None => return DEFAULT_ALLOWED_PATHS.iter().map(|s| s.to_string()).collect(),
    };
    match val {
        serde_yaml::Value::String(s) => {
            let trimmed = s.trim();
            if trimmed.eq_ignore_ascii_case("all") {
                return DEFAULT_ALLOWED_PATHS.iter().map(|s| s.to_string()).collect();
            }
            // Inline comma-separated scalar fallback (legacy form).
            trimmed
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        }
        serde_yaml::Value::Sequence(items) => items
            .iter()
            .filter_map(|v| match v {
                serde_yaml::Value::String(s) => Some(s.trim().to_string()),
                _ => None,
            })
            .filter(|s| !s.is_empty())
            .collect(),
        _ => {
            eprintln!(
                "[config] WARN: allowed_paths in {} has unsupported value type; falling back to DEFAULT_ALLOWED_PATHS",
                config_path.display()
            );
            DEFAULT_ALLOWED_PATHS.iter().map(|s| s.to_string()).collect()
        }
    }
}

/// Check if a relative path (from NLR_ROOT) is within an allowed directory.
pub fn is_path_allowed(root: &Path, rel_path: &str) -> bool {
    let allowed = allowed_paths(root);
    // Gate-53 SSS1: empty allowlist means DENY ALL, not allow-all.
    // RRR1's parser intentionally returns empty for malformed/empty
    // YAML sequences as a fail-closed signal — but is_path_allowed
    // was still treating empty as allow-everything, defeating the
    // safety property entirely. The explicit "all" case is now
    // represented by allowed_paths returning DEFAULT_ALLOWED_PATHS
    // (non-empty), so an empty Vec is unambiguously deny-all.
    if allowed.is_empty() {
        return false;
    }
    // Gate-45 KKK2: match the FIRST PATH SEGMENT, not raw string
    // prefix. Prior `starts_with` let `vaults-private/...` or
    // `02-KB-main-archive/...` slip through under the default
    // allowlist because the first allowed entry's name was a string
    // prefix of those siblings. Now `vaults` matches `vaults` and
    // `vaults/...` exactly, never `vaults-private/...`.
    let first_segment = rel_path
        .trim_start_matches('/')
        .split(['/', std::path::MAIN_SEPARATOR])
        .next()
        .unwrap_or("");
    for allowed_dir in &allowed {
        if first_segment == allowed_dir.as_str() {
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
