use anyhow::{bail, Result};
use regex::Regex;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

pub fn tool_defs() -> Vec<Value> {
    vec![
        json!({"name":"nlr_wiki_create","description":"Create a new wiki page in 02-KB-main/","inputSchema":{"type":"object","properties":{"path":{"type":"string","description":"Relative path under 02-KB-main/ (e.g. trading/market-microstructure.md)"},"title":{"type":"string"},"domain":{"type":"string"},"content":{"type":"string"},"confidence":{"type":"string","enum":["high","medium","low","contested"]}},"required":["path","title","content"]}}),
        json!({"name":"nlr_wiki_read","description":"Read a wiki page by path","inputSchema":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}}),
        json!({"name":"nlr_wiki_update","description":"Update an existing wiki page","inputSchema":{"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"},"append":{"type":"boolean"}},"required":["path","content"]}}),
        json!({"name":"nlr_wiki_list","description":"List all wiki pages with metadata","inputSchema":{"type":"object","properties":{}}}),
        json!({"name":"nlr_wiki_search","description":"Search wiki pages by keyword","inputSchema":{"type":"object","properties":{"query":{"type":"string"}},"required":["query"]}}),
    ]
}

/// Split caller-supplied content into (caller_frontmatter_yaml, body).
///
/// If the content begins with a `---\n...\n---\n` block, the inner text is
/// returned as the first tuple element and the remaining body as the second.
/// Otherwise returns (empty, content).
///
/// A-fu4: the server used to always prepend its own frontmatter, so a caller
/// passing a pre-rendered markdown page would produce two stacked frontmatter
/// blocks and break Obsidian rendering.
fn split_caller_frontmatter(content: &str) -> (String, String) {
    let trimmed = content.trim_start();
    let Some(after_open) = trimmed.strip_prefix("---\n") else {
        return (String::new(), content.to_string());
    };
    // Find closing fence `\n---\n` (or trailing `\n---` at EOF).
    if let Some(close) = after_open.find("\n---\n") {
        let yaml = after_open[..close].to_string();
        let body = after_open[close + 5..].to_string();
        (yaml, body)
    } else if let Some(close) = after_open.find("\n---") {
        let yaml = after_open[..close].to_string();
        let body = after_open[close + 4..].trim_start_matches('\n').to_string();
        (yaml, body)
    } else {
        (String::new(), content.to_string())
    }
}

fn parse_yaml_fields(yaml: &str) -> std::collections::BTreeMap<String, String> {
    let mut map = std::collections::BTreeMap::new();
    for line in yaml.lines() {
        if let Some((k, v)) = line.split_once(':') {
            let key = k.trim().to_string();
            let val = v.trim().trim_matches('"').to_string();
            if !key.is_empty() {
                map.insert(key, val);
            }
        }
    }
    map
}

fn validate_wiki_path(kb: &Path, rel: &str) -> Result<std::path::PathBuf> {
    if rel.contains("..") || rel.starts_with('/') || rel.contains('\0') {
        bail!("Invalid path: traversal not allowed");
    }
    let full = kb.join(rel);
    // For existing files, canonicalize and verify
    if full.exists() {
        let canonical = full.canonicalize()?;
        let kb_canonical = kb.canonicalize().unwrap_or_else(|_| kb.to_path_buf());
        if !canonical.starts_with(&kb_canonical) {
            bail!("Access denied: path outside knowledge base");
        }
        return Ok(canonical);
    }
    // For new files, verify the parent is under kb
    Ok(full)
}

