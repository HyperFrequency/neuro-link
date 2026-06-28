//! State management: heartbeat, session log, score history, deviation log.

use anyhow::Result;
use chrono::Utc;
use serde_json::{json, Value};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

pub struct StateManager { root: PathBuf }

impl StateManager {
    pub fn new(root: PathBuf) -> Self { Self { root } }
    fn state_dir(&self) -> PathBuf { self.root.join("state") }

    pub fn read_heartbeat(&self) -> Result<Value> {
        let content = fs::read_to_string(self.state_dir().join("heartbeat.json"))?;
        Ok(serde_json::from_str(&content)?)
    }

    pub fn update_heartbeat(&self, status: &str, errors: &[String]) -> Result<()> {
        let hb = json!({"status": status, "last_check": Utc::now().to_rfc3339(), "errors": errors});
        fs::write(self.state_dir().join("heartbeat.json"), serde_json::to_string_pretty(&hb)?)?;
        Ok(())
    }

    pub fn append_session_log(&self, tool: &str, exit_code: Option<i32>) -> Result<()> {
        let entry = json!({"timestamp": Utc::now().to_rfc3339(), "tool": tool, "exit_code": exit_code, "success": exit_code.map(|c| c == 0).unwrap_or(true), "session": "mcp-server"});
        self.append_jsonl("session_log.jsonl", &entry)
    }

    pub fn append_score(&self, metric: &str, value: f64, target: Option<f64>) -> Result<()> {
        let mut entry = json!({"timestamp": Utc::now().to_rfc3339(), "metric": metric, "value": value});
        if let Some(t) = target { entry["target"] = json!(t); }
        self.append_jsonl("score_history.jsonl", &entry)
    }

    pub fn append_deviation(&self, category: &str, description: &str) -> Result<()> {
        let entry = json!({"timestamp": Utc::now().to_rfc3339(), "category": category, "description": description, "resolved": false});
        self.append_jsonl("deviation_log.jsonl", &entry)
    }

    fn append_jsonl(&self, filename: &str, entry: &Value) -> Result<()> {
        let mut file = OpenOptions::new().create(true).append(true).open(self.state_dir().join(filename))?;
        writeln!(file, "{}", serde_json::to_string(entry)?)?;
        Ok(())
    }

    pub fn count_pending_tasks(&self) -> Result<usize> {
        let task_dir = self.root.join("07-neuro-link-task");
        if !task_dir.is_dir() { return Ok(0); }
        let mut count = 0;
        for entry in fs::read_dir(&task_dir)? {
            let path = entry?.path();
            if path.extension().is_some_and(|e| e == "md") {
                if fs::read_to_string(&path)?.contains("status: pending") { count += 1; }
            }
        }
        Ok(count)
    }
}
