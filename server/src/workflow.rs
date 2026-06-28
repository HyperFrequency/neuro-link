use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

const STATES: &[&str] = &[
    "signal",
    "impression",
    "insight",
    "framework",
    "lens",
    "synthesis",
    "index",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    pub id: String,
    pub name: String,
    pub state: String,
    pub created: String,
    pub transitions: Vec<TransitionRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransitionRecord {
    pub from: String,
    pub to: String,
    pub timestamp: String,
}

fn workflows_path(root: &Path) -> std::path::PathBuf {
    root.join("state/workflows.json")
}

fn load_workflows(root: &Path) -> Result<Vec<Workflow>> {
    let path = workflows_path(root);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&content)?)
}

fn save_workflows(root: &Path, workflows: &[Workflow]) -> Result<()> {
    let path = workflows_path(root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, serde_json::to_string_pretty(workflows)?)?;
    Ok(())
}

pub fn create_workflow(root: &Path, name: &str) -> Result<String> {
    let mut workflows = load_workflows(root)?;
    let id = uuid::Uuid::new_v4().to_string()[..8].to_string();
    let wf = Workflow {
        id: id.clone(),
        name: name.to_string(),
        state: STATES[0].to_string(),
        created: chrono::Utc::now().to_rfc3339(),
        transitions: Vec::new(),
    };
    workflows.push(wf);
    save_workflows(root, &workflows)?;
    Ok(id)
}

pub fn transition(root: &Path, id: &str, to_state: &str) -> Result<()> {
    if !STATES.contains(&to_state) {
        bail!(
            "Invalid state: {to_state}. Valid: {}",
            STATES.join(" -> ")
        );
    }

    let mut workflows = load_workflows(root)?;
    let wf = workflows
        .iter_mut()
        .find(|w| w.id == id)
        .ok_or_else(|| anyhow::anyhow!("Workflow not found: {id}"))?;

    let current_idx = STATES
        .iter()
        .position(|s| *s == wf.state)
        .unwrap_or(0);
    let target_idx = STATES
        .iter()
        .position(|s| *s == to_state)
        .unwrap_or(0);

    if target_idx != current_idx + 1 {
        bail!(
            "Cannot transition from '{}' to '{to_state}'. Next valid state: '{}'",
            wf.state,
            STATES.get(current_idx + 1).unwrap_or(&"(end)")
        );
    }

    wf.transitions.push(TransitionRecord {
        from: wf.state.clone(),
        to: to_state.to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    });
    wf.state = to_state.to_string();
    save_workflows(root, &workflows)?;
    Ok(())
}

pub fn list_workflows(root: &Path) -> Result<Vec<Workflow>> {
    load_workflows(root)
}
