use super::*;
use anyhow::{Context, Result};
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};

/// Generate Obsidian-native markdown with frontmatter for a session.
pub fn session_to_markdown(s: &SessionSummary) -> String {
    let mut out = String::new();

    // ── Frontmatter ──
    let skills: Vec<String> = s.skills_invoked.iter().map(|sk| format!("\"{}\"", sk.skill)).collect();
    let tools_list: Vec<String> = {
        let mut v: Vec<(&String, &u32)> = s.tools_used.iter().collect();
        v.sort_by(|a, b| b.1.cmp(a.1));
        v.iter().map(|(name, count)| format!("{}: {}", name, count)).collect()
    };
    let files_list: Vec<String> = s.files_touched.iter().map(|f| format!("\"{}\"", f)).collect();
    let tags: Vec<&str> = {
        let mut t = vec!["claude-code", "session"];
        // Inherit first word of project as tag
        if let Some(first_tag) = s.project.split('-').next_back() {
            if !first_tag.is_empty() { t.push(first_tag); }
        }
        t
    };

    let duration_min = (s.ended - s.started).num_minutes();
    let title = format!(
        "Session {} — {}",
        s.started.format("%Y-%m-%d %H:%M"),
        short_project(&s.project)
    );

    // Missing tool call count etc for frontmatter
    let flag_counts = count_flags(&s.quality_flags);

    writeln!(out, "---").unwrap();
    writeln!(out, "title: \"{}\"", title).unwrap();
    writeln!(out, "session_id: {}", s.session_id).unwrap();
    writeln!(out, "project: {}", s.project).unwrap();
    writeln!(out, "model: {}", s.model).unwrap();
    writeln!(out, "cwd: {}", s.cwd.display()).unwrap();
    if let Some(br) = &s.git_branch { writeln!(out, "git_branch: {}", br).unwrap(); }
    writeln!(out, "started: {}", s.started.to_rfc3339()).unwrap();
    writeln!(out, "ended: {}", s.ended.to_rfc3339()).unwrap();
    writeln!(out, "duration_minutes: {}", duration_min).unwrap();
    writeln!(out, "turns: {}", s.turns.len()).unwrap();
    writeln!(out, "skills_invoked: [{}]", skills.join(", ")).unwrap();
    writeln!(out, "tools_used: {{{}}}", tools_list.join(", ")).unwrap();
    writeln!(out, "tokens:").unwrap();
    writeln!(out, "  input: {}", s.tokens.input).unwrap();
    writeln!(out, "  output: {}", s.tokens.output).unwrap();
    writeln!(out, "  cache_read: {}", s.tokens.cache_read).unwrap();
    writeln!(out, "  cache_creation: {}", s.tokens.cache_creation).unwrap();
    writeln!(out, "cost_usd_estimate: {:.2}", s.tokens.cost_usd_estimate).unwrap();
    writeln!(out, "quality:").unwrap();
    writeln!(out, "  total_flags: {}", s.quality_flags.len()).unwrap();
    for (kind, n) in flag_counts.iter() {
        writeln!(out, "  {}: {}", kind, n).unwrap();
    }
    writeln!(out, "files_touched: [{}]", files_list.join(", ")).unwrap();
    writeln!(out, "subagents: {}", s.subagents.len()).unwrap();
    writeln!(out, "external_fetches: {}", s.external_fetches.len()).unwrap();
    writeln!(out, "tags: [{}]", tags.join(", ")).unwrap();
    writeln!(out, "---").unwrap();
    writeln!(out).unwrap();

    // ── Header ──
    writeln!(out, "# {}", title).unwrap();
    writeln!(out).unwrap();

    // ── Intent & Outcome ──
    if let Some(intent) = first_user_prompt(s) {
        writeln!(out, "**Intent:** {}", truncate_to_sentences(&intent, 3)).unwrap();
        writeln!(out).unwrap();
    }
    if let Some(outcome) = last_assistant_text(s) {
        writeln!(out, "**Outcome:** {}", truncate_to_sentences(&outcome, 3)).unwrap();
        writeln!(out).unwrap();
    }

    // ── Skills invoked ──
    if !s.skills_invoked.is_empty() {
        writeln!(out, "## Skills Invoked").unwrap();
        for sk in &s.skills_invoked {
            writeln!(out, "- [[skill-{}]] @ turn {}", slugify(&sk.skill.trim_start_matches('/')), sk.turn).unwrap();
        }
        writeln!(out).unwrap();
    }

    // ── Quality flags (surface at top) ──
    if !s.quality_flags.is_empty() {
        writeln!(out, "## Quality Flags ({})", s.quality_flags.len()).unwrap();
        for flag in &s.quality_flags {
            let icon = match flag.severity {
                quality::Severity::Critical => "🔴",
                quality::Severity::Warning => "⚠️",
                quality::Severity::Info => "ℹ️",
            };
            writeln!(out, "- {} **{}** (turn {}): {}", icon, flag.flag_type.as_str(), flag.turn, flag.description).unwrap();
            if !flag.evidence.is_empty() {
                writeln!(out, "  > {}", flag.evidence.lines().next().unwrap_or(&flag.evidence)).unwrap();
            }
        }
        writeln!(out).unwrap();
    }

    // ── Files modified ──
    if !s.files_touched.is_empty() {
        writeln!(out, "## Files Touched").unwrap();
        for f in &s.files_touched {
            // Make wikilink-compatible path relative to cwd
            let rel = f.strip_prefix(&format!("{}/", s.cwd.display())).unwrap_or(f);
            writeln!(out, "- [[{}]]", rel).unwrap();
        }
        writeln!(out).unwrap();
    }

    // ── Subagents ──
    if !s.subagents.is_empty() {
        writeln!(out, "## Subagent Invocations ({})", s.subagents.len()).unwrap();
        for sub in &s.subagents {
            writeln!(out, "- Turn {}: `{}` ({})", sub.turn, sub.description, sub.subagent_type).unwrap();
        }
        writeln!(out).unwrap();
    }

    // ── External resources ──
    if !s.external_fetches.is_empty() {
        writeln!(out, "## External Resources").unwrap();
        for url in &s.external_fetches {
            writeln!(out, "- {}", url).unwrap();
        }
        writeln!(out).unwrap();
    }

    // ── Timeline ──
    writeln!(out, "## Timeline").unwrap();
    writeln!(out).unwrap();
    for turn in &s.turns {
        render_turn(&mut out, turn);
    }

    out
}