pub fn call(name: &str, args: &Value, root: &Path) -> Result<String> {
    let kb = root.join("02-KB-main");
    match name {
        "nlr_wiki_create" => {
            // Required args: path, title, content. Reject empty/missing — previously
            // this silently wrote 02-KB-main/untitled.md with empty body (surfaced by
            // heavy-testing-suite negative tests).
            let rel = args.get("path").and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .ok_or_else(|| anyhow::anyhow!("nlr_wiki_create: 'path' is required (non-empty string)"))?;
            let title = args.get("title").and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .ok_or_else(|| anyhow::anyhow!("nlr_wiki_create: 'title' is required (non-empty string)"))?;
            let content = args.get("content").and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .ok_or_else(|| anyhow::anyhow!("nlr_wiki_create: 'content' is required (non-empty string)"))?;
            let domain = args.get("domain").and_then(|v| v.as_str()).unwrap_or("general");
            let confidence = args.get("confidence").and_then(|v| v.as_str()).unwrap_or("medium");

            let path = validate_wiki_path(&kb, rel)?;
            if let Some(parent) = path.parent() { fs::create_dir_all(parent)?; }

            // A-fu4: if caller already wrapped the content in `--- ... ---`, merge
            // their fields with ours instead of stacking two frontmatter blocks.
            let (caller_yaml, body) = split_caller_frontmatter(content);
            let caller_fields = parse_yaml_fields(&caller_yaml);
            let body_sha = hex::encode(Sha256::digest(body.as_bytes()));
            let now = chrono::Utc::now().format("%Y-%m-%d").to_string();

            let merged_title = caller_fields.get("title").cloned().filter(|s| !s.is_empty()).unwrap_or_else(|| title.to_string());
            let merged_domain = caller_fields.get("domain").cloned().filter(|s| !s.is_empty()).unwrap_or_else(|| domain.to_string());
            let merged_confidence = caller_fields.get("confidence").cloned().filter(|s| !s.is_empty()).unwrap_or_else(|| confidence.to_string());

            let mut fm = format!("---\ntitle: {merged_title}\ndomain: {merged_domain}\nconfidence: {merged_confidence}\nlast_updated: {now}\nsha256: {body_sha}\n");
            // Preserve any extra caller fields (anything we didn't handle above) so
            // custom metadata isn't silently dropped.
            for (k, v) in &caller_fields {
                if !["title", "domain", "confidence", "last_updated", "sha256"].contains(&k.as_str()) {
                    fm.push_str(&format!("{k}: {v}\n"));
                }
            }
            fm.push_str("---\n\n");
            let page = format!("{fm}{}\n", body.trim_end());
            let sha = body_sha.clone();
            fs::write(&path, &page)?;

            // Append to log
            let log = kb.join("log.md");
            let entry = format!("\n## [{now}] Created: {rel}\n- Confidence: {confidence}\n- SHA256: {sha}\n");
            fs::OpenOptions::new().append(true).open(&log).map(|mut f| { use std::io::Write; let _ = write!(f, "{entry}"); })?;

            Ok(format!("Created: {rel}"))
        }
        "nlr_wiki_read" => {
            let rel = args["path"].as_str().unwrap_or("");
            let path = validate_wiki_path(&kb, rel)?;
            if !path.exists() { bail!("Page not found: {rel}"); }
            Ok(fs::read_to_string(&path)?)
        }
        "nlr_wiki_update" => {
            let rel = args["path"].as_str().unwrap_or("");
            let content = args["content"].as_str().unwrap_or("");
            let append = args.get("append").and_then(|v| v.as_bool()).unwrap_or(false);
            let path = validate_wiki_path(&kb, rel)?;
            if !path.exists() { bail!("Page not found: {rel}"); }
            if append {
                let mut existing = fs::read_to_string(&path)?;
                existing.push_str("\n");
                existing.push_str(content);
                fs::write(&path, existing)?;
            } else {
                fs::write(&path, content)?;
            }
            Ok(format!("Updated: {rel}"))
        }
        "nlr_wiki_list" => {
            let skip = ["schema.md", "index.md", "log.md"];
            let mut pages = Vec::new();
            for entry in WalkDir::new(&kb).into_iter().filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "md") && !skip.iter().any(|s| path.file_name().is_some_and(|f| f == *s)) {
                    let rel = path.strip_prefix(&kb).unwrap_or(path).display().to_string();
                    let content = fs::read_to_string(path).unwrap_or_default();
                    let title = extract_field(&content, "title").unwrap_or(rel.clone());
                    let domain = extract_field(&content, "domain").unwrap_or_default();
                    let confidence = extract_field(&content, "confidence").unwrap_or_default();
                    pages.push(json!({"path": rel, "title": title, "domain": domain, "confidence": confidence}));
                }
            }
            Ok(serde_json::to_string_pretty(&pages)?)
        }
        "nlr_wiki_search" => {
            let query = args["query"].as_str().unwrap_or("").to_lowercase();
            let skip = ["schema.md", "index.md", "log.md"];
            let mut hits = Vec::new();
            for entry in WalkDir::new(&kb).into_iter().filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "md") && !skip.iter().any(|s| path.file_name().is_some_and(|f| f == *s)) {
                    let content = fs::read_to_string(path).unwrap_or_default();
                    if content.to_lowercase().contains(&query) {
                        let rel = path.strip_prefix(&kb).unwrap_or(path).display().to_string();
                        let title = extract_field(&content, "title").unwrap_or(rel.clone());
                        hits.push(json!({"path": rel, "title": title}));
                        if hits.len() >= 10 { break; }
                    }
                }
            }
            Ok(serde_json::to_string_pretty(&hits)?)
        }
        _ => bail!("Unknown wiki tool: {name}"),
    }
}

fn extract_field(content: &str, field: &str) -> Option<String> {
    let re = Regex::new(&format!(r"{}:\s*(.+)", regex::escape(field))).ok()?;
    re.captures(content).map(|c| c[1].trim().trim_matches('"').to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_kb() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("02-KB-main")).unwrap();
        // create log.md (append requires it exist)
        fs::write(tmp.path().join("02-KB-main/log.md"), "# Wiki log\n").unwrap();
        tmp
    }

    #[test]
    fn create_without_caller_frontmatter_single_block() {
        let tmp = setup_kb();
        let args = json!({
            "path": "test/plain.md",
            "title": "Plain",
            "content": "Body only, no frontmatter.",
        });
        call("nlr_wiki_create", &args, tmp.path()).unwrap();
        let got = fs::read_to_string(tmp.path().join("02-KB-main/test/plain.md")).unwrap();
        assert_eq!(got.matches("---\n").count(), 2, "exactly one frontmatter block");
        assert!(got.contains("title: Plain"));
        assert!(got.ends_with("Body only, no frontmatter.\n"));
    }

    #[test]
    fn create_with_caller_frontmatter_merges_instead_of_stacking() {
        let tmp = setup_kb();
        let caller = "---\ntitle: Caller Title\ndomain: math\ncustom_field: keep-me\n---\n\nReal body here.\n";
        let args = json!({
            "path": "test/merged.md",
            "title": "Server Title",
            "content": caller,
            "domain": "ignored-because-caller-has-it",
        });
        call("nlr_wiki_create", &args, tmp.path()).unwrap();
        let got = fs::read_to_string(tmp.path().join("02-KB-main/test/merged.md")).unwrap();
        assert_eq!(got.matches("---\n").count(), 2, "no stacked frontmatter");
        assert!(got.contains("title: Caller Title"), "caller title wins");
        assert!(got.contains("domain: math"), "caller domain wins");
        assert!(got.contains("custom_field: keep-me"), "extra caller field preserved");
        assert!(got.contains("Real body here."));
        assert!(!got.contains("Server Title"), "server-supplied title is overridden by caller fm");
    }
}
