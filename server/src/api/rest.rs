use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, patch, post, put},
    Json, Router,
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Arc;

use super::error::ApiError;
use crate::tools::ToolRegistry;

pub fn routes() -> Router<Arc<ToolRegistry>> {
    Router::new()
        // Wiki
        .route("/wiki/pages", get(wiki_list).post(wiki_create))
        .route("/wiki/pages/{*path}", get(wiki_read).put(wiki_update).patch(wiki_append))
        .route("/wiki/search", get(wiki_search))
        // RAG
        .route("/rag/query", get(rag_query))
        .route("/rag/index", post(rag_rebuild))
        // Ingest
        .route("/ingest", post(ingest))
        .route("/ingest/{slug}/classify", post(ingest_classify))
        .route("/ingest/dedup", post(ingest_dedup))
        // Tasks
        .route("/tasks", get(task_list).post(task_create))
        .route("/tasks/{filename}", patch(task_update))
        // Ontology
        .route("/ontology", post(ontology_generate))
        .route("/ontology/gaps", get(ontology_gaps))
        .route("/ontology/{name}", get(ontology_query))
        // Scan
        .route("/scan/health", get(scan_health))
        .route("/scan/staleness", get(scan_staleness))
        // Harness
        .route("/harness", get(harness_list))
        .route("/harness/dispatch", post(harness_dispatch))
        // State
        .route("/state/heartbeat", get(state_heartbeat_read).post(state_heartbeat_update))
        .route("/state/log", post(state_log))
        // Config
        .route("/config/{name}", get(config_read))
        // Access control
        .route("/access/paths", get(access_paths_list))
        // Hooks (CLI client event ingestion)
        .route("/hooks/event", post(hooks_event))
        // Service supervisor status
        .route("/services", get(super::services_status))
}

// Helper to call a tool and parse JSON result
fn call_tool(registry: &ToolRegistry, name: &str, args: &Value) -> Result<Value, ApiError> {
    let result = registry.call(name, args).map_err(ApiError::from)?;
    serde_json::from_str(&result).or_else(|_| Ok(json!(result)))
}

// ── Wiki ──

async fn wiki_list(State(reg): State<Arc<ToolRegistry>>) -> Result<Json<Value>, ApiError> {
    let data = call_tool(&reg, "nlr_wiki_list", &json!({}))?;
    Ok(Json(json!({ "pages": data, "count": data.as_array().map(|a| a.len()).unwrap_or(0) })))
}

#[derive(Deserialize)]
struct WikiCreateBody {
    path: String,
    title: String,
    content: String,
    domain: Option<String>,
    confidence: Option<String>,
}

async fn wiki_create(
    State(reg): State<Arc<ToolRegistry>>,
    Json(body): Json<WikiCreateBody>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let args = json!({
        "path": body.path,
        "title": body.title,
        "content": body.content,
        "domain": body.domain.unwrap_or_else(|| "general".into()),
        "confidence": body.confidence.unwrap_or_else(|| "medium".into()),
    });
    let data = call_tool(&reg, "nlr_wiki_create", &args)?;
    Ok((StatusCode::CREATED, Json(data)))
}

async fn wiki_read(
    State(reg): State<Arc<ToolRegistry>>,
    Path(path): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let data = call_tool(&reg, "nlr_wiki_read", &json!({ "path": path }))?;
    Ok(Json(data))
}

#[derive(Deserialize)]
struct WikiUpdateBody {
    content: String,
}

async fn wiki_update(
    State(reg): State<Arc<ToolRegistry>>,
    Path(path): Path<String>,
    Json(body): Json<WikiUpdateBody>,
) -> Result<Json<Value>, ApiError> {
    let data = call_tool(&reg, "nlr_wiki_update", &json!({
        "path": path,
        "content": body.content,
        "mode": "replace",
    }))?;
    Ok(Json(data))
}

async fn wiki_append(
    State(reg): State<Arc<ToolRegistry>>,
    Path(path): Path<String>,
    Json(body): Json<WikiUpdateBody>,
) -> Result<Json<Value>, ApiError> {
    let data = call_tool(&reg, "nlr_wiki_update", &json!({
        "path": path,
        "content": body.content,
        "mode": "append",
    }))?;
    Ok(Json(data))
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
}

