use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

pub fn tool_defs() -> Vec<Value> {
    vec![
        json!({"name":"nlr_harness_dispatch","description":"Send a message/task to another harness","inputSchema":{"type":"object","properties":{"to":{"type":"string","description":"Target harness name"},"task":{"type":"string"},"priority":{"type":"integer"}},"required":["to","task"]}}),
        json!({"name":"nlr_harness_list","description":"List configured harnesses","inputSchema":{"type":"object","properties":{}}}),
    ]
}

pub fn call(name: &str, args: &Value, root: &Path) -> Result<String> {
    match name {
        "nlr_harness_list" => {
            let config_path = root.join("config/harness-harness-comms.md");
            if config_path.exists() {
                let fm = crate::config::parse_frontmatter(&config_path)?;
                Ok(serde_json::to_string_pretty(&fm)?)
            } else {
                Ok("No harness config found".into())
            }
        }
        "nlr_harness_dispatch" => {
            let to = args["to"].as_str().unwrap_or("unknown");
            let task = args["task"].as_str().unwrap_or("");
            let priority = args.get("priority").and_then(|v| v.as_u64()).unwrap_or(3);
            // Write dispatch message to the comms directory
            let comms_dir = root.join("06-self-improvement-recursive/harness-to-harness-comms");
            fs::create_dir_all(&comms_dir)?;
            let ts = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
            let msg = json!({"from":"neuro-link-mcp","to":to,"task":task,"priority":priority,"timestamp":chrono::Utc::now().to_rfc3339(),"status":"dispatched"});
            fs::write(comms_dir.join(format!("{ts}-to-{to}.json")), serde_json::to_string_pretty(&msg)?)?;
            Ok(format!("Dispatched to {to}: {task}"))
        }
        _ => bail!("Unknown harness tool: {name}"),
    }
}
