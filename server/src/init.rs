use anyhow::Result;
use std::fs;
use std::path::Path;

const DIRS: &[&str] = &[
    "00-raw",
    "01-sorted/arxiv",
    "01-sorted/medium",
    "01-sorted/github",
    "01-sorted/huggingface",
    "01-sorted/docs",
    "01-sorted/books",
    "02-KB-main",
    "03-ontology-main/domain",
    "03-ontology-main/agent",
    "03-ontology-main/workflow",
    "04-KB-agents-workflows",
    "05-insights-gaps",
    "06-progress-reports",
    "06-self-improvement-recursive/harness-to-harness-comms",
    "07-neuro-link-task",
    "08-code-docs",
    "09-business-docs",
    "config",
    "state",
    "secrets",
];

pub fn init(root: &Path) -> Result<()> {
    for d in DIRS {
        fs::create_dir_all(root.join(d))?;
    }

    let schema = root.join("02-KB-main/schema.md");
    if !schema.exists() {
        fs::write(
            &schema,
            "---\ntitle: Wiki Schema\n---\n\n# Wiki Page Schema\n\nSee CLAUDE.md for conventions.\n",
        )?;
    }

    let index = root.join("02-KB-main/index.md");
    if !index.exists() {
        fs::write(&index, "# Wiki Index\n\n*Auto-generated.*\n")?;
    }

    let log = root.join("02-KB-main/log.md");
    if !log.exists() {
        fs::write(&log, "# Wiki Change Log\n")?;
    }

    let config = root.join("config/neuro-link.md");
    if !config.exists() {
        fs::write(
            &config,
            "---\nversion: 1\nname: neuro-link\n---\n\n# neuro-link-recursive config\n\nMaster configuration file.\n",
        )?;
    }

    let heartbeat = root.join("state/heartbeat.json");
    if !heartbeat.exists() {
        fs::write(
            &heartbeat,
            serde_json::to_string_pretty(&serde_json::json!({
                "status": "initialized",
                "last_check": chrono::Utc::now().to_rfc3339(),
                "errors": []
            }))?,
        )?;
    }

    // Persist nlr_root for future resolution
    if let Some(home) = dirs::home_dir() {
        let state_dir = home.join(".claude/state");
        fs::create_dir_all(&state_dir)?;
        fs::write(state_dir.join("nlr_root"), root.display().to_string())?;
    }

    Ok(())
}
