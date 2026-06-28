// Claude Code session log tracking + quality analysis.
// Reads ~/.claude/projects/<project>/<session>.jsonl, exports to Obsidian markdown,
// detects quality issues (missed tool calls, bad reasoning, loops).

pub mod parser;
pub mod markdown;
pub mod quality;
pub mod llm_export;
pub mod watcher;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub session_id: String,
    pub project: String,
    pub cwd: PathBuf,
    pub git_branch: Option<String>,
    pub model: String,
    pub started: DateTime<Utc>,
    pub ended: DateTime<Utc>,
    pub turns: Vec<Turn>,
    pub skills_invoked: Vec<SkillInvocation>,
    pub tools_used: HashMap<String, u32>,
    pub tokens: TokenTotals,
    pub files_touched: Vec<String>,
    pub subagents: Vec<SubagentInvocation>,
    pub external_fetches: Vec<String>,
    pub quality_flags: Vec<quality::QualityFlag>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Turn {
    pub turn_num: u32,
    pub role: String, // user | assistant | system | tool_result
    pub timestamp: DateTime<Utc>,
    pub text: Option<String>,
    pub thinking: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub tool_results: Vec<ToolResult>,
    pub tokens: Option<TurnTokens>,
    pub skill_command: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub tool_use_id: String,
    pub is_error: bool,
    pub content_preview: String,
    pub content_bytes: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TurnTokens {
    pub input: u64,
    pub output: u64,
    pub cache_creation: u64,
    pub cache_read: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenTotals {
    pub input: u64,
    pub output: u64,
    pub cache_creation: u64,
    pub cache_read: u64,
    pub cost_usd_estimate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInvocation {
    pub skill: String,  // "/neuro-link"
    pub turn: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentInvocation {
    pub description: String,
    pub subagent_type: String,
    pub turn: u32,
}

/// Resolve the Claude Code projects directory (respects CLAUDE_ROOT override).
pub fn claude_root() -> PathBuf {
    if let Ok(v) = std::env::var("CLAUDE_ROOT") {
        return PathBuf::from(v);
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".claude");
    }
    PathBuf::from(".claude")
}
