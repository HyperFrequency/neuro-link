use super::*;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Parse one Claude Code session JSONL file.
pub fn parse_session(path: &Path) -> Result<SessionSummary> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("reading {}", path.display()))?;

    let skill_cmd_re = Regex::new(r"<command-name>/?([^<]+)</command-name>").unwrap();

    // Project slug from parent dir name (e.g. -Users-DanBot-Desktop-HyperFrequency)
    let project = path
        .parent()
        .and_then(|p| p.file_name())
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".into());

    let mut session_id = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let mut cwd = PathBuf::new();
    let mut git_branch: Option<String> = None;
    let mut model = String::new();
    let mut first_ts: Option<DateTime<Utc>> = None;
    let mut last_ts: Option<DateTime<Utc>> = None;
    let mut turns: Vec<Turn> = Vec::new();
    let mut turn_counter: u32 = 0;
    let mut skills_invoked: Vec<SkillInvocation> = Vec::new();
    let mut tools_used: HashMap<String, u32> = HashMap::new();
    let mut totals = TokenTotals::default();
    let mut files_touched: Vec<String> = Vec::new();
    let mut subagents: Vec<SubagentInvocation> = Vec::new();
    let mut external_fetches: Vec<String> = Vec::new();

    for line in content.lines() {
        if line.trim().is_empty() { continue; }
        let entry: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Meta fields
        if let Some(v) = entry.get("sessionId").and_then(|v| v.as_str()) {
            if session_id.is_empty() { session_id = v.to_string(); }
        }
        if let Some(v) = entry.get("cwd").and_then(|v| v.as_str()) {
            if cwd.as_os_str().is_empty() { cwd = PathBuf::from(v); }
        }
        if let Some(v) = entry.get("gitBranch").and_then(|v| v.as_str()) {
            if git_branch.is_none() && !v.is_empty() { git_branch = Some(v.to_string()); }
        }

        let ts = entry
            .get("timestamp")
            .and_then(|v| v.as_str())
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc));
        if let Some(t) = ts {
            if first_ts.is_none() || t < first_ts.unwrap() { first_ts = Some(t); }
            if last_ts.is_none() || t > last_ts.unwrap() { last_ts = Some(t); }
        }

        let entry_type = entry.get("type").and_then(|v| v.as_str()).unwrap_or("");

        match entry_type {
            "user" | "assistant" | "system" => {
                turn_counter += 1;
                let message = entry.get("message");
                let role = if entry_type == "user" { "user" }
                    else if entry_type == "assistant" { "assistant" }
                    else { "system" }.to_string();

                // Extract model from assistant messages
                if entry_type == "assistant" {
                    if let Some(m) = message.and_then(|m| m.get("model")).and_then(|v| v.as_str()) {
                        if model.is_empty() { model = m.to_string(); }
                    }
                }

                // Text + thinking + tool_use from assistant; tool_result from user
                let mut text = None;
                let mut thinking = None;
                let mut tool_calls: Vec<ToolCall> = Vec::new();
                let mut tool_results: Vec<ToolResult> = Vec::new();

                if let Some(content) = message.and_then(|m| m.get("content")) {
                    if let Some(s) = content.as_str() {
                        text = Some(s.to_string());
                    } else if let Some(arr) = content.as_array() {
                        let mut text_parts: Vec<String> = Vec::new();
                        let mut think_parts: Vec<String> = Vec::new();
                        for item in arr {
                            let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
                            match item_type {
                                "text" => {
                                    if let Some(s) = item.get("text").and_then(|v| v.as_str()) {
                                        text_parts.push(s.to_string());
                                    }
                                }
                                "thinking" => {
                                    if let Some(s) = item.get("thinking").and_then(|v| v.as_str()) {
                                        think_parts.push(s.to_string());
                                    }
                                }
                                "tool_use" => {
                                    let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                    let id = item.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                    let input = item.get("input").cloned().unwrap_or(Value::Null);

                                    *tools_used.entry(name.clone()).or_default() += 1;

                                    // Track file-touching tools
                                    if matches!(name.as_str(), "Read" | "Edit" | "Write" | "NotebookEdit") {
                                        if let Some(fp) = input.get("file_path").and_then(|v| v.as_str()) {
                                            if !files_touched.contains(&fp.to_string()) {
                                                files_touched.push(fp.to_string());
                                            }
                                        }
                                    }
                                    // Track WebFetch URLs
                                    if name == "WebFetch" {
                                        if let Some(u) = input.get("url").and_then(|v| v.as_str()) {
                                            external_fetches.push(u.to_string());
                                        }
                                    }
                                    // Track subagent spawns
                                    if name == "Agent" {
                                        let desc = input.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                        let sub = input.get("subagent_type").and_then(|v| v.as_str()).unwrap_or("general-purpose").to_string();
                                        subagents.push(SubagentInvocation {
                                            description: desc,
                                            subagent_type: sub,
                                            turn: turn_counter,
                                        });
                                    }

                                    tool_calls.push(ToolCall { id, name, input });
                                }
                                "tool_result" => {
                                    let tool_use_id = item.get("tool_use_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                    let is_error = item.get("is_error").and_then(|v| v.as_bool()).unwrap_or(false);
                                    let (preview, bytes) = extract_tool_result_preview(&item.get("content").cloned().unwrap_or(Value::Null));
                                    tool_results.push(ToolResult {
                                        tool_use_id,
                                        is_error,
                                        content_preview: preview,
                                        content_bytes: bytes,
                                    });
                                }
                                _ => {}
                            }
                        }
                        if !text_parts.is_empty() { text = Some(text_parts.join("\n\n")); }
                        if !think_parts.is_empty() { thinking = Some(think_parts.join("\n\n")); }
                    }
                }

                // Check for slash command invocations in user text
                let mut skill_command = None;
                if let Some(ref t) = text {
                    for cap in skill_cmd_re.captures_iter(t) {
                        if let Some(name) = cap.get(1) {
                            let name_str = name.as_str().trim().to_string();
                            if !name_str.is_empty() {
                                skill_command = Some(format!("/{}", name_str));
                                skills_invoked.push(SkillInvocation {
                                    skill: format!("/{}", name_str),
                                    turn: turn_counter,
                                });
                            }
                        }
                    }
                }

                // Tokens
                let mut turn_tokens = None;
                if let Some(usage) = message.and_then(|m| m.get("usage")) {
                    let t = TurnTokens {
                        input: usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                        output: usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                        cache_creation: usage.get("cache_creation_input_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                        cache_read: usage.get("cache_read_input_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                    };
                    totals.input += t.input;
                    totals.output += t.output;
                    totals.cache_creation += t.cache_creation;
                    totals.cache_read += t.cache_read;
                    turn_tokens = Some(t);
                }

                turns.push(Turn {
                    turn_num: turn_counter,
                    role,
                    timestamp: ts.unwrap_or_else(Utc::now),
                    text,
                    thinking,
                    tool_calls,
                    tool_results,
                    tokens: turn_tokens,
                    skill_command,
                });
            }
            _ => {}
        }
    }

    // Cost estimate (Anthropic Claude pricing, conservative)
    totals.cost_usd_estimate = estimate_cost(&model, &totals);

    Ok(SessionSummary {
        session_id,
        project,
        cwd,
        git_branch,
        model,
        started: first_ts.unwrap_or_else(Utc::now),
        ended: last_ts.unwrap_or_else(Utc::now),
        turns,
        skills_invoked,
        tools_used,
        tokens: totals,
        files_touched,
        subagents,
        external_fetches,
        quality_flags: Vec::new(), // filled later by quality::analyze()
    })
}

