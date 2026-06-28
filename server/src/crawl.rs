use anyhow::Result;
use scraper::{Html, Selector};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::Path;

pub struct CrawlResult {
    pub url: String,
    pub text: String,
    pub title: String,
    pub error: Option<String>,
}

pub struct IngestResult {
    pub slug: String,
    pub sha256: String,
    pub duplicate: bool,
    pub domain: String,
}

pub async fn crawl_url(url: &str, client: &reqwest::Client) -> CrawlResult {
    match client.get(url).send().await {
        Ok(resp) => match resp.text().await {
            Ok(html) => {
                let doc = Html::parse_document(&html);
                let title = doc
                    .select(&Selector::parse("title").unwrap())
                    .next()
                    .map(|el| el.text().collect::<String>())
                    .unwrap_or_default();
                let text = extract_text(&doc);
                CrawlResult { url: url.to_string(), text, title, error: None }
            }
            Err(e) => CrawlResult {
                url: url.to_string(),
                text: String::new(),
                title: String::new(),
                error: Some(format!("Body read error: {e}")),
            },
        },
        Err(e) => CrawlResult {
            url: url.to_string(),
            text: String::new(),
            title: String::new(),
            error: Some(format!("Fetch error: {e}")),
        },
    }
}

fn extract_text(doc: &Html) -> String {
    for tag in ["article", "main"] {
        if let Ok(sel) = Selector::parse(tag) {
            if let Some(el) = doc.select(&sel).next() {
                let text: String = el.text().collect::<Vec<_>>().join(" ");
                let trimmed = text.split_whitespace().collect::<Vec<_>>().join(" ");
                if !trimmed.is_empty() {
                    return trimmed;
                }
            }
        }
    }
    if let Ok(sel) = Selector::parse("body") {
        if let Some(el) = doc.select(&sel).next() {
            let text: String = el.text().collect::<Vec<_>>().join(" ");
            return text.split_whitespace().collect::<Vec<_>>().join(" ");
        }
    }
    String::new()
}

pub async fn parallel_crawl(urls: &[String], max_concurrent: usize) -> Vec<CrawlResult> {
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(max_concurrent));
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_default();
    let mut handles = Vec::new();
    for url in urls {
        let permit = semaphore.clone();
        let client = client.clone();
        let url = url.clone();
        handles.push(tokio::spawn(async move {
            let _permit = permit.acquire().await.unwrap();
            crawl_url(&url, &client).await
        }));
    }
    let mut results = Vec::new();
    for handle in handles {
        match handle.await {
            Ok(r) => results.push(r),
            Err(e) => results.push(CrawlResult {
                url: String::new(),
                text: String::new(),
                title: String::new(),
                error: Some(format!("Join error: {e}")),
            }),
        }
    }
    results
}

pub fn ingest_content(root: &Path, slug: &str, content: &str, url: &str) -> Result<IngestResult> {
    let sha = hex::encode(Sha256::digest(content.as_bytes()));
    let hashes_path = root.join("00-raw/.hashes");
    if hashes_path.exists() {
        let existing = fs::read_to_string(&hashes_path)?;
        if existing.contains(&sha) {
            return Ok(IngestResult {
                slug: slug.to_string(),
                sha256: sha,
                duplicate: true,
                domain: String::new(),
            });
        }
    }
    let dir = root.join("00-raw").join(slug);
    fs::create_dir_all(&dir)?;
    fs::write(dir.join("source.md"), content)?;
    let meta = serde_json::json!({
        "url": url,
        "sha256": &sha,
        "word_count": content.split_whitespace().count(),
    });
    fs::write(dir.join("metadata.json"), serde_json::to_string_pretty(&meta)?)?;
    let mut f = fs::OpenOptions::new().create(true).append(true).open(&hashes_path)?;
    writeln!(f, "{sha} {slug}")?;

    let domain = classify_url(url);
    let dst_dir = root.join("01-sorted").join(&domain);
    fs::create_dir_all(&dst_dir)?;
    fs::write(dst_dir.join(format!("{slug}.md")), content)?;

    Ok(IngestResult {
        slug: slug.to_string(),
        sha256: sha,
        duplicate: false,
        domain,
    })
}

fn classify_url(url: &str) -> String {
    if url.contains("arxiv.org") {
        "arxiv".into()
    } else if url.contains("medium.com") {
        "medium".into()
    } else if url.contains("github.com") {
        "github".into()
    } else if url.contains("huggingface.co") {
        "huggingface".into()
    } else {
        "docs".into()
    }
}