async fn wiki_search(
    State(reg): State<Arc<ToolRegistry>>,
    Query(params): Query<SearchQuery>,
) -> Result<Json<Value>, ApiError> {
    let data = call_tool(&reg, "nlr_wiki_search", &json!({ "query": params.q }))?;
    Ok(Json(json!({ "results": data, "query": params.q, "count": data.as_array().map(|a| a.len()).unwrap_or(0) })))
}

// ── RAG ──

#[derive(Deserialize)]
struct RagQuery {
    q: String,
    limit: Option<usize>,
}

async fn rag_query(
    State(reg): State<Arc<ToolRegistry>>,
    Query(params): Query<RagQuery>,
) -> Result<Json<Value>, ApiError> {
    let data = call_tool(&reg, "nlr_rag_query", &json!({
        "query": params.q,
        "limit": params.limit.unwrap_or(5),
    }))?;
    Ok(Json(json!({ "results": data, "query": params.q })))
}

async fn rag_rebuild(State(reg): State<Arc<ToolRegistry>>) -> Result<Json<Value>, ApiError> {
    let data = call_tool(&reg, "nlr_rag_rebuild_index", &json!({}))?;
    Ok(Json(data))
}

// ── Ingest ──

#[derive(Deserialize)]
struct IngestBody {
    slug: String,
    content: String,
    url: Option<String>,
    source_type: Option<String>,
}