fn render_turn(out: &mut String, turn: &Turn) {
    let token_info = if let Some(t) = &turn.tokens {
        format!(" (in: {}, out: {})", t.input, t.output)
    } else {
        String::new()
    };
    writeln!(out, "### Turn {} — {}{}", turn.turn_num, turn.role, token_info).unwrap();

    if let Some(skill) = &turn.skill_command {
        writeln!(out, "*Invoked skill:* `{}`", skill).unwrap();
        writeln!(out).unwrap();
    }

    if let Some(thinking) = &turn.thinking {
        writeln!(out, "<details><summary>💭 Thinking ({} chars)</summary>", thinking.len()).unwrap();
        writeln!(out).unwrap();
        writeln!(out, "{}", thinking).unwrap();
        writeln!(out, "</details>").unwrap();
        writeln!(out).unwrap();
    }

    if let Some(text) = &turn.text {
        // Strip common noise (command-name wrappers, tool result passthrough)
        let cleaned = clean_text(text);
        if !cleaned.is_empty() {
            writeln!(out, "> {}", truncate_for_timeline(&cleaned, 400).replace('\n', "\n> ")).unwrap();
            writeln!(out).unwrap();
        }
    }

    if !turn.tool_calls.is_empty() {
        writeln!(out, "**Tools called ({}):**", turn.tool_calls.len()).unwrap();
        for tc in &turn.tool_calls {
            let input_preview = match tc.input.get("description").and_then(|v| v.as_str()) {
                Some(d) => d.to_string(),
                None => {
                    // Common inputs: file_path, command, url
                    if let Some(p) = tc.input.get("file_path").and_then(|v| v.as_str()) {
                        p.to_string()
                    } else if let Some(c) = tc.input.get("command").and_then(|v| v.as_str()) {
                        truncate_for_timeline(c, 80)
                    } else if let Some(u) = tc.input.get("url").and_then(|v| v.as_str()) {
                        u.to_string()
                    } else if let Some(q) = tc.input.get("query").and_then(|v| v.as_str()) {
                        q.to_string()
                    } else if let Some(p) = tc.input.get("pattern").and_then(|v| v.as_str()) {
                        p.to_string()
                    } else {
                        String::new()
                    }
                }
            };
            writeln!(out, "- `{}` — {}", tc.name, input_preview).unwrap();
        }
        writeln!(out).unwrap();
    }

    if !turn.tool_results.is_empty() {
        let errors = turn.tool_results.iter().filter(|r| r.is_error).count();
        let marker = if errors > 0 { format!("**⚠️ {} errors / {} results**", errors, turn.tool_results.len()) }
            else { format!("**{} tool results**", turn.tool_results.len()) };
        writeln!(out, "{}", marker).unwrap();
        for r in &turn.tool_results {
            let status = if r.is_error { "❌" } else { "✓" };
            let short_id: String = r.tool_use_id.chars().take(10).collect();
            writeln!(out, "- {} `{}`: {} bytes", status, short_id, r.content_bytes).unwrap();
        }
        writeln!(out).unwrap();
    }
}

