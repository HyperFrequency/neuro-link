use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::path::Path;

use crate::sessions;

pub fn tool_defs() -> Vec<Value> {
    vec![
        json!({
            "name": "nlr_sessions_list",
            "description": "List recent Claude Code sessions with turn/token/cost metadata",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "days": {"type": "integer", "description": "Look back N days (default 7)"}
                }
            }
        }),
        json!({
            "name": "nlr_sessions_scan_quality",
            "description": "Run rule-based quality scan on Claude Code sessions. Returns flags: missed_tool_call, repeated_failure, error_ignored, abandoned_tool, loop_detected, hallucinated_file",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "days": {"type": "integer", "description": "Look back N days (default 7)"}
                }
            }
        }),
        json!({
            "name": "nlr_sessions_skill_usage",
            "description": "Aggregate which skills were invoked across sessions, count of invocations per skill",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "days": {"type": "integer", "description": "Look back N days (default 30)"}
                }
            }
        }),
        json!({
            "name": "nlr_sessions_tool_usage",
            "description": "Histogram of tool calls across sessions (most-called tool first)",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "days": {"type": "integer", "description": "Look back N days (default 30)"}
                }
            }
        }),
        json!({
            "name": "nlr_sessions_parse",
            "description": "Parse Claude Code session logs and export to vault markdown. Returns number of files written.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "since": {"type": "string", "description": "YYYY-MM-DD (optional)"},
                    "vault": {"type": "string", "description": "Override vault path (optional)"}
                }
            }
        }),
    ]
}

pub fn call(name: &str, args: &Value, root: &Path) -> Result<String> {
    let claude_root = sessions::claude_root();
    let days = args.get("days").and_then(|v| v.as_u64()).unwrap_or(7);
    let since = chrono::Utc::now() - chrono::Duration::days(days as i64);

    match name {
        "nlr_sessions_list" => {
            let sessions_list = sessions::parser::parse_sessions_since(&claude_root, since)?;
            let out: Vec<Value> = sessions_list.iter().map(|s| {
                json!({
                    "session_id": s.session_id,
                    "project": s.project,
                    "started": s.started,
                    "ended": s.ended,
                    "turns": s.turns.len(),
                    "tools": s.tools_used.values().sum::<u32>(),
                    "skills": s.skills_invoked.len(),
                    "model": s.model,
                    "cost_usd": s.tokens.cost_usd_estimate,
                })
            }).collect();
            Ok(serde_json::to_string_pretty(&out)?)
        }

        "nlr_sessions_scan_quality" => {
            let mut sessions_list = sessions::parser::parse_sessions_since(&claude_root, since)?;
            let mut all_flags: Vec<sessions::quality::QualityFlag> = Vec::new();
            let mut flag_counts: std::collections::HashMap<&str, u32> = Default::default();

            for s in sessions_list.iter_mut() {
                s.quality_flags = sessions::quality::analyze(s);
                for f in &s.quality_flags {
                    *flag_counts.entry(f.flag_type.as_str()).or_default() += 1;
                }
                all_flags.extend_from_slice(&s.quality_flags);
            }

            let mut worst: Vec<_> = sessions_list.iter()
                .filter(|s| !s.quality_flags.is_empty())
                .collect();
            worst.sort_by(|a, b| b.quality_flags.len().cmp(&a.quality_flags.len()));

            Ok(serde_json::to_string_pretty(&json!({
                "days": days,
                "sessions_analyzed": sessions_list.len(),
                "total_flags": all_flags.len(),
                "flags_by_type": flag_counts,
                "worst_offenders": worst.iter().take(10).map(|s| json!({
                    "session_id": s.session_id,
                    "started": s.started,
                    "flag_count": s.quality_flags.len(),
                })).collect::<Vec<_>>(),
                "sample_flags": all_flags.iter().take(20).collect::<Vec<_>>(),
            }))?)
        }

        "nlr_sessions_skill_usage" => {
            let sessions_list = sessions::parser::parse_sessions_since(&claude_root, since)?;
            let mut counts: std::collections::HashMap<String, u32> = Default::default();
            let mut sessions_per_skill: std::collections::HashMap<String, std::collections::HashSet<String>> = Default::default();
            for s in &sessions_list {
                for sk in &s.skills_invoked {
                    *counts.entry(sk.skill.clone()).or_default() += 1;
                    sessions_per_skill.entry(sk.skill.clone()).or_default().insert(s.session_id.clone());
                }
            }
            let mut out: Vec<_> = counts.into_iter().map(|(skill, count)| {
                let sessions = sessions_per_skill.get(&skill).map(|s| s.len()).unwrap_or(0);
                json!({ "skill": skill, "invocations": count, "unique_sessions": sessions })
            }).collect();
            out.sort_by(|a, b| b["invocations"].as_u64().cmp(&a["invocations"].as_u64()));
            Ok(serde_json::to_string_pretty(&out)?)
        }

        "nlr_sessions_tool_usage" => {
            let sessions_list = sessions::parser::parse_sessions_since(&claude_root, since)?;
            let mut counts: std::collections::HashMap<String, u32> = Default::default();
            for s in &sessions_list {
                for (tool, n) in &s.tools_used {
                    *counts.entry(tool.clone()).or_default() += n;
                }
            }
            let mut out: Vec<_> = counts.into_iter().collect();
            out.sort_by(|a, b| b.1.cmp(&a.1));
            let out_json: Vec<_> = out.into_iter().map(|(tool, n)| json!({"tool": tool, "calls": n})).collect();
            Ok(serde_json::to_string_pretty(&out_json)?)
        }

        "nlr_sessions_parse" => {
            let since_arg = args.get("since").and_then(|v| v.as_str());
            let vault_arg = args.get("vault").and_then(|v| v.as_str()).map(std::path::PathBuf::from);

            let vault_path = vault_arg.unwrap_or_else(|| {
                let config = root.join("config/neuro-link.md");
                if let Ok(content) = std::fs::read_to_string(&config) {
                    for line in content.lines() {
                        if let Some(v) = line.strip_prefix("obsidian_vault:") {
                            let p = v.trim();
                            if !p.is_empty() {
                                return std::path::PathBuf::from(p);
                            }
                        }
                    }
                }
                root.to_path_buf()
            });

            let mut parsed = if let Some(s) = since_arg {
                let dt = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")?
                    .and_hms_opt(0, 0, 0)
                    .unwrap()
                    .and_utc();
                sessions::parser::parse_sessions_since(&claude_root, dt)?
            } else {
                sessions::parser::parse_all_sessions(&claude_root)?
            };

            for s in parsed.iter_mut() {
                s.quality_flags = sessions::quality::analyze(s);
            }

            let mut written = 0;
            for s in &parsed {
                if sessions::markdown::write_session_markdown(s, &vault_path).is_ok() {
                    written += 1;
                }
            }

            Ok(serde_json::to_string_pretty(&json!({
                "parsed": parsed.len(),
                "written": written,
                "vault": vault_path.to_string_lossy(),
            }))?)
        }

        _ => bail!("Unknown sessions tool: {name}"),
    }
}
