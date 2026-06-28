use anyhow::{bail, Context, Result};
use chrono::Utc;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Default render resolution for full-page screenshots of illustration pages.
const RENDER_DPI: &str = "150";
/// Minimum embedded image size (bytes) to classify a page as "has illustrations".
const MIN_ILLUSTRATION_IMAGE_BYTES: u64 = 100 * 1024;

fn classify_pdf_content(text: &str) -> &'static str {
    let lower = text.to_lowercase();

    // Explicit override: meta-harness / optimizer / benchmark / LLM / agent
    // style papers have been mislabelled as "math" because they mention
    // theorems in passing. If any of these markers hit, route to ml-nn
    // regardless of other keyword counts.
    let ml_override = ["harness", "optimizer", "benchmark", "llm", "agent"];
    if ml_override.iter().any(|kw| lower.contains(kw)) {
        return "ml-nn";
    }

    // Otherwise: count keyword hits per domain and pick the winner.
    // Tie (multiple domains with the same max count) or all-zero → "docs".
    let domains: &[(&str, &[&str])] = &[
        ("math", &["theorem", "lemma", "proof", "corollary", "manifold", "topology"]),
        ("quant", &["portfolio", "sharpe", "backtest", "alpha", "factor", "volatility"]),
        ("ml-nn", &["neural", "transformer", "gradient", "attention", "embedding"]),
        ("software-engineering", &["rust", "cargo", "tokio", "serde", "async", "trait"]),
    ];

    let scored: Vec<(&'static str, usize)> = domains
        .iter()
        .map(|(name, kws)| {
            let hits: usize = kws.iter().map(|kw| lower.matches(kw).count()).sum();
            (*name, hits)
        })
        .collect();

    let max_hits = scored.iter().map(|(_, h)| *h).max().unwrap_or(0);
    if max_hits == 0 {
        return "docs";
    }
    let top: Vec<&'static str> = scored
        .iter()
        .filter(|(_, h)| *h == max_hits)
        .map(|(n, _)| *n)
        .collect();
    if top.len() != 1 {
        "docs"
    } else {
        top[0]
    }
}