/// Write the markdown to `<vault>/sessions/<filename>.md`
/// Returns the path written.
pub fn write_session_markdown(s: &SessionSummary, vault_path: &Path) -> Result<PathBuf> {
    let sessions_dir = vault_path.join("sessions");
    fs::create_dir_all(&sessions_dir).with_context(|| format!("mkdir {}", sessions_dir.display()))?;

    let filename = format!(
        "{}-{}-{}.md",
        s.started.format("%Y%m%d-%H%M"),
        short_project(&s.project),
        &s.session_id[..s.session_id.len().min(8)]
    );
    let path = sessions_dir.join(&filename);

    let md = session_to_markdown(s);
    fs::write(&path, md).with_context(|| format!("writing {}", path.display()))?;
    Ok(path)
}

// ── helpers ──

fn short_project(project: &str) -> String {
    // e.g. "-Users-DanBot-Desktop-HyperFrequency-neuro-link" → "neuro-link"
    project.split('-').rev()
        .take_while(|s| !matches!(*s, "Desktop" | "Users" | "HyperFrequency" | ""))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("-")
}

fn slugify(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c.to_ascii_lowercase() } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn first_user_prompt(s: &SessionSummary) -> Option<String> {
    s.turns.iter().find_map(|t| {
        if t.role == "user" {
            t.text.as_ref().map(|text| clean_text(text))
                .filter(|c| !c.is_empty() && !c.starts_with("<command-name>") && !c.contains("tool_result"))
        } else { None }
    })
}

fn last_assistant_text(s: &SessionSummary) -> Option<String> {
    s.turns.iter().rev().find_map(|t| {
        if t.role == "assistant" {
            t.text.clone().filter(|text| !text.is_empty())
        } else { None }
    })
}

fn clean_text(text: &str) -> String {
    // Strip command-name tags, system reminders
    let mut cleaned = text.to_string();
    let noise_patterns = [
        r"<command-name>[^<]*</command-name>",
        r"<command-message>[^<]*</command-message>",
        r"<command-args>[^<]*</command-args>",
        r"<local-command-caveat>[^<]*</local-command-caveat>",
        r"<local-command-stdout>[^<]*</local-command-stdout>",
        r"<system-reminder>[\s\S]*?</system-reminder>",
    ];
    for pat in &noise_patterns {
        if let Ok(re) = regex::Regex::new(pat) {
            cleaned = re.replace_all(&cleaned, "").to_string();
        }
    }
    cleaned.trim().to_string()
}

fn truncate_for_timeline(s: &str, max: usize) -> String {
    if s.chars().count() <= max { return s.to_string(); }
    let truncated: String = s.chars().take(max).collect();
    format!("{}…", truncated)
}

fn truncate_to_sentences(s: &str, n: usize) -> String {
    let s = s.replace('\n', " ");
    let mut out = String::new();
    let mut count = 0;
    for part in s.split_inclusive(|c: char| c == '.' || c == '!' || c == '?') {
        out.push_str(part);
        count += 1;
        if count >= n { break; }
    }
    if out.chars().count() > 300 {
        let t: String = out.chars().take(297).collect();
        return format!("{}...", t);
    }
    out.trim().to_string()
}

fn count_flags(flags: &[quality::QualityFlag]) -> Vec<(&'static str, u32)> {
    let mut counts: std::collections::HashMap<&'static str, u32> = Default::default();
    for f in flags {
        *counts.entry(f.flag_type.as_str()).or_default() += 1;
    }
    let mut v: Vec<_> = counts.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    v
}
