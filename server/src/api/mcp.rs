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
            // Gate-46 LLL1: vault dirs (vaults/ + 02-KB-main/) get
            // cross-vault dedup + non-empty filter — same logic the
            // stdio transport in main.rs applies (gate-45 KKK1).
            // Non-vault allowed paths (00-raw, 01-sorted, etc.) still
            // get listed broadly as before.
            let allowed = crate::config::allowed_paths(root);
            let skip = ["schema.md", "index.md", "log.md"];
            let vault_set: std::collections::HashSet<&str> =
                crate::embed::DEFAULT_VAULT_DIRS.iter().copied().collect();
            let mut resources = Vec::new();
            let mut vault_seen: std::collections::HashSet<String> =
                std::collections::HashSet::new();
            // Iterate allowed in declared order; vault entries are
            // visited in the canonical vault precedence (vaults first
            // when present in allowed_paths).
            let mut allowed_sorted = allowed.clone();
            allowed_sorted.sort_by_key(|d| {
                if vault_set.contains(d.as_str()) {
                    crate::embed::DEFAULT_VAULT_DIRS
                        .iter()
                        .position(|v| *v == d.as_str())
                        .unwrap_or(usize::MAX)
                } else {
                    usize::MAX
                }
            });
            for dir_name in &allowed_sorted {
                let dir = root.join(dir_name);
                if !dir.is_dir() { continue; }
                let is_vault = vault_set.contains(dir_name.as_str());
                for entry in WalkDir::new(&dir).into_iter().filter_map(|e| e.ok()) {
                    let path = entry.path();
                    if path.is_file()
                        && path.extension().is_some_and(|e| e == "md")
                        && !skip.iter().any(|s| path.file_name().is_some_and(|f| f == *s))
                    {
                        if is_vault {
                            let vault_rel = path
                                .strip_prefix(&dir)
                                .unwrap_or(path)
                                .display()
                                .to_string();
                            if vault_seen.contains(&vault_rel) { continue; }
                            // Skip blank vault files so legacy fallback can win
                            let is_non_empty = std::fs::read_to_string(path)
                                .map(|c| !c.trim().is_empty())
                                .unwrap_or(false);
                            if !is_non_empty { continue; }
                            vault_seen.insert(vault_rel.clone());
                            // Gate-47 MMM2: vault entries use the same
                            // nlr://wiki/{vault_rel} URI scheme as the
                            // stdio transport so clients can round-trip
                            // resource identifiers across transports.
                            resources.push(json!({
                                "uri": format!("nlr://wiki/{vault_rel}"),
                                "name": vault_rel,
                                "mimeType": "text/markdown"
                            }));
                            continue;
                        }
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
            // Gate-47 MMM2: accept BOTH `nlr://wiki/{vault_rel}` (vault
            // entry — same scheme stdio uses + new HTTP-vault-list
            // emits) AND `nlr://{rel}` (legacy non-vault path). Vault
            // form keeps the relative path under any vault root; legacy
            // form is the absolute-from-repo-root path.
            let (is_wiki_uri, rel_path) = if let Some(rest) = uri.strip_prefix("nlr://wiki/") {
                (true, rest)
            } else {
                (false, uri.strip_prefix("nlr://").unwrap_or(uri))
            };
            // Block traversal: reject .., absolute paths, and null bytes
            if rel_path.contains("..") || rel_path.starts_with('/') || rel_path.contains('\0') {
                return Json(serde_json::to_value(JsonRpcResponse::error(
                    id, -32602, "Invalid path: traversal not allowed".into(),
                )).unwrap_or(json!(null)));
            }
            // Allowlist check uses the underlying vault path for the
            // wiki form so a customized allowed_paths still gates.
            let vault_dirs = crate::embed::DEFAULT_VAULT_DIRS;
            let probe_path: String = if is_wiki_uri {
                // For allowlist purposes, treat as the canonical vaults/
                // path; if that's not allowed, the actual fallback loop
                // below will further restrict to allowed roots only.
                format!("{}/{rel_path}", vault_dirs[0])
            } else {
                rel_path.to_string()
            };
            if !crate::config::is_path_allowed(root, &probe_path) {
                return Json(serde_json::to_value(JsonRpcResponse::error(
                    id, -32602, "Access denied: path not in allowed_paths".into(),
                )).unwrap_or(json!(null)));
            }
            // Gate-46 LLL1 + Gate-47 MMM1: vault-aware fallback that
            // RESPECTS allowed_paths — if a vault root is excluded
            // from the user's customized allowlist, it cannot be used
            // as a fallback target (no cross-vault data leak).
            let allowed = crate::config::allowed_paths(root);
            let allowed_set: std::collections::HashSet<&str> =
                allowed.iter().map(|s| s.as_str()).collect();
            let root_canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
            // Determine vault_rel: extract from URI if wiki form, else
            // from rel_path's first segment if it matches a vault dir.
            let vault_rel: Option<&str> = if is_wiki_uri {
                Some(rel_path)
            } else {
                vault_dirs.iter().find_map(|v| {
                    let p = format!("{}/", v);
                    rel_path.strip_prefix(&p)
                })
            };
            let candidates: Vec<std::path::PathBuf> = if let Some(vrel) = vault_rel {
                vault_dirs
                    .iter()
                    .filter(|v| allowed_set.contains(*v))
                    .map(|v| root.join(v).join(vrel))
                    .collect()
            } else {
                vec![root.join(rel_path)]
            };
            let mut chosen: Option<(std::path::PathBuf, String)> = None;
            for candidate in candidates {
                let canonical = match candidate.canonicalize() {
                    Ok(p) => p,
                    Err(_) => continue,
                };
                if !canonical.starts_with(&root_canonical) {
                    continue;
                }
                match std::fs::read_to_string(&canonical) {
                    Ok(content) if !content.trim().is_empty() => {
                        chosen = Some((canonical, content));
                        break;
                    }
                    _ => continue,
                }
            }
            match chosen {
                Some((_canonical, content)) => JsonRpcResponse::success(id, json!({
                    "contents": [{ "uri": uri, "mimeType": "text/markdown", "text": content }]
                })),
                None => JsonRpcResponse::error(id, -32602, format!("Resource not found or empty in any vault root: {rel_path}")),
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