async fn ingest(
    State(reg): State<Arc<ToolRegistry>>,
    Json(body): Json<IngestBody>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let args = json!({
        "slug": body.slug,
        "content": body.content,
        "url": body.url.unwrap_or_default(),
        "source_type": body.source_type.unwrap_or_else(|| "web".into()),
    });
    let data = call_tool(&reg, "nlr_ingest", &args)?;
    let status = if data.get("duplicate").and_then(|v| v.as_bool()).unwrap_or(false) {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((status, Json(data)))
}

async fn ingest_classify(
    State(reg): State<Arc<ToolRegistry>>,
    Path(slug): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let domain = body.get("domain").and_then(|v| v.as_str()).unwrap_or("docs");
    let data = call_tool(&reg, "nlr_ingest_classify", &json!({
        "slug": slug,
        "domain": domain,
    }))?;
    Ok(Json(data))
}

async fn ingest_dedup(
    State(reg): State<Arc<ToolRegistry>>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let data = call_tool(&reg, "nlr_ingest_dedup", &body)?;
    Ok(Json(data))
}

// ── Tasks ──

#[derive(Deserialize)]
struct TaskListQuery {
    status: Option<String>,
}

async fn task_list(
    State(reg): State<Arc<ToolRegistry>>,
    Query(params): Query<TaskListQuery>,
) -> Result<Json<Value>, ApiError> {
    let filter = params.status.unwrap_or_else(|| "all".into());
    let data = call_tool(&reg, "nlr_task_list", &json!({ "status_filter": filter }))?;
    Ok(Json(json!({ "tasks": data, "filter": filter, "count": data.as_array().map(|a| a.len()).unwrap_or(0) })))
}

#[derive(Deserialize)]
struct TaskCreateBody {
    title: String,
    #[serde(rename = "type")]
    task_type: String,
    priority: Option<u64>,
    body: Option<String>,
}

async fn task_create(
    State(reg): State<Arc<ToolRegistry>>,
    Json(body): Json<TaskCreateBody>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let args = json!({
        "title": body.title,
        "type": body.task_type,
        "priority": body.priority.unwrap_or(3),
        "body": body.body.unwrap_or_default(),
    });
    let data = call_tool(&reg, "nlr_task_create", &args)?;
    Ok((StatusCode::CREATED, Json(data)))
}

#[derive(Deserialize)]
struct TaskUpdateBody {
    status: String,
}

async fn task_update(
    State(reg): State<Arc<ToolRegistry>>,
    Path(filename): Path<String>,
    Json(body): Json<TaskUpdateBody>,
) -> Result<Json<Value>, ApiError> {
    let data = call_tool(&reg, "nlr_task_update", &json!({
        "filename": filename,
        "status": body.status,
    }))?;
    Ok(Json(data))
}

// ── Ontology ──

#[derive(Deserialize)]
struct OntologyCreateBody {
    name: String,
    text: String,
    #[serde(rename = "type")]
    ontology_type: Option<String>,
}

async fn ontology_generate(
    State(reg): State<Arc<ToolRegistry>>,
    Json(body): Json<OntologyCreateBody>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let args = json!({
        "name": body.name,
        "text": body.text,
        "type": body.ontology_type.unwrap_or_else(|| "domain".into()),
    });
    let data = call_tool(&reg, "nlr_ontology_generate", &args)?;
    Ok((StatusCode::CREATED, Json(data)))
}

#[derive(Deserialize)]
struct OntologyQueryParams {
    tier: Option<String>,
}

async fn ontology_query(
    State(reg): State<Arc<ToolRegistry>>,
    Path(name): Path<String>,
    Query(params): Query<OntologyQueryParams>,
) -> Result<Json<Value>, ApiError> {
    let data = call_tool(&reg, "nlr_ontology_query", &json!({
        "name": name,
        "tier": params.tier.unwrap_or_else(|| "summary".into()),
    }))?;
    Ok(Json(data))
}

async fn ontology_gaps(State(reg): State<Arc<ToolRegistry>>) -> Result<Json<Value>, ApiError> {
    let data = call_tool(&reg, "nlr_ontology_gaps", &json!({}))?;
    Ok(Json(data))
}

// ── Scan ──

async fn scan_health(State(reg): State<Arc<ToolRegistry>>) -> Result<Json<Value>, ApiError> {
    let data = call_tool(&reg, "nlr_scan_health", &json!({}))?;
    Ok(Json(data))
}

#[derive(Deserialize)]
struct StalenessQuery {
    days: Option<u64>,
}

async fn scan_staleness(
    State(reg): State<Arc<ToolRegistry>>,
    Query(params): Query<StalenessQuery>,
) -> Result<Json<Value>, ApiError> {
    let data = call_tool(&reg, "nlr_scan_staleness", &json!({
        "threshold_days": params.days.unwrap_or(30),
    }))?;
    Ok(Json(json!({ "stale_pages": data, "threshold_days": params.days.unwrap_or(30) })))
}

// ── Harness ──

async fn harness_list(State(reg): State<Arc<ToolRegistry>>) -> Result<Json<Value>, ApiError> {
    let data = call_tool(&reg, "nlr_harness_list", &json!({}))?;
    Ok(Json(json!({ "harnesses": data })))
}

#[derive(Deserialize)]
struct HarnessDispatchBody {
    to: String,
    task: String,
    priority: Option<u64>,
}

async fn harness_dispatch(
    State(reg): State<Arc<ToolRegistry>>,
    Json(body): Json<HarnessDispatchBody>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let args = json!({
        "target_harness": body.to,
        "task_description": body.task,
        "priority": body.priority.unwrap_or(3),
    });
    let data = call_tool(&reg, "nlr_harness_dispatch", &args)?;
    Ok((StatusCode::ACCEPTED, Json(data)))
}

// ── State ──

async fn state_heartbeat_read(State(reg): State<Arc<ToolRegistry>>) -> Result<Json<Value>, ApiError> {
    let data = call_tool(&reg, "nlr_state_heartbeat", &json!({ "action": "read" }))?;
    Ok(Json(data))
}

async fn state_heartbeat_update(State(reg): State<Arc<ToolRegistry>>) -> Result<Json<Value>, ApiError> {
    let data = call_tool(&reg, "nlr_state_heartbeat", &json!({ "action": "update" }))?;
    Ok(Json(data))
}

#[derive(Deserialize)]
struct StateLogBody {
    tool: String,
    exit_code: Option<i32>,
}

async fn state_log(
    State(reg): State<Arc<ToolRegistry>>,
    Json(body): Json<StateLogBody>,
) -> Result<Json<Value>, ApiError> {
    let data = call_tool(&reg, "nlr_state_log", &json!({
        "tool": body.tool,
        "exit_code": body.exit_code,
    }))?;
    Ok(Json(data))
}

// ── Config ──

async fn config_read(
    State(reg): State<Arc<ToolRegistry>>,
    Path(name): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let data = call_tool(&reg, "nlr_config_read", &json!({ "name": name }))?;
    Ok(Json(json!({ "name": name, "frontmatter": data })))
}

// ── Hooks (CLI client event ingestion) ──

#[derive(Deserialize)]
struct HookEventBody {
    event_type: String,
    client: String,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    data: Value,
}

async fn hooks_event(
    State(reg): State<Arc<ToolRegistry>>,
    Json(body): Json<HookEventBody>,
) -> Result<Json<Value>, ApiError> {
    // Validate event_type
    const ALLOWED: &[&str] = &[
        "pre_tool", "post_tool", "llm_response",
        "user_prompt", "session_start", "session_end",
    ];
    if !ALLOWED.contains(&body.event_type.as_str()) {
        return Err(ApiError::bad_request(format!(
            "Invalid event_type '{}'. Allowed: {}",
            body.event_type,
            ALLOWED.join(", ")
        )));
    }

    let entry = json!({
        "timestamp": Utc::now().to_rfc3339(),
        "event_type": body.event_type,
        "client": body.client,
        "session_id": body.session_id,
        "data": body.data,
    });

    // Append to state/hooks_log.jsonl
    let log_dir = reg.root().join("state");
    std::fs::create_dir_all(&log_dir)
        .map_err(|e| ApiError::internal(format!("create state dir: {e}")))?;
    let log_path = log_dir.join("hooks_log.jsonl");
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| ApiError::internal(format!("open hooks log: {e}")))?;
    writeln!(file, "{}", serde_json::to_string(&entry).unwrap_or_default())
        .map_err(|e| ApiError::internal(format!("write hooks log: {e}")))?;

    // For user_prompt events, try to produce RAG additionalContext.
    let mut additional_context: Option<String> = None;
    if body.event_type == "user_prompt" {
        if let Some(prompt) = body.data.get("prompt").and_then(|v| v.as_str()) {
            if !prompt.trim().is_empty() {
                if let Ok(raw) = reg.call(
                    "nlr_rag_query",
                    &json!({ "query": prompt, "limit": 3 }),
                ) {
                    // raw is a JSON-encoded list of {path, score, preview, source}
                    if let Ok(results) = serde_json::from_str::<Value>(&raw) {
                        if let Some(arr) = results.as_array() {
                            if !arr.is_empty() {
                                let mut parts: Vec<String> = Vec::new();
                                for r in arr.iter().take(3) {
                                    let path = r.get("path").and_then(|v| v.as_str()).unwrap_or("");
                                    let preview = r.get("preview").and_then(|v| v.as_str()).unwrap_or("");
                                    let preview = if preview.len() > 400 {
                                        &preview[..400]
                                    } else {
                                        preview
                                    };
                                    parts.push(format!("[{}] {}", path, preview));
                                }
                                additional_context = Some(format!(
                                    "NEURO-LINK-RECURSIVE AUTO-RAG:\n{}",
                                    parts.join("\n---\n")
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    let mut out = json!({ "ok": true });
    if let Some(ctx) = additional_context {
        out["additionalContext"] = json!(ctx);
    }
    Ok(Json(out))
}

// ── Access Control ──

async fn access_paths_list(
    State(reg): State<Arc<ToolRegistry>>,
) -> Result<Json<Value>, ApiError> {
    let allowed = crate::config::allowed_paths(reg.root());
    // Also list all available directories at root
    let mut available = Vec::new();
    if let Ok(entries) = std::fs::read_dir(reg.root()) {
        for entry in entries.filter_map(|e| e.ok()) {
            if entry.path().is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !name.starts_with('.') && name != "server" && name != "obsidian-plugin"
                    && name != "npm" && name != "skills" && name != "hooks" && name != "scripts"
                    && name != "node_modules" && name != "target"
                {
                    let enabled = allowed.iter().any(|a| a == &name);
                    available.push(json!({ "name": name, "enabled": enabled }));
                }
            }
        }
    }
    available.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    Ok(Json(json!({ "allowed": allowed, "available": available })))
}
