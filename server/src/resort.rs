//! P07: `neuro-link resort` — walk `00-raw/*/source.md` and classify any
//! slugs that don't yet have a `.classified` marker, via
//! `nlr_ingest_classify`.

use anyhow::Result;
use serde_json::json;
use std::fs;
use std::path::Path;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ResortReport {
    pub moved: usize,
    pub skipped: usize,
}

pub fn resort(root: &Path) -> Result<ResortReport> {
    let raw = root.join("00-raw");
    let mut report = ResortReport::default();
    if !raw.is_dir() {
        return Ok(report);
    }

    for entry in fs::read_dir(&raw)? {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let source = path.join("source.md");
        if !source.is_file() {
            continue;
        }
        let slug = match path.file_name().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        if path.join(".classified").exists() {
            report.skipped += 1;
            continue;
        }
        let content = fs::read_to_string(&source).unwrap_or_default();
        let domain = crate::tools::ingest::classify_slug_content(&content);
        let args = json!({ "slug": slug, "domain": domain });
        match crate::tools::ingest::call("nlr_ingest_classify", &args, root) {
            Ok(_) => report.moved += 1,
            Err(e) => {
                eprintln!("[resort] classify failed for {slug}: {e}");
            }
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scaffold(tmp: &Path) {
        for d in ["00-raw", "01-sorted", "02-KB-main"] {
            fs::create_dir_all(tmp.join(d)).unwrap();
        }
    }

    #[test]
    fn resort_classifies_only_unclassified_slugs() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        scaffold(root);

        // Two unclassified slugs + one already-classified slug.
        for slug in ["a-rust", "b-scipy"] {
            let dir = root.join("00-raw").join(slug);
            fs::create_dir_all(&dir).unwrap();
            let body = if slug.contains("rust") {
                "rust cargo tokio serde"
            } else {
                "numpy scipy matplotlib jupyter"
            };
            fs::write(dir.join("source.md"), body).unwrap();
        }
        // Third slug: already classified.
        let done = root.join("00-raw").join("c-done");
        fs::create_dir_all(&done).unwrap();
        fs::write(done.join("source.md"), "already classified").unwrap();
        fs::write(done.join(".classified"), "2024-01-01T00:00:00Z\n").unwrap();
        // And it already lives in 01-sorted/ to satisfy the idempotent path.
        let sorted_dir = root.join("01-sorted").join("docs");
        fs::create_dir_all(&sorted_dir).unwrap();
        fs::write(sorted_dir.join("c-done.md"), "already classified").unwrap();

        let report = resort(root).unwrap();
        assert_eq!(report.moved, 2, "expected 2 unclassified slugs to move");
        assert_eq!(report.skipped, 1, "expected the already-classified slug to be skipped");

        // The two unclassified slugs now have markers.
        assert!(root.join("00-raw/a-rust/.classified").exists());
        assert!(root.join("00-raw/b-scipy/.classified").exists());
    }
}