fn extract_tool_result_preview(content: &Value) -> (String, usize) {
    let extract = |s: &str| -> (String, usize) {
        let bytes = s.len();
        let preview = if s.chars().count() > 500 {
            let truncated: String = s.chars().take(500).collect();
            format!("{}... ({} bytes)", truncated, bytes)
        } else {
            s.to_string()
        };
        (preview, bytes)
    };
    if let Some(s) = content.as_str() {
        return extract(s);
    }
    if let Some(arr) = content.as_array() {
        let mut combined = String::new();
        for item in arr {
            if let Some(t) = item.get("text").and_then(|v| v.as_str()) {
                combined.push_str(t);
                combined.push('\n');
            }
        }
        return extract(&combined);
    }
    (String::new(), 0)
}

fn estimate_cost(model: &str, tokens: &TokenTotals) -> f64 {
    // Per-million-token rates
    let (in_rate, out_rate) = if model.contains("opus") {
        (15.0, 75.0)
    } else if model.contains("sonnet") {
        (3.0, 15.0)
    } else if model.contains("haiku") {
        (1.0, 5.0)
    } else {
        (3.0, 15.0)
    };
    // Cache read is 10% of input rate; cache creation is 25% more than input rate
    let input_cost = tokens.input as f64 * in_rate / 1_000_000.0;
    let output_cost = tokens.output as f64 * out_rate / 1_000_000.0;
    let cache_read_cost = tokens.cache_read as f64 * (in_rate * 0.1) / 1_000_000.0;
    let cache_creation_cost = tokens.cache_creation as f64 * (in_rate * 1.25) / 1_000_000.0;
    input_cost + output_cost + cache_read_cost + cache_creation_cost
}

/// Parse all session logs under ~/.claude/projects/
pub fn parse_all_sessions(claude_root: &Path) -> Result<Vec<SessionSummary>> {
    let projects = claude_root.join("projects");
    if !projects.is_dir() { return Ok(Vec::new()); }

    let mut sessions = Vec::new();
    for entry in WalkDir::new(&projects).into_iter().filter_map(|e| e.ok()) {
        let p = entry.path();
        if p.extension().is_some_and(|e| e == "jsonl") {
            match parse_session(p) {
                Ok(s) => sessions.push(s),
                Err(e) => eprintln!("[sessions] failed to parse {}: {e}", p.display()),
            }
        }
    }
    Ok(sessions)
}

/// Parse only sessions newer than the given timestamp
pub fn parse_sessions_since(claude_root: &Path, since: DateTime<Utc>) -> Result<Vec<SessionSummary>> {
    let all = parse_all_sessions(claude_root)?;
    Ok(all.into_iter().filter(|s| s.ended >= since).collect())
}