fn slug_from_path(path: &Path) -> Result<String> {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .context("pdf path missing file stem")?;
    Ok(stem
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-"))
}

fn run_cmd(program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("failed to spawn {program}"))?;
    if !output.status.success() {
        bail!(
            "{program} exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn extract_text(pdf: &Path) -> Result<String> {
    run_cmd("pdftotext", &["-layout", pdf.to_str().unwrap_or(""), "-"])
}

fn count_pages(pdf: &Path) -> Result<usize> {
    let info = run_cmd("pdfinfo", &[pdf.to_str().unwrap_or("")])?;
    for line in info.lines() {
        if let Some(rest) = line.strip_prefix("Pages:") {
            return Ok(rest.trim().parse().unwrap_or(0));
        }
    }
    Ok(0)
}

/// Extract embedded images via pdfimages, returning list of produced files.
fn extract_embedded_images(pdf: &Path, out_dir: &Path, prefix: &str) -> Result<Vec<PathBuf>> {
    fs::create_dir_all(out_dir)?;
    let prefix_path = out_dir.join(prefix);
    let _ = run_cmd(
        "pdfimages",
        &[
            "-png",
            pdf.to_str().unwrap_or(""),
            prefix_path.to_str().unwrap_or(""),
        ],
    )?;
    let mut out = Vec::new();
    for entry in fs::read_dir(out_dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with(prefix) && name_str.ends_with(".png") {
            out.push(entry.path());
        }
    }
    out.sort();
    Ok(out)
}

/// Render specific pages as PNG screenshots (for illustration-heavy pages).
fn render_pages(pdf: &Path, out_dir: &Path, prefix: &str, pages: &[usize]) -> Result<Vec<PathBuf>> {
    fs::create_dir_all(out_dir)?;
    let mut rendered = Vec::new();
    let prefix_path = out_dir.join(prefix);
    for &page in pages {
        let page_s = page.to_string();
        let _ = run_cmd(
            "pdftoppm",
            &[
                "-r",
                RENDER_DPI,
                "-png",
                "-f",
                &page_s,
                "-l",
                &page_s,
                pdf.to_str().unwrap_or(""),
                prefix_path.to_str().unwrap_or(""),
            ],
        )?;
        // pdftoppm output name: {prefix}-{page-padded}.png (padding varies by page count).
        for entry in fs::read_dir(out_dir)? {
            let entry = entry?;
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            let expect = format!("{prefix}-");
            if name_str.starts_with(&expect)
                && name_str.ends_with(".png")
                && name_str.contains(&format!("-{page_s}"))
            {
                if !rendered.contains(&entry.path()) {
                    rendered.push(entry.path());
                }
            }
        }
    }
    rendered.sort();
    Ok(rendered)
}

/// Detect pages with significant embedded images via `pdfimages -list`.
/// Output is a whitespace-delimited table; columns (by header) are:
/// `page num  type  width height color comp bpc  enc interp  object ID  x-ppi y-ppi size ratio`
fn pages_with_illustrations(pdf: &Path) -> Result<Vec<usize>> {
    let list = run_cmd("pdfimages", &["-list", pdf.to_str().unwrap_or("")])?;
    let mut pages = std::collections::BTreeSet::new();
    for (idx, line) in list.lines().enumerate() {
        if idx < 2 {
            continue; // header rows
        }
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 14 {
            continue;
        }
        let page: usize = cols[0].parse().unwrap_or(0);
        if page == 0 {
            continue;
        }
        // "size" column carries a human-readable suffix (B/K/M). Check the numeric prefix.
        let size_col = cols[13];
        let (num, suffix) = size_col
            .chars()
            .position(|c| !c.is_ascii_digit() && c != '.')
            .map(|i| size_col.split_at(i))
            .unwrap_or((size_col, ""));
        let value: f64 = num.parse().unwrap_or(0.0);
        let bytes = match suffix {
            "B" => value as u64,
            "K" => (value * 1024.0) as u64,
            "M" => (value * 1024.0 * 1024.0) as u64,
            "G" => (value * 1024.0 * 1024.0 * 1024.0) as u64,
            _ => value as u64,
        };
        if bytes >= MIN_ILLUSTRATION_IMAGE_BYTES {
            pages.insert(page);
        }
    }
    Ok(pages.into_iter().collect())
}

/// Turn pdftotext layout output into Obsidian-flavored markdown and embed image links.
fn to_markdown(text: &str, slug: &str, attachments: &[PathBuf]) -> String {
    let mut md = String::with_capacity(text.len() + 256);
    md.push_str(&format!("# {}\n\n", slug.replace('-', " ")));
    // Normalize: collapse runs of ≥3 blank lines, strip page-number-only lines.
    let mut blank_run = 0usize;
    for raw in text.lines() {
        let trimmed = raw.trim_end();
        if trimmed.is_empty() {
            blank_run += 1;
            if blank_run <= 2 {
                md.push('\n');
            }
            continue;
        }
        blank_run = 0;
        // drop standalone page-number lines
        if trimmed.trim().chars().all(|c| c.is_ascii_digit())
            && trimmed.trim().len() <= 4
        {
            continue;
        }
        md.push_str(trimmed);
        md.push('\n');
    }
    if !attachments.is_empty() {
        md.push_str("\n## Attachments\n\n");
        for p in attachments {
            if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
                md.push_str(&format!("![[{slug}/{name}]]\n"));
            }
        }
    }
    md
}

pub struct IngestOutcome {
    pub slug: String,
    pub domain: String,
    pub raw_dir: PathBuf,
    pub sorted_path: PathBuf,
    pub attachments: Vec<PathBuf>,
    pub page_count: usize,
}

pub fn ingest_pdf(root: &Path, src: &Path, target_domain: Option<&str>) -> Result<IngestOutcome> {
    if !src.exists() {
        bail!("PDF source does not exist: {}", src.display());
    }
    let slug = slug_from_path(src)?;
    if slug.is_empty() {
        bail!("could not derive slug from {}", src.display());
    }

    let raw_dir = root.join("00-raw").join(&slug);
    fs::create_dir_all(&raw_dir)?;
    let dst_pdf = raw_dir.join("source.pdf");
    fs::copy(src, &dst_pdf).with_context(|| {
        format!(
            "failed to copy {} -> {}",
            src.display(),
            dst_pdf.display()
        )
    })?;

    let bytes = fs::read(&dst_pdf)?;
    let sha = hex::encode(Sha256::digest(&bytes));
    let page_count = count_pages(&dst_pdf).unwrap_or(0);

    let text = extract_text(&dst_pdf)?;
    let domain = target_domain
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| classify_pdf_content(&text).to_string());

    // Image extraction: embedded images + illustration-page screenshots.
    let attachments_dir = root.join("_attachments").join(&slug);
    fs::create_dir_all(&attachments_dir)?;
    let mut attachments = extract_embedded_images(&dst_pdf, &attachments_dir, "img")
        .unwrap_or_default();
    if let Ok(illustration_pages) = pages_with_illustrations(&dst_pdf) {
        if !illustration_pages.is_empty() {
            let mut rendered =
                render_pages(&dst_pdf, &attachments_dir, "page", &illustration_pages)
                    .unwrap_or_default();
            attachments.append(&mut rendered);
        }
    }

    let sorted_dir = root.join("01-sorted").join(&domain);
    fs::create_dir_all(&sorted_dir)?;
    let sorted_path = sorted_dir.join(format!("{slug}.md"));
    let md = to_markdown(&text, &slug, &attachments);
    fs::write(&sorted_path, &md)?;

    let meta = json!({
        "sha256": sha,
        "source_type": "pdf",
        "ingested": Utc::now().to_rfc3339(),
        "pages": page_count,
        "domain": domain,
        "source_filename": src.file_name().and_then(|s| s.to_str()).unwrap_or_default(),
        "attachments": attachments.iter().filter_map(|p| p.file_name().and_then(|s| s.to_str()).map(str::to_string)).collect::<Vec<_>>(),
    });
    fs::write(
        raw_dir.join("metadata.json"),
        serde_json::to_string_pretty(&meta)?,
    )?;

    // Record SHA to dedup index + append to curation queue.
    let hashes = root.join("00-raw/.hashes");
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&hashes) {
        let _ = writeln!(f, "{sha} {slug}");
    }
    let queue_path = root.join("state").join("curation_queue.jsonl");
    if let Some(parent) = queue_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let entry = json!({
        "ts": Utc::now().to_rfc3339(),
        "slug": slug,
        "domain": domain,
        "source": dst_pdf.display().to_string(),
        "source_type": "pdf",
        "page_count": page_count,
    });
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&queue_path)?;
    writeln!(f, "{}", entry)?;

    Ok(IngestOutcome {
        slug,
        domain,
        raw_dir,
        sorted_path,
        attachments,
        page_count,
    })
}

