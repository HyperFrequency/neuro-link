use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

pub fn tool_defs() -> Vec<Value> {
    vec![
        json!({"name":"nlr_task_list","description":"List tasks from 07-neuro-link-task/","inputSchema":{"type":"object","properties":{"status_filter":{"type":"string","enum":["pending","running","completed","failed","all"]}}}}),
        json!({"name":"nlr_task_create","description":"Create a new task file","inputSchema":{"type":"object","properties":{"title":{"type":"string"},"type":{"type":"string"},"priority":{"type":"integer"},"body":{"type":"string"}},"required":["title","type"]}}),
        json!({"name":"nlr_task_update","description":"Update task status","inputSchema":{"type":"object","properties":{"filename":{"type":"string"},"status":{"type":"string","enum":["pending","running","completed","failed"]}},"required":["filename","status"]}}),
        json!({"name":"nlr_task_dispatch","description":"Force-dispatch a task file through the job-scanner dispatcher (manual retry override)","inputSchema":{"type":"object","properties":{"filename":{"type":"string","description":"Task filename in 07-neuro-link-task/"}},"required":["filename"]}}),
    ]
}

pub fn call(name: &str, args: &Value, root: &Path) -> Result<String> {
    let task_dir = root.join("07-neuro-link-task");
    match name {
        "nlr_task_list" => {
            let filter = args.get("status_filter").and_then(|v| v.as_str()).unwrap_or("all");
            let mut tasks = Vec::new();
            if task_dir.is_dir() {
                for entry in fs::read_dir(&task_dir)? {
                    let path = entry?.path();
                    if path.extension().is_some_and(|e| e == "md") {
                        let content = fs::read_to_string(&path)?;
                        let fname = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                        if filter != "all" && !content.contains(&format!("status: {filter}")) { continue; }
                        tasks.push(json!({"filename": fname, "preview": &content[..content.len().min(200)]}));
                    }
                }
            }
            Ok(serde_json::to_string_pretty(&tasks)?)
        }
        "nlr_task_create" => {
            let title = args["title"].as_str().unwrap_or("untitled");
            let task_type = args["type"].as_str().unwrap_or("general");
            let priority = args.get("priority").and_then(|v| v.as_u64()).unwrap_or(3);
            let body = args.get("body").and_then(|v| v.as_str()).unwrap_or("");
            let now = chrono::Utc::now().format("%Y-%m-%d").to_string();
            let slug = title.to_lowercase().replace(' ', "-");
            let filename = format!("{priority}-{task_type}-{slug}.md");
            let content = format!("---\ntype: {task_type}\nstatus: pending\npriority: {priority}\ncreated: {now}\ndepends_on: []\nassigned_harness: claude-code\n---\n# {title}\n\n{body}\n");
            fs::create_dir_all(&task_dir)?;
            fs::write(task_dir.join(&filename), &content)?;
            Ok(format!("Created: {filename}"))
        }
        "nlr_task_update" => {
            let filename = args["filename"].as_str().unwrap_or("");
            let new_status = args["status"].as_str().unwrap_or("pending");
            let path = task_dir.join(filename);
            if !path.exists() { bail!("Task not found: {filename}"); }
            let content = fs::read_to_string(&path)?;
            let updated = regex::Regex::new(r"status:\s*\w+")?.replace(&content, &format!("status: {new_status}")).to_string();
            fs::write(&path, updated)?;
            Ok(format!("Updated {filename} → {new_status}"))
        }
        "nlr_task_dispatch" => {
            let filename = args["filename"].as_str().unwrap_or("");
            if filename.is_empty() { bail!("filename required"); }
            // Block traversal: only allow bare filenames, not paths.
            if filename.contains('/') || filename.contains("..") {
                bail!("invalid filename: path traversal not allowed");
            }
            let path = task_dir.join(filename);
            if !path.exists() { bail!("Task not found: {filename}"); }
            let outcome = super::dispatcher::dispatch_task(root, &path)?;
            Ok(serde_json::to_string_pretty(&outcome)?)
        }
        _ => bail!("Unknown task tool: {name}"),
    }
}
