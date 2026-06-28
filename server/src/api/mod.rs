pub mod auth;
pub mod error;
pub mod llm_proxy;
pub mod mcp;
pub mod rest;

use axum::{
    extract::State,
    middleware,
    response::Html,
    routing::{get, post},
    Json, Router,
};
use serde_json::json;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::tools::ToolRegistry;

const DASHBOARD_HTML: &str = include_str!("../dashboard/index.html");

pub fn build_router(registry: Arc<ToolRegistry>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let public_routes = Router::new()
        .route("/health", get(health))
        .route("/", get(dashboard))
        .route("/dashboard", get(dashboard));

    let protected_routes = Router::new()
        // MCP Streamable HTTP transport: GET for SSE server→client stream, POST for client→server JSON-RPC
        .route("/mcp", get(mcp::handle_mcp_sse).post(mcp::handle_mcp))
        // REST API v1
        .nest("/api/v1", rest::routes())
        // LLM API passthrough proxy (captures every LLM request/response)
        .nest("/llm/v1", llm_proxy::routes())
        .route_layer(middleware::from_fn(auth::bearer_auth))
        .with_state(registry.clone());

    Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .layer(cors)
        .layer(TraceLayer::new_for_http())
}

async fn health() -> &'static str {
    r#"{"status":"ok"}"#
}

async fn dashboard() -> Html<&'static str> {
    Html(DASHBOARD_HTML)
}

pub async fn services_status(
    State(registry): State<Arc<ToolRegistry>>,
) -> Json<serde_json::Value> {
    let statuses = crate::supervisor::all_statuses(registry.root()).await;
    Json(json!({ "services": statuses }))
}
