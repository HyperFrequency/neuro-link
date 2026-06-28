use anyhow::Result;
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::Path;
use walkdir::WalkDir;

#[derive(Debug, Serialize)]
pub struct SessionGrades {
    pub total_calls: usize,
    pub success_count: usize,
    pub success_rate: f64,
    pub tool_distribution: HashMap<String, usize>,
}

#[derive(Debug, Serialize)]
pub struct WikiGrades {
    pub total_pages: usize,
    pub open_questions: usize,
    pub contradictions: usize,
    pub stale_pages: usize,
    pub avg_confidence: String,
}

pub fn grade_session(root: &Path) -> Result<SessionGrades> {
    let log_path = root.join("state/session_log.jsonl");
    let mut total = 0;
    let mut success = 0;
    let mut tools: HashMap<String, usize> = HashMap::new();

    if log_path.exists() {
        let content = fs::read_to_string(&log_path)?;
        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) {
                total += 1;
                if entry["success"].as_bool().unwrap_or(false) {
                    success += 1;
                }
                if let Some(tool) = entry["tool"].as_str() {
                    *tools.entry(tool.to_string()).or_default() += 1;
                }
            }
        }
    }

    let rate = if total > 0 {
        success as f64 / total as f64
    } else {
        0.0
    };

    Ok(SessionGrades {
        total_calls: total,
        success_count: success,
        success_rate: rate,
        tool_distribution: tools,
    })
}

pub fn grade_wiki(root: &Path) -> Result<WikiGrades> {
    let kb = root.join("02-KB-main");
    let skip = ["schema.md", "index.md", "log.md"];
    let now = chrono::Utc::now().date_naive();
    let mut total = 0;
    let mut open_q = 0;
    let mut contradictions = 0;
    let mut stale = 0;
    let mut confidence_counts: HashMap<String, usize> = HashMap::new();

    for entry in WalkDir::new(&kb).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.extension().is_some_and(|e| e == "md")
            || skip.iter().any(|s| path.file_name().is_some_and(|f| f == *s))
        {
            continue;
        }
        let content = fs::read_to_string(path).unwrap_or_default();
        total += 1;

        if content.contains("## Open Questions") || content.contains("open_questions:") {
            let q_count = content
                .lines()
                .filter(|l| l.trim_start().starts_with("- ") || l.trim_start().starts_with("* "))
                .count();
            open_q += q_count.min(20);
        }
        if content.contains("## Contradictions") {
            contradictions += 1;
        }
        if let Some(date_line) = content.lines().find(|l| l.starts_with("last_updated:")) {
            let date_part = date_line.split(':').nth(1).unwrap_or("").trim();
            if let Ok(date) = chrono::NaiveDate::parse_from_str(date_part, "%Y-%m-%d") {
                if (now - date).num_days() > 30 {
                    stale += 1;
                }
            }
            if let Some(conf_line) = content.lines().find(|l| l.starts_with("confidence:")) {
                let conf = conf_line.split(':').nth(1).unwrap_or("").trim().to_string();
                *confidence_counts.entry(conf).or_default() += 1;
            }
        }
    }

    let avg_conf = confidence_counts
        .iter()
        .max_by_key(|(_, c)| *c)
        .map(|(k, _)| k.clone())
        .unwrap_or_else(|| "unknown".into());

    Ok(WikiGrades {
        total_pages: total,
        open_questions: open_q,
        contradictions,
        stale_pages: stale,
        avg_confidence: avg_conf,
    })
}

pub fn append_score(root: &Path, metric: &str, value: f64, target: Option<f64>) -> Result<()> {
    let mut entry = serde_json::json!({
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "metric": metric,
        "value": value,
    });
    if let Some(t) = target {
        entry["target"] = serde_json::json!(t);
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join("state/score_history.jsonl"))?;
    writeln!(file, "{}", serde_json::to_string(&entry)?)?;
    Ok(())
}