pub fn tool_defs() -> Vec<Value> {
    vec![json!({
        "name": "nlr_pdf_ingest",
        "description": "Ingest a PDF: copy to 00-raw/<slug>/source.pdf, extract text via pdftotext, extract images via pdfimages, render illustration pages via pdftoppm, emit Obsidian-flavored markdown to 01-sorted/<domain>/<slug>.md, and enqueue for curation. Requires poppler-utils (pdftotext, pdfimages, pdftoppm, pdfinfo) on PATH.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Absolute path to the source PDF"},
                "target_domain": {"type": "string", "description": "Override auto-classified domain (math, quant, ml-nn, software-engineering, scientific-computing, docs)"}
            },
            "required": ["path"]
        }
    })]
}

pub fn call(name: &str, args: &Value, root: &Path) -> Result<String> {
    match name {
        "nlr_pdf_ingest" => {
            let path = args
                .get("path")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .ok_or_else(|| anyhow::anyhow!("nlr_pdf_ingest: 'path' is required (non-empty string)"))?;
            let target_domain = args.get("target_domain").and_then(|v| v.as_str());
            let outcome = ingest_pdf(root, Path::new(path), target_domain)?;
            Ok(json!({
                "status": "ingested",
                "slug": outcome.slug,
                "domain": outcome.domain,
                "sorted_path": outcome.sorted_path.display().to_string(),
                "page_count": outcome.page_count,
                "attachments": outcome.attachments.iter().filter_map(|p| p.file_name().and_then(|s| s.to_str()).map(str::to_string)).collect::<Vec<_>>(),
            })
            .to_string())
        }
        _ => bail!("Unknown pdf_ingest tool: {name}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_from_path_normalizes() {
        let p = PathBuf::from("/tmp/Meta-Harness ArXiv 2026.pdf");
        assert_eq!(slug_from_path(&p).unwrap(), "meta-harness-arxiv-2026");
    }

    #[test]
    fn classify_recognizes_math() {
        assert_eq!(classify_pdf_content("By the theorem of Stokes, ..."), "math");
    }

    #[test]
    fn classify_meta_harness_goes_to_ml_nn_not_math() {
        // Previously: a single "theorem" hit would flip this to math. The
        // explicit override now catches harness/optimizer/benchmark/LLM/agent
        // and routes to ml-nn so these meta-papers are labelled correctly.
        let text =
            "We propose a new harness for LLM agents. Our optimizer outperforms \
             prior benchmarks. Theorem 1 states the convergence bound.";
        assert_eq!(classify_pdf_content(text), "ml-nn");
    }

    #[test]
    fn classify_picks_majority_domain() {
        // ml-nn keywords dominate (neural, transformer, gradient, attention),
        // with a single passing mention of "theorem". Majority wins.
        let text = "neural transformer gradient attention embedding theorem";
        assert_eq!(classify_pdf_content(text), "ml-nn");
    }

    #[test]
    fn classify_ties_fall_back_to_docs() {
        // Exactly one math hit + one quant hit → tie → docs.
        let text = "theorem portfolio";
        assert_eq!(classify_pdf_content(text), "docs");
    }

    #[test]
    fn classify_all_zero_falls_back_to_docs() {
        let text = "nothing interesting here.";
        assert_eq!(classify_pdf_content(text), "docs");
    }

    #[test]
    fn to_markdown_embeds_attachments() {
        let md = to_markdown(
            "Hello world\n\n\n\n\n1\nEnd.\n",
            "test-slug",
            &[PathBuf::from("/attachments/test-slug/img-000.png")],
        );
        assert!(md.starts_with("# test slug\n\n"));
        assert!(md.contains("Hello world"));
        assert!(md.contains("![[test-slug/img-000.png]]"));
        assert!(!md.contains("\n1\n"), "page-number line should be dropped");
    }
}
