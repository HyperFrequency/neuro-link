// Rule-based quality analysis for Claude Code sessions.
// Detects: missed tool calls, repeated failures, ignored errors, abandoned tools,
// loops, hallucinated files, self-contradictions, skill misuse.

use super::*;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityFlag {
    pub session_id: String,
    pub turn: u32,
    pub flag_type: FlagType,
    pub severity: Severity,
    pub description: String,
    pub evidence: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum FlagType {
    MissedToolCall,
    RepeatedFailure,
    ErrorIgnored,
    AbandonedTool,
    LoopDetected,
    HallucinatedFile,
    ContradictedSelf,
    SkillMisuse,
}

impl FlagType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::MissedToolCall => "missed_tool_call",
            Self::RepeatedFailure => "repeated_failure",
            Self::ErrorIgnored => "error_ignored",
            Self::AbandonedTool => "abandoned_tool",
            Self::LoopDetected => "loop_detected",
            Self::HallucinatedFile => "hallucinated_file",
            Self::ContradictedSelf => "contradicted_self",
            Self::SkillMisuse => "skill_misuse",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Severity {
    Critical,
    Warning,
    Info,
}

/// Run all heuristics against a session, returning detected flags.
pub fn analyze(session: &SessionSummary) -> Vec<QualityFlag> {
    let mut flags = Vec::new();
    flags.extend(detect_missed_tool_calls(session));
    flags.extend(detect_repeated_failures(session));
    flags.extend(detect_errors_ignored(session));
    flags.extend(detect_abandoned_tools(session));
    flags.extend(detect_loops(session));
    flags.extend(detect_hallucinated_files(session));
    flags.extend(detect_self_contradictions(session));
    flags
}

// ─── 1. Missed Tool Calls ───────────────────────────────────────────
// Assistant text promises a tool action but no tool_use follows.

fn detect_missed_tool_calls(session: &SessionSummary) -> Vec<QualityFlag> {
    let mut flags = Vec::new();
    let intent_patterns: Vec<(Regex, &str)> = vec![
        (Regex::new(r"(?i)(?:i'?ll|let me|i will|let's|going to)\s+(edit|modify|update|change|fix)\s+").unwrap(), "Edit"),
        (Regex::new(r"(?i)(?:i'?ll|let me|i will|let's|going to)\s+(read|check|look at|open)\s+").unwrap(), "Read"),
        (Regex::new(r"(?i)(?:i'?ll|let me|i will|let's|going to)\s+(run|execute|invoke)\s+").unwrap(), "Bash"),
        (Regex::new(r"(?i)(?:i'?ll|let me|i will|let's|going to)\s+(write|create|generate)\s+").unwrap(), "Write"),
        (Regex::new(r"(?i)(?:i'?ll|let me|i will|let's|going to)\s+(search|grep|find)\s+").unwrap(), "Grep"),
    ];

    for (i, turn) in session.turns.iter().enumerate() {
        if turn.role != "assistant" { continue; }
        let Some(text) = &turn.text else { continue; };
        if text.is_empty() { continue; }

        for (re, expected_tool) in &intent_patterns {
            if let Some(m) = re.find(text) {
                // Did this turn or the next assistant turn use the expected tool?
                let used_here = turn.tool_calls.iter().any(|tc| is_tool_match(&tc.name, expected_tool));
                let used_next = session.turns.iter().skip(i + 1).take(2)
                    .any(|t| t.role == "assistant" && t.tool_calls.iter().any(|tc| is_tool_match(&tc.name, expected_tool)));
                if !used_here && !used_next {
                    // Make sure user didn't intervene between
                    let user_between = session.turns.iter().skip(i + 1).take(1)
                        .any(|t| t.role == "user" && t.text.as_ref().map(|s| !s.contains("tool_result")).unwrap_or(false));
                    if !user_between {
                        let preview_start = m.start().saturating_sub(20);
                        let preview_end = (m.end() + 60).min(text.len());
                        // Make sure we slice at char boundary
                        let start_char = text.char_indices().find(|(i, _)| *i >= preview_start).map(|(i, _)| i).unwrap_or(0);
                        let end_char = text.char_indices().find(|(i, _)| *i >= preview_end).map(|(i, _)| i).unwrap_or(text.len());
                        let preview: String = text[start_char..end_char].to_string();
                        flags.push(QualityFlag {
                            session_id: session.session_id.clone(),
                            turn: turn.turn_num,
                            flag_type: FlagType::MissedToolCall,
                            severity: Severity::Warning,
                            description: format!("Said \"{}...\" but no {} tool call followed", m.as_str().trim(), expected_tool),
                            evidence: preview,
                        });
                        break; // one missed-tool flag per turn
                    }
                }
            }
        }
    }
    flags
}

fn is_tool_match(actual: &str, expected: &str) -> bool {
    match expected {
        "Edit" => matches!(actual, "Edit" | "MultiEdit" | "Write" | "NotebookEdit"),
        "Read" => matches!(actual, "Read" | "Glob" | "Grep"),
        "Bash" => actual == "Bash",
        "Write" => matches!(actual, "Write" | "Edit"),
        "Grep" => matches!(actual, "Grep" | "Glob"),
        _ => actual == expected,
    }
}

// ─── 2. Repeated Failures ───────────────────────────────────────────
// Same tool + args called 3+ times.

fn detect_repeated_failures(session: &SessionSummary) -> Vec<QualityFlag> {
    let mut counts: HashMap<String, Vec<u32>> = HashMap::new();
    for turn in &session.turns {
        for tc in &turn.tool_calls {
            // Hash tool name + normalized input
            let key = format!("{}:{}", tc.name, normalize_input(&tc.input));
            counts.entry(key).or_default().push(turn.turn_num);
        }
    }
    let mut flags = Vec::new();
    for (key, turns) in counts {
        if turns.len() >= 3 {
            let parts: Vec<&str> = key.splitn(2, ':').collect();
            flags.push(QualityFlag {
                session_id: session.session_id.clone(),
                turn: *turns.last().unwrap_or(&0),
                flag_type: FlagType::RepeatedFailure,
                severity: Severity::Warning,
                description: format!("Called {} with same args {} times (turns: {:?})", parts[0], turns.len(), turns),
                evidence: parts.get(1).unwrap_or(&"").chars().take(200).collect::<String>(),
            });
        }
    }
    flags
}

fn normalize_input(v: &serde_json::Value) -> String {
    // Hash key parameters only, not the whole blob (content fields vary)
    if let Some(fp) = v.get("file_path").and_then(|x| x.as_str()) {
        return format!("fp={}", fp);
    }
    if let Some(c) = v.get("command").and_then(|x| x.as_str()) {
        return format!("cmd={}", c.chars().take(100).collect::<String>());
    }
    if let Some(q) = v.get("query").and_then(|x| x.as_str()) {
        return format!("q={}", q);
    }
    if let Some(u) = v.get("url").and_then(|x| x.as_str()) {
        return format!("url={}", u);
    }
    if let Some(p) = v.get("pattern").and_then(|x| x.as_str()) {
        return format!("pat={}", p);
    }
    serde_json::to_string(v).unwrap_or_default().chars().take(200).collect()
}

// ─── 3. Errors Ignored ──────────────────────────────────────────────
// Tool returned is_error=true, next assistant turn doesn't address it.

fn detect_errors_ignored(session: &SessionSummary) -> Vec<QualityFlag> {
    let mut flags = Vec::new();
    let error_mention_re = Regex::new(r"(?i)(error|fail|exit|broken|wrong|issue|problem|retry|try again|didn'?t work|fix)").unwrap();

    for (i, turn) in session.turns.iter().enumerate() {
        for r in &turn.tool_results {
            if !r.is_error { continue; }
            // SIGTERM (exit 143) and SIGKILL (137) are expected for deliberately-killed
            // background processes (run_in_background=true). Skip these false positives.
            if r.content_preview.contains("Exit code 143") || r.content_preview.contains("Exit code 137") {
                continue;
            }
            // Check the next assistant turn (if any) for error acknowledgement
            let next_assistant = session.turns.iter().skip(i + 1).find(|t| t.role == "assistant");
            let acknowledged = match next_assistant {
                Some(t) => {
                    let combined = format!("{} {}", t.text.clone().unwrap_or_default(), t.thinking.clone().unwrap_or_default());
                    error_mention_re.is_match(&combined)
                }
                None => false,
            };
            if !acknowledged {
                flags.push(QualityFlag {
                    session_id: session.session_id.clone(),
                    turn: turn.turn_num,
                    flag_type: FlagType::ErrorIgnored,
                    severity: Severity::Warning,
                    description: format!("Tool error not acknowledged in next turn"),
                    evidence: r.content_preview.chars().take(200).collect(),
                });
            }
        }
    }
    flags
}

// ─── 4. Abandoned Tools ─────────────────────────────────────────────
// tool_use appears but no tool_result with that id.

fn detect_abandoned_tools(session: &SessionSummary) -> Vec<QualityFlag> {
    let mut all_uses: HashMap<String, u32> = HashMap::new(); // id -> turn
    let mut all_results: HashSet<String> = HashSet::new();
    for turn in &session.turns {
        for tc in &turn.tool_calls {
            all_uses.insert(tc.id.clone(), turn.turn_num);
        }
        for r in &turn.tool_results {
            all_results.insert(r.tool_use_id.clone());
        }
    }
    let mut flags = Vec::new();
    for (id, turn) in &all_uses {
        if !all_results.contains(id) {
            flags.push(QualityFlag {
                session_id: session.session_id.clone(),
                turn: *turn,
                flag_type: FlagType::AbandonedTool,
                severity: Severity::Info,
                description: format!("tool_use {} has no matching tool_result", &id[..id.len().min(10)]),
                evidence: String::new(),
            });
        }
    }
    flags
}

// ─── 5. Loops ────────────────────────────────────────────────────────
// Same 3-turn sequence of (role, tool-name-signature) appears 2+ times.

fn detect_loops(session: &SessionSummary) -> Vec<QualityFlag> {
    if session.turns.len() < 6 { return Vec::new(); }
    let sigs: Vec<String> = session.turns.iter().map(|t| {
        let tools: Vec<&str> = t.tool_calls.iter().map(|tc| tc.name.as_str()).collect();
        format!("{}:{}", t.role, tools.join(","))
    }).collect();

    let mut seen_windows: HashMap<String, Vec<usize>> = HashMap::new();
    for i in 0..sigs.len().saturating_sub(2) {
        let window = sigs[i..i+3].join("|");
        seen_windows.entry(window).or_default().push(i);
    }
    let mut flags = Vec::new();
    for (window, positions) in seen_windows {
        if positions.len() >= 2 && !window.contains("user:") {
            // Filter out trivial empty-tool windows
            if window.contains("assistant:,") && window.matches(',').count() < 3 { continue; }
            // Sequential single-tool turns aren't a loop — they're just sequential work.
            // E.g. "assistant:|assistant:|assistant:Bash" or three Read turns in a row.
            // Only flag when the window contains a real repeated *pattern* of tools,
            // i.e. at least 2 distinct non-empty tool sigs in the 3-turn window.
            let parts: Vec<&str> = window.split('|').collect();
            let tool_sigs: Vec<&str> = parts.iter()
                .filter_map(|p| p.split_once(':').map(|(_, t)| t))
                .filter(|t| !t.is_empty())
                .collect();
            if tool_sigs.len() < 2 { continue; }
            let turn = session.turns.get(*positions.last().unwrap_or(&0))
                .map(|t| t.turn_num).unwrap_or(0);
            flags.push(QualityFlag {
                session_id: session.session_id.clone(),
                turn,
                flag_type: FlagType::LoopDetected,
                severity: Severity::Warning,
                description: format!("Repeated 3-turn sequence {} times", positions.len()),
                evidence: window.chars().take(200).collect(),
            });
        }
    }
    flags
}

// ─── 6. Hallucinated Files ──────────────────────────────────────────
// Assistant claims file exists but never reads it + it doesn't exist at cwd.

fn detect_hallucinated_files(session: &SessionSummary) -> Vec<QualityFlag> {
    let mut flags = Vec::new();
    let file_mention_re = Regex::new(r"`([a-zA-Z0-9_./\-]+\.(?:rs|ts|py|js|md|toml|json|yaml|yml|sh))`").unwrap();

    // Collect all file paths that were actually touched via tools
    let touched: HashSet<String> = session.files_touched.iter().cloned().collect();

    for turn in &session.turns {
        if turn.role != "assistant" { continue; }
        let Some(text) = &turn.text else { continue; };

        for cap in file_mention_re.captures_iter(text) {
            if let Some(m) = cap.get(1) {
                let claimed = m.as_str();
                if claimed.len() < 6 || !claimed.contains('/') { continue; } // skip simple names

                // Was it touched in this session?
                let was_touched = touched.iter().any(|t| t.ends_with(claimed) || t.contains(claimed));
                if was_touched { continue; }

                // Does it exist on disk relative to cwd?
                let candidate = session.cwd.join(claimed);
                if candidate.exists() { continue; }

                flags.push(QualityFlag {
                    session_id: session.session_id.clone(),
                    turn: turn.turn_num,
                    flag_type: FlagType::HallucinatedFile,
                    severity: Severity::Warning,
                    description: format!("Referenced `{}` but file not touched and not on disk", claimed),
                    evidence: text.chars().skip(m.start().saturating_sub(50)).take(150).collect(),
                });
                if flags.iter().filter(|f| matches!(f.flag_type, FlagType::HallucinatedFile) && f.turn == turn.turn_num).count() >= 3 {
                    break; // cap to 3 per turn
                }
            }
        }
    }
    flags
}

// ─── 7. Self-Contradictions ─────────────────────────────────────────
// Very simple heuristic: "X works" then later "X is broken" with same short noun.

fn detect_self_contradictions(_session: &SessionSummary) -> Vec<QualityFlag> {
    // Placeholder — this heuristic is noisy and would need an NLP library to do well.
    // Left as a stub for Phase I-F (LLM-based quality check).
    Vec::new()
}
