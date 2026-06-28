use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use futures_util::stream::{self, Stream};
use serde_json::{json, Value};
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;
use walkdir::WalkDir;

use crate::protocol::{JsonRpcRequest, JsonRpcResponse};
use crate::tools::ToolRegistry;

/// GET /mcp — Streamable HTTP transport server→client SSE channel.
/// Clients (K-Dense web, Claude Desktop streaming transport, etc.) open this
/// to receive notifications. We emit a ready event then keep-alive.
pub async fn handle_mcp_sse() -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let initial = Event::default()
        .event("ready")
        .data(r#"{"protocol":"mcp","version":"2025-03-26"}"#);
    let s = stream::once(async { Ok::<Event, Infallible>(initial) });
    Sse::new(s).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

pub async fn handle_mcp(
    State(registry): State<Arc<ToolRegistry>>,
    Json(request): Json<JsonRpcRequest>,
) -> Json<Value> {
    let id = request.id.clone();
    let root = registry.root();

    let response = match request.method.as_str() {
        "initialize" => JsonRpcResponse::success(
            id,
            json!({
                "protocolVersion": "2025-03-26",
                "capabilities": {
                    "tools": {},
                    "resources": { "listChanged": false },
                    "prompts": { "listChanged": false }
                },
                "serverInfo": {
                    "name": "neuro-link-recursive",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        ),
        "tools/list" => {
            let tools = registry.list_tools();
            JsonRpcResponse::success(id, json!({ "tools": tools }))
        }
        "tools/call" => {
            let params = request.params.as_ref();
            let tool_name = params
                .and_then(|p| p.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let arguments = params
                .and_then(|p| p.get("arguments"))
                .cloned()
                .unwrap_or(Value::Object(Default::default()));

            match registry.call(tool_name, &arguments) {
                Ok(result) => JsonRpcResponse::success(
                    id,
                    json!({
                        "content": [{ "type": "text", "text": result }]
                    }),
                ),
                Err(e) => JsonRpcResponse::success(
                    id,
                    json!({
                        "content": [{ "type": "text", "text": format!("Error: {e}") }],
                        "isError": true
                    }),
                ),
            }
        }
        "resources/list" => {
            let allowed = crate::config::allowed_paths(root);
            let skip = ["schema.md", "index.md", "log.md"];
            let mut resources = Vec::new();
            for dir_name in &allowed {
                let dir = root.join(dir_name);
                if !dir.is_dir() { continue; }
                for entry in WalkDir::new(&dir).into_iter().filter_map(|e| e.ok()) {
                    let path = entry.path();
                    if path.is_file()
                        && path.extension().is_some_and(|e| e == "md")
                        && !skip.iter().any(|s| path.file_name().is_some_and(|f| f == *s))
                    {
                        let rel = path.strip_prefix(root).unwrap_or(path).display().to_string();
                        resources.push(json!({
                            "uri": format!("nlr://{rel}"),
                            "name": rel,
                            "mimeType": "text/markdown"
                        }));
                    }
                }
            }
            JsonRpcResponse::success(id, json!({ "resources": resources }))
        }
        "resources/read" => {
            let uri = request.params.as_ref()
                .and_then(|p| p.get("uri"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let rel_path = uri.strip_prefix("nlr://").unwrap_or(uri);
            // Block traversal: reject .., absolute paths, and null bytes
            if rel_path.contains("..") || rel_path.starts_with('/') || rel_path.contains('\0') {
                return Json(serde_json::to_value(JsonRpcResponse::error(
                    id, -32602, "Invalid path: traversal not allowed".into(),
                )).unwrap_or(json!(null)));
            }
            // Check allowed_paths
            if !crate::config::is_path_allowed(root, rel_path) {
                return Json(serde_json::to_value(JsonRpcResponse::error(
                    id, -32602, "Access denied: path not in allowed_paths".into(),
                )).unwrap_or(json!(null)));
            }
            let full_path = root.join(rel_path);
            // Canonicalize and verify resolved path is under root
            let canonical = match full_path.canonicalize() {
                Ok(p) => p,
                Err(e) => {
                    return Json(serde_json::to_value(JsonRpcResponse::error(
                        id, -32602, format!("Resource not found: {e}"),
                    )).unwrap_or(json!(null)));
                }
            };
            let root_canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
            if !canonical.starts_with(&root_canonical) {
                return Json(serde_json::to_value(JsonRpcResponse::error(
                    id, -32602, "Access denied: path outside data root".into(),
                )).unwrap_or(json!(null)));
            }
            match std::fs::read_to_string(&canonical) {
                Ok(content) => JsonRpcResponse::success(id, json!({
                    "contents": [{ "uri": uri, "mimeType": "text/markdown", "text": content }]
                })),
                Err(e) => JsonRpcResponse::error(id, -32602, format!("Resource not found: {e}")),
            }
        }
        "prompts/list" => {
            let prompts = json!([
                {
                    "name": "wiki-curate",
                    "description": "Synthesize raw sources into a wiki page",
                    "arguments": [{ "name": "topic", "description": "Topic to curate", "required": true }]
                },
                {
                    "name": "rag-query",
                    "description": "Query the knowledge base for relevant context",
                    "arguments": [{ "name": "query", "description": "Search query", "required": true }]
                },
                {
                    "name": "brain-scan",
                    "description": "Scan for pending tasks, stale pages, and gaps",
                    "arguments": []
                }
            ]);
            JsonRpcResponse::success(id, json!({ "prompts": prompts }))
        }
        "prompts/get" => {
            let name = request.params.as_ref()
                .and_then(|p| p.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let args = request.params.as_ref()
                .and_then(|p| p.get("arguments"))
                .cloned()
                .unwrap_or(json!({}));
            let messages = match name {
                "wiki-curate" => {
                    let topic = args.get("topic").and_then(|v| v.as_str()).unwrap_or("unknown");
                    json!([{ "role": "user", "content": { "type": "text", "text": format!("Curate a wiki page for '{topic}' from 00-raw/ sources following 02-KB-main/schema.md conventions.") }}])
                }
                "rag-query" => {
                    let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
                    json!([{ "role": "user", "content": { "type": "text", "text": format!("Search the neuro-link knowledge base for: {query}") }}])
                }
                "brain-scan" => {
                    json!([{ "role": "user", "content": { "type": "text", "text": "Run a brain scan: check pending tasks, stale wiki pages, knowledge gaps, and deviation log failures." }}])
                }
                _ => {
                    return Json(serde_json::to_value(JsonRpcResponse::error(id, -32602, format!("Prompt not found: {name}"))).unwrap_or(json!(null)));
                }
            };
            JsonRpcResponse::success(id, json!({ "messages": messages }))
        }
        "notifications/initialized" => {
            return Json(json!(null));
        }
        _ => JsonRpcResponse::error(
            id,
            -32601,
            format!("Method not found: {}", request.method),
        ),
    };

    Json(serde_json::to_value(response).unwrap_or(json!(null)))
}
