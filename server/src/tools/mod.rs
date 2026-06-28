//! Tool registry: dispatches MCP tool calls to handlers.

pub mod wiki;
pub mod rag;
pub mod rag_verified;
pub mod ontology;
pub mod ingest;
pub mod pdf_ingest;
pub mod tasks;
pub mod harness;
pub mod scan;
pub mod llm_logs;
pub mod hooks_log;
pub mod sessions_tools;
pub mod session_state;
pub mod graph_traverse;
pub mod math;
pub mod traces;
pub mod external;
pub mod dispatcher;

use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::path::PathBuf;

use crate::state::StateManager;

pub struct ToolRegistry {
    root: PathBuf,
    state: StateManager,
}

impl ToolRegistry {
    pub fn new(root: PathBuf) -> Self {
        let state = StateManager::new(root.clone());
        Self { root, state }
    }

    pub fn root(&self) -> &std::path::Path {
        &self.root
    }

    pub fn list_tools(&self) -> Vec<Value> {
        let mut tools = Vec::new();
        tools.extend(wiki::tool_defs());
        tools.extend(rag::tool_defs());
        tools.extend(rag_verified::tool_defs());
        tools.extend(ontology::tool_defs());
        tools.extend(ingest::tool_defs());
        tools.extend(pdf_ingest::tool_defs());
        tools.extend(tasks::tool_defs());
        tools.extend(harness::tool_defs());
        tools.extend(scan::tool_defs());
        tools.extend(llm_logs::tool_defs());
        tools.extend(hooks_log::tool_defs());
        tools.extend(sessions_tools::tool_defs());
        tools.extend(session_state::tool_defs());
        tools.extend(graph_traverse::tool_defs());
        tools.extend(math::tool_defs());
        tools.extend(traces::tool_defs());
        // State tools
        tools.push(json!({"name": "nlr_state_heartbeat", "description": "Read or update heartbeat status", "inputSchema": {"type": "object", "properties": {"action": {"type": "string", "enum": ["read", "update"]}}, "required": ["action"]}}));
        tools.push(json!({"name": "nlr_state_log", "description": "Append to session log", "inputSchema": {"type": "object", "properties": {"tool": {"type": "string"}, "exit_code": {"type": "integer"}}, "required": ["tool"]}}));
        tools.push(json!({"name": "nlr_config_read", "description": "Read a config file frontmatter", "inputSchema": {"type": "object", "properties": {"name": {"type": "string", "description": "Config filename without extension"}}, "required": ["name"]}}));
        tools.push(json!({"name": "nlr_worker_status", "description": "Self-improvement worker status: last run time, proposals generated count, current metrics", "inputSchema": {"type": "object", "properties": {}}}));
        tools
    }

    pub fn call(&self, name: &str, args: &Value) -> Result<String> {
        match name {
            n if n.starts_with("nlr_wiki_") => wiki::call(n, args, &self.root),
            "nlr_rag_query_verified" => rag_verified::call(name, args, &self.root),
            n if n.starts_with("nlr_rag_") => rag::call(n, args, &self.root),
            n if n.starts_with("nlr_ontology_") => ontology::call(n, args, &self.root),
            n if n.starts_with("nlr_pdf_") => pdf_ingest::call(n, args, &self.root),
            n if n.starts_with("nlr_ingest") => ingest::call(n, args, &self.root),
            n if n.starts_with("nlr_task_") => tasks::call(n, args, &self.root),
            n if n.starts_with("nlr_harness_") => harness::call(n, args, &self.root),
            n if n.starts_with("nlr_scan_") => scan::call(n, args, &self.root, &self.state),
            n if n.starts_with("nlr_llm_logs_") => llm_logs::call(n, args, &self.root),
            n if n.starts_with("nlr_hooks_log_") => hooks_log::call(n, args, &self.root),
            n if n.starts_with("nlr_sessions_") => sessions_tools::call(n, args, &self.root),
            "nlr_session_context" => session_state::call("nlr_session_context", args, &self.root),
            n if n.starts_with("nlr_graph_") => graph_traverse::call(n, args, &self.root),
            n if n.starts_with("nlr_math_") => math::call(n, args, &self.root),
            n if n.starts_with("nlr_trace_") => traces::call(n, args, &self.root),
            "nlr_state_heartbeat" => {
                let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("read");
                match action {
                    "read" => Ok(serde_json::to_string_pretty(&self.state.read_heartbeat()?)?),
                    "update" => { self.state.update_heartbeat("ok", &[])?; Ok("Heartbeat updated".into()) }
                    _ => bail!("Unknown action: {action}"),
                }
            }
            "nlr_state_log" => {
                let tool = args.get("tool").and_then(|v| v.as_str()).unwrap_or("unknown");
                let exit_code = args.get("exit_code").and_then(|v| v.as_i64()).map(|v| v as i32);
                self.state.append_session_log(tool, exit_code)?;
                Ok("Logged".into())
            }
            "nlr_config_read" => {
                let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("neuro-link");
                let path = self.root.join("config").join(format!("{name}.md"));
                let fm = crate::config::parse_frontmatter(&path)?;
                Ok(serde_json::to_string_pretty(&fm)?)
            }
            "nlr_worker_status" => {
                let snap = crate::worker::global_state().snapshot();
                Ok(serde_json::to_string_pretty(&snap)?)
            }
            _ => bail!("Unknown tool: {name}"),
        }
    }
}
