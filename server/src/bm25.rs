use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

const K1: f64 = 1.2;
const B: f64 = 0.75;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BM25Index {
    pub doc_count: usize,
    pub avg_doc_len: f64,
    /// doc_id -> (relative_path, preview, token_count)
    pub docs: HashMap<usize, (String, String, usize)>,
    /// term -> vec of (doc_id, term_frequency)
    pub postings: HashMap<String, Vec<(usize, usize)>>,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub path: String,
    pub score: f64,
    pub preview: String,
}

fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2)
        .map(|w| w.to_string())
        .collect()
}

pub fn build_index(root: &Path) -> BM25Index {
    let kb = root.join("02-KB-main");
    let skip = ["schema.md", "index.md", "log.md"];
    let mut docs = HashMap::new();
    let mut postings: HashMap<String, Vec<(usize, usize)>> = HashMap::new();
    let mut total_tokens: usize = 0;
    let mut doc_id: usize = 0;

    for entry in WalkDir::new(&kb).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.extension().is_some_and(|e| e == "md")
            || skip.iter().any(|s| path.file_name().is_some_and(|f| f == *s))
        {
            continue;
        }

        let content = fs::read_to_string(path).unwrap_or_default();
        let rel = path.strip_prefix(&kb).unwrap_or(path).display().to_string();
        let preview: String = content
            .lines()
            .filter(|l| !l.starts_with("---") && !l.starts_with('#') && !l.is_empty())
            .take(3)
            .collect::<Vec<_>>()
            .join(" ");

        let tokens = tokenize(&content);
        let token_count = tokens.len();
        total_tokens += token_count;

        // Count term frequencies for this doc
        let mut tf: HashMap<String, usize> = HashMap::new();
        for token in &tokens {
            *tf.entry(token.clone()).or_default() += 1;
        }

        for (term, freq) in tf {
            postings.entry(term).or_default().push((doc_id, freq));
        }

        docs.insert(
            doc_id,
            (rel, preview[..preview.len().min(300)].to_string(), token_count),
        );
        doc_id += 1;
    }

    let doc_count = docs.len();
    let avg_doc_len = if doc_count > 0 {
        total_tokens as f64 / doc_count as f64
    } else {
        1.0
    };

    BM25Index {
        doc_count,
        avg_doc_len,
        docs,
        postings,
    }
}

pub fn search(index: &BM25Index, query: &str, limit: usize) -> Vec<SearchResult> {
    let query_tokens = tokenize(query);
    let mut scores: HashMap<usize, f64> = HashMap::new();
    let n = index.doc_count as f64;

    for token in &query_tokens {
        if let Some(posting_list) = index.postings.get(token) {
            let df = posting_list.len() as f64;
            // IDF with smoothing
            let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln();
            if idf <= 0.0 {
                continue;
            }

            for &(doc_id, tf) in posting_list {
                let doc_len = index.docs.get(&doc_id).map(|d| d.2).unwrap_or(1) as f64;
                let tf_norm =
                    (tf as f64 * (K1 + 1.0)) / (tf as f64 + K1 * (1.0 - B + B * doc_len / index.avg_doc_len));
                *scores.entry(doc_id).or_default() += idf * tf_norm;
            }
        }
    }

    let mut results: Vec<_> = scores
        .into_iter()
        .filter_map(|(doc_id, score)| {
            index.docs.get(&doc_id).map(|(path, preview, _)| SearchResult {
                path: path.clone(),
                score,
                preview: preview.clone(),
            })
        })
        .collect();

    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(limit);
    results
}

pub fn save_index(index: &BM25Index, root: &Path) -> std::io::Result<()> {
    let state_dir = root.join("state");
    fs::create_dir_all(&state_dir)?;
    fs::write(
        state_dir.join("bm25_index.json"),
        serde_json::to_string(index).unwrap_or_default(),
    )
}

pub fn load_index(root: &Path) -> Option<BM25Index> {
    let path = root.join("state/bm25_index.json");
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}
