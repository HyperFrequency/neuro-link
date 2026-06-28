// LLM API passthrough proxy with logging, RAG injection, and cost tracking.
//
// Exposes:
//   POST /llm/v1/chat/completions  (OpenAI-compatible, routes via OpenRouter/OpenAI/etc.)
//   POST /llm/v1/messages           (Anthropic native format, used by Claude Code)
//   POST /llm/v1/embeddings         (routes to local embedding server or cloud)
//   GET  /llm/v1/models             (list of available models)
//
// Client usage:
//   ANTHROPIC_BASE_URL=http://localhost:8080/llm/v1 claude
//   OPENAI_API_BASE=http://localhost:8080/llm/v1    <other-cli>
//
// Logs every request+response to state/llm_logs/<token_hash>/<YYYY-MM-DD>.jsonl
//
// WAVE C / C3 guardrails:
//   - Per-token rate limit (rolling 60s window, default 60 rpm)
//   - Per-token daily USD budget cap (UTC day, default $50)
//   - Model allowlist (glob patterns, default: claude-*, anthropic/*, openrouter/*)
// Config from config/neuro-link.md `llm_proxy:` block; env overrides via
//   NLR_LLM_RPM, NLR_LLM_DAILY_USD, NLR_LLM_ALLOWED_MODELS.

use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;
use tokio::io::AsyncWriteExt;

use crate::tools::ToolRegistry;

pub fn routes() -> Router<Arc<ToolRegistry>> {
    Router::new()
        .route("/chat/completions", post(chat_completions))
        .route("/messages", post(messages))
        .route("/embeddings", post(embeddings))
        .route("/models", get(list_models))
}

// ── Log entry ──

#[derive(Debug, Serialize, Deserialize)]
struct LlmLogEntry {
    timestamp: String,
    client_token_hash: String,
    provider: String,
    model: String,
    endpoint: String,
    request: Value,
    response: Value,
    rag_context_injected: bool,
    rag_pages: Vec<String>,
    tokens: TokenCounts,
    latency_ms: u64,
    cost_usd: f64,
    session_id: String,
    error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct TokenCounts {
    input: u64,
    output: u64,
}

// ── Quota ledger: state/llm_quota.jsonl ──

#[derive(Debug, Serialize, Deserialize)]
struct QuotaEvent {
    timestamp: String,
    client_token_hash: String,
    utc_date: String,
    event: String, // request | rate_limit_block | budget_block | model_block
    model: String,
    cost_usd: f64,
    daily_spent_usd: f64,
    rpm_observed: u32,
}

async fn append_quota(root: &std::path::Path, event: &QuotaEvent) -> anyhow::Result<()> {
    let dir = root.join("state");
    tokio::fs::create_dir_all(&dir).await?;
    let path = dir.join("llm_quota.jsonl");
    let line = serde_json::to_string(event)? + "\n";
    let mut f = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .await?;
    f.write_all(line.as_bytes()).await?;
    Ok(())
}

// ── Guardrail config ──

#[derive(Debug, Clone)]
struct GuardrailConfig {
    rate_limit_rpm: u32,
    daily_budget_usd: f64,
    allowed_models: Vec<String>,
}

impl GuardrailConfig {
    fn load(root: &std::path::Path) -> Self {
        let rpm: u32 = std::env::var("NLR_LLM_RPM")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| parse_config_int(root, "rate_limit_rpm").unwrap_or(60));

        let daily: f64 = std::env::var("NLR_LLM_DAILY_USD")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| parse_config_float(root, "daily_budget_usd").unwrap_or(50.0));

        let allowed = std::env::var("NLR_LLM_ALLOWED_MODELS")
            .ok()
            .map(|s| {
                s.split(',')
                    .map(|p| p.trim().to_string())
                    .filter(|p| !p.is_empty())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| parse_config_models(root));

        let allowed = if allowed.is_empty() {
            vec![
                "claude-*".to_string(),
                "anthropic/*".to_string(),
                "openrouter/*".to_string(),
            ]
        } else {
            allowed
        };

        GuardrailConfig {
            rate_limit_rpm: rpm,
            daily_budget_usd: daily,
            allowed_models: allowed,
        }
    }
}

fn parse_config_int(root: &std::path::Path, key: &str) -> Option<u32> {
    let content = std::fs::read_to_string(root.join("config/neuro-link.md")).ok()?;
    for line in content.lines() {
        if let Some(rest) = line.trim().strip_prefix(&format!("{key}:")) {
            return rest.trim().parse().ok();
        }
    }
    None
}

fn parse_config_float(root: &std::path::Path, key: &str) -> Option<f64> {
    let content = std::fs::read_to_string(root.join("config/neuro-link.md")).ok()?;
    for line in content.lines() {
        if let Some(rest) = line.trim().strip_prefix(&format!("{key}:")) {
            return rest.trim().parse().ok();
        }
    }
    None
}

fn parse_config_models(root: &std::path::Path) -> Vec<String> {
    let content = match std::fs::read_to_string(root.join("config/neuro-link.md")) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    let mut in_block = false;
    for line in content.lines() {
        let trimmed = line.trim_start();
        if !in_block {
            if trimmed.starts_with("allowed_models:") {
                in_block = true;
            }
            continue;
        }
        if let Some(val) = trimmed.strip_prefix("- ") {
            let v = val.trim().trim_matches('"').to_string();
            if !v.is_empty() {
                out.push(v);
            }
        } else if trimmed.is_empty() {
            continue;
        } else {
            break;
        }
    }
    out
}

fn model_matches(pattern: &str, model: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix('*') {
        model.starts_with(prefix)
    } else {
        pattern == model
    }
}

fn is_model_allowed(cfg: &GuardrailConfig, model: &str) -> bool {
    cfg.allowed_models.iter().any(|p| model_matches(p, model))
}

// ── Rate limiter: rolling 60s window per token-hash ──

#[derive(Default)]
struct RateWindow {
    recent: std::collections::HashMap<String, VecDeque<Instant>>,
}

impl RateWindow {
    fn record_and_check(&mut self, hash: &str, limit: u32) -> (bool, u32) {
        let now = Instant::now();
        let entry = self.recent.entry(hash.to_string()).or_default();
        while let Some(front) = entry.front() {
            if now.duration_since(*front).as_secs() >= 60 {
                entry.pop_front();
            } else {
                break;
            }
        }
        let observed = entry.len() as u32;
        if observed >= limit {
            return (false, observed);
        }
        entry.push_back(now);
        (true, observed + 1)
    }
}

// ── Daily-spend ledger (rebuilt lazily from state/llm_quota.jsonl) ──

#[derive(Default)]
struct SpendLedger {
    spend: std::collections::HashMap<(String, String), f64>,
    loaded: std::collections::HashSet<String>,
}

impl SpendLedger {
    fn ensure_loaded_for(&mut self, root: &std::path::Path, hash: &str, utc_date: &str) {
        let marker = format!("{hash}:{utc_date}");
        if self.loaded.contains(&marker) {
            return;
        }
        let path = root.join("state/llm_quota.jsonl");
        if let Ok(content) = std::fs::read_to_string(&path) {
            for line in content.lines() {
                let Ok(ev): Result<QuotaEvent, _> = serde_json::from_str(line) else {
                    continue;
                };
                if ev.client_token_hash != hash || ev.utc_date != utc_date {
                    continue;
                }
                if ev.event == "request" {
                    *self
                        .spend
                        .entry((hash.to_string(), utc_date.to_string()))
                        .or_default() += ev.cost_usd;
                }
            }
        }
        self.loaded.insert(marker);
    }

    fn spent(&self, hash: &str, utc_date: &str) -> f64 {
        *self
            .spend
            .get(&(hash.to_string(), utc_date.to_string()))
            .unwrap_or(&0.0)
    }

    fn add(&mut self, hash: &str, utc_date: &str, delta: f64) {
        *self
            .spend
            .entry((hash.to_string(), utc_date.to_string()))
            .or_default() += delta;
    }
}

static RATE_WINDOW: OnceLock<Mutex<RateWindow>> = OnceLock::new();
static SPEND_LEDGER: OnceLock<Mutex<SpendLedger>> = OnceLock::new();

fn rate_window() -> &'static Mutex<RateWindow> {
    RATE_WINDOW.get_or_init(|| Mutex::new(RateWindow::default()))
}
fn spend_ledger() -> &'static Mutex<SpendLedger> {
    SPEND_LEDGER.get_or_init(|| Mutex::new(SpendLedger::default()))
}

// ── Provider resolution ──

fn resolve_provider_key(provider: &str) -> Option<String> {
    let env_var = match provider {
        "openrouter" => "OPENROUTER_API_KEY",
        "anthropic" => "ANTHROPIC_API_KEY",
        "openai" => "OPENAI_API_KEY",
        _ => return None,
    };
    std::env::var(env_var).ok().filter(|s| !s.is_empty())
}

fn provider_base_url(provider: &str) -> &'static str {
    match provider {
        "openrouter" => "https://openrouter.ai/api/v1",
        "anthropic" => "https://api.anthropic.com/v1",
        "openai" => "https://api.openai.com/v1",
        _ => "https://openrouter.ai/api/v1",
    }
}

fn detect_provider_from_headers(headers: &HeaderMap) -> String {
    if let Some(p) = headers.get("x-nlr-provider").and_then(|v| v.to_str().ok()) {
        return p.to_string();
    }
    if headers.contains_key("anthropic-version") {
        return "anthropic".to_string();
    }
    "openrouter".to_string()
}

fn token_hash(headers: &HeaderMap) -> String {
    let auth = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .strip_prefix("Bearer ")
        .unwrap_or("");
    let hash = Sha256::digest(auth.as_bytes());
    hex::encode(&hash[..8])
}

// ── Logging ──

async fn append_log(root: &std::path::Path, entry: &LlmLogEntry) -> anyhow::Result<()> {
    let dir = root.join("state/llm_logs").join(&entry.client_token_hash);
    tokio::fs::create_dir_all(&dir).await?;
    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let path = dir.join(format!("{date}.jsonl"));
    let line = serde_json::to_string(entry)? + "\n";
    let mut f = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .await?;
    f.write_all(line.as_bytes()).await?;
    Ok(())
}

// ── RAG injection ──

async fn inject_rag_context(
    registry: &ToolRegistry,
    request: &mut Value,
    endpoint: &str,
) -> (bool, Vec<String>) {
    let query_text = match endpoint {
        "/chat/completions" => request
            .get("messages")
            .and_then(|m| m.as_array())
            .and_then(|arr| arr.iter().rev().find(|m| m.get("role").and_then(|r| r.as_str()) == Some("user")))
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string(),
        "/messages" => request
            .get("messages")
            .and_then(|m| m.as_array())
            .and_then(|arr| arr.iter().rev().find(|m| m.get("role").and_then(|r| r.as_str()) == Some("user")))
            .and_then(|m| m.get("content"))
            .map(|c| {
                if c.is_string() {
                    c.as_str().unwrap_or("").to_string()
                } else if let Some(arr) = c.as_array() {
                    arr.iter()
                        .filter_map(|item| item.get("text").and_then(|t| t.as_str()))
                        .collect::<Vec<_>>()
                        .join(" ")
                } else {
                    String::new()
                }
            })
            .unwrap_or_default(),
        _ => String::new(),
    };

    if query_text.is_empty() {
        return (false, vec![]);
    }

    let rag_result = registry.call("nlr_rag_query", &json!({ "query": query_text, "limit": 3 }));
    let rag_text = match rag_result {
        Ok(t) => t,
        Err(_) => return (false, vec![]),
    };

    let parsed: Vec<Value> = serde_json::from_str(&rag_text).unwrap_or_default();
    if parsed.is_empty() {
        return (false, vec![]);
    }

    let pages: Vec<String> = parsed
        .iter()
        .filter_map(|p| p.get("path").and_then(|v| v.as_str()).map(String::from))
        .collect();

    let context_block = format!(
        "\n\n[neuro-link RAG context — {} wiki pages]\n{}",
        pages.len(),
        parsed
            .iter()
            .map(|p| {
                let path = p.get("path").and_then(|v| v.as_str()).unwrap_or("");
                let preview = p.get("preview").and_then(|v| v.as_str()).unwrap_or("");
                format!("## {path}\n{preview}\n")
            })
            .collect::<Vec<_>>()
            .join("\n")
    );

    if endpoint == "/messages" {
        let existing = request.get("system").and_then(|s| s.as_str()).unwrap_or("").to_string();
        request["system"] = json!(format!("{existing}{context_block}"));
    } else if let Some(messages) = request.get_mut("messages").and_then(|m| m.as_array_mut()) {
        messages.insert(0, json!({"role": "system", "content": context_block}));
    }

    (true, pages)
}

// ── Cost estimation ──

fn estimate_cost(provider: &str, model: &str, input_tokens: u64, output_tokens: u64) -> f64 {
    let (in_rate, out_rate) = match (provider, model) {
        ("anthropic", m) if m.contains("opus") => (15.0, 75.0),
        ("anthropic", m) if m.contains("sonnet") => (3.0, 15.0),
        ("anthropic", m) if m.contains("haiku") => (0.25, 1.25),
        ("openai", m) if m.contains("gpt-4") => (2.5, 10.0),
        ("openai", m) if m.contains("gpt-3.5") => (0.5, 1.5),
        _ => (1.0, 3.0),
    };
    (input_tokens as f64 * in_rate + output_tokens as f64 * out_rate) / 1_000_000.0
}

fn utc_today() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

// ── Handlers ──

async fn chat_completions(
    State(registry): State<Arc<ToolRegistry>>,
    headers: HeaderMap,
    Json(mut request): Json<Value>,
) -> Response {
    passthrough(&registry, &headers, &mut request, "/chat/completions").await
}

async fn messages(
    State(registry): State<Arc<ToolRegistry>>,
    headers: HeaderMap,
    Json(mut request): Json<Value>,
) -> Response {
    passthrough(&registry, &headers, &mut request, "/messages").await
}

async fn embeddings(
    State(_registry): State<Arc<ToolRegistry>>,
    Json(request): Json<Value>,
) -> Response {
    let url = std::env::var("EMBEDDING_API_URL")
        .unwrap_or_else(|_| "http://localhost:8400/v1/embeddings".into());
    let client = reqwest::Client::new();
    match client.post(&url).json(&request).send().await {
        Ok(resp) => {
            let status = resp.status();
            let body = resp.bytes().await.unwrap_or_default();
            Response::builder()
                .status(status)
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap_or_else(|_| {
                    (StatusCode::INTERNAL_SERVER_ERROR, "build error").into_response()
                })
        }
        Err(e) => {
            (StatusCode::BAD_GATEWAY, format!("embedding server unreachable: {e}")).into_response()
        }
    }
}

async fn list_models(headers: HeaderMap) -> Response {
    let provider = detect_provider_from_headers(&headers);
    let api_key = match resolve_provider_key(&provider) {
        Some(k) => k,
        None => {
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("no API key for {provider}"))
                .into_response()
        }
    };
    let url = format!("{}/models", provider_base_url(&provider));
    let client = reqwest::Client::new();
    match client.get(&url).bearer_auth(&api_key).send().await {
        Ok(resp) => {
            let status = resp.status();
            let body = resp.bytes().await.unwrap_or_default();
            Response::builder()
                .status(status)
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap_or_else(|_| {
                    (StatusCode::INTERNAL_SERVER_ERROR, "build error").into_response()
                })
        }
        Err(e) => (StatusCode::BAD_GATEWAY, format!("upstream: {e}")).into_response(),
    }
}

// ── Core passthrough logic ──

async fn passthrough(
    registry: &Arc<ToolRegistry>,
    headers: &HeaderMap,
    request: &mut Value,
    endpoint: &str,
) -> Response {
    let started = Instant::now();
    let hash = token_hash(headers);
    let provider = detect_provider_from_headers(headers);
    let session_id = uuid::Uuid::new_v4().to_string();
    let auto_rag = headers
        .get("x-nlr-auto-rag")
        .and_then(|v| v.to_str().ok())
        == Some("true");

    // ── Guardrails (T4) ──
    let cfg = GuardrailConfig::load(registry.root());
    let model = request
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let today = utc_today();

    // 1. Model allowlist.
    if !is_model_allowed(&cfg, &model) {
        let daily = {
            let mut ledger = spend_ledger().lock().unwrap();
            ledger.ensure_loaded_for(registry.root(), &hash, &today);
            ledger.spent(&hash, &today)
        };
        let ev = QuotaEvent {
            timestamp: chrono::Utc::now().to_rfc3339(),
            client_token_hash: hash.clone(),
            utc_date: today.clone(),
            event: "model_block".into(),
            model: model.clone(),
            cost_usd: 0.0,
            daily_spent_usd: daily,
            rpm_observed: 0,
        };
        let _ = append_quota(registry.root(), &ev).await;
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "model_not_allowed",
                "message": format!("Model '{model}' is not in the allowlist. Configured patterns: {:?}", cfg.allowed_models),
                "allowed_models": cfg.allowed_models,
                "status": 400
            })),
        )
            .into_response();
    }

    // 2. Rate limit.
    let (rate_ok, rpm_observed) = {
        let mut w = rate_window().lock().unwrap();
        w.record_and_check(&hash, cfg.rate_limit_rpm)
    };
    if !rate_ok {
        let daily = {
            let mut ledger = spend_ledger().lock().unwrap();
            ledger.ensure_loaded_for(registry.root(), &hash, &today);
            ledger.spent(&hash, &today)
        };
        let ev = QuotaEvent {
            timestamp: chrono::Utc::now().to_rfc3339(),
            client_token_hash: hash.clone(),
            utc_date: today.clone(),
            event: "rate_limit_block".into(),
            model: model.clone(),
            cost_usd: 0.0,
            daily_spent_usd: daily,
            rpm_observed,
        };
        let _ = append_quota(registry.root(), &ev).await;
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [("retry-after", "60")],
            Json(json!({
                "error": "rate_limit_exceeded",
                "message": format!("{rpm_observed} req in last 60s (limit {})", cfg.rate_limit_rpm),
                "rpm_limit": cfg.rate_limit_rpm,
                "status": 429
            })),
        )
            .into_response();
    }

    // 3. Daily budget (pre-flight, based on already-spent).
    let spent_now = {
        let mut ledger = spend_ledger().lock().unwrap();
        ledger.ensure_loaded_for(registry.root(), &hash, &today);
        ledger.spent(&hash, &today)
    };
    if spent_now >= cfg.daily_budget_usd {
        let ev = QuotaEvent {
            timestamp: chrono::Utc::now().to_rfc3339(),
            client_token_hash: hash.clone(),
            utc_date: today.clone(),
            event: "budget_block".into(),
            model: model.clone(),
            cost_usd: 0.0,
            daily_spent_usd: spent_now,
            rpm_observed,
        };
        let _ = append_quota(registry.root(), &ev).await;
        return (
            StatusCode::PAYMENT_REQUIRED,
            Json(json!({
                "error": "daily_budget_exceeded",
                "message": format!("${:.2} spent today (cap ${:.2})", spent_now, cfg.daily_budget_usd),
                "daily_spent_usd": spent_now,
                "daily_budget_usd": cfg.daily_budget_usd,
                "status": 402
            })),
        )
            .into_response();
    }

    // Optional RAG injection
    let (rag_injected, rag_pages) = if auto_rag {
        inject_rag_context(registry, request, endpoint).await
    } else {
        (false, vec![])
    };

    // Provider auth
    let api_key = match resolve_provider_key(&provider) {
        Some(k) => k,
        None => {
            let err = format!(
                "No API key configured for provider '{provider}'. Set {}_API_KEY in secrets/.env",
                provider.to_uppercase()
            );
            return (StatusCode::INTERNAL_SERVER_ERROR, err).into_response();
        }
    };

    // Build upstream request
    let url = format!("{}{}", provider_base_url(&provider), endpoint);
    let client = reqwest::Client::new();
    let mut req = client.post(&url).json(&request);

    req = match provider.as_str() {
        "anthropic" => req.header("x-api-key", &api_key).header(
            "anthropic-version",
            headers
                .get("anthropic-version")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("2023-06-01"),
        ),
        _ => req.bearer_auth(&api_key),
    };

    let upstream = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            let latency = started.elapsed().as_millis() as u64;
            let entry = LlmLogEntry {
                timestamp: chrono::Utc::now().to_rfc3339(),
                client_token_hash: hash.clone(),
                provider: provider.clone(),
                model: model.clone(),
                endpoint: endpoint.to_string(),
                request: request.clone(),
                response: json!(null),
                rag_context_injected: rag_injected,
                rag_pages: rag_pages.clone(),
                tokens: TokenCounts::default(),
                latency_ms: latency,
                cost_usd: 0.0,
                session_id: session_id.clone(),
                error: Some(format!("upstream: {e}")),
            };
            let _ = append_log(registry.root(), &entry).await;
            return (StatusCode::BAD_GATEWAY, format!("upstream error: {e}")).into_response();
        }
    };

    let status = upstream.status();
    let body_bytes = upstream.bytes().await.unwrap_or_default();

    let response_json: Value = serde_json::from_slice(&body_bytes).unwrap_or(json!(null));
    let tokens = extract_tokens(&response_json, &provider);
    let cost = estimate_cost(&provider, &model, tokens.input, tokens.output);
    let latency = started.elapsed().as_millis() as u64;

    // Post-flight quota ledger update.
    {
        let mut ledger = spend_ledger().lock().unwrap();
        ledger.add(&hash, &today, cost);
    }
    let daily_spent = {
        let ledger = spend_ledger().lock().unwrap();
        ledger.spent(&hash, &today)
    };
    let quota_event = QuotaEvent {
        timestamp: chrono::Utc::now().to_rfc3339(),
        client_token_hash: hash.clone(),
        utc_date: today.clone(),
        event: "request".into(),
        model: model.clone(),
        cost_usd: cost,
        daily_spent_usd: daily_spent,
        rpm_observed,
    };
    let _ = append_quota(registry.root(), &quota_event).await;

    let entry = LlmLogEntry {
        timestamp: chrono::Utc::now().to_rfc3339(),
        client_token_hash: hash,
        provider,
        model,
        endpoint: endpoint.to_string(),
        request: request.clone(),
        response: response_json,
        rag_context_injected: rag_injected,
        rag_pages,
        tokens,
        latency_ms: latency,
        cost_usd: cost,
        session_id,
        error: if status.is_success() { None } else { Some(format!("HTTP {status}")) },
    };
    let _ = append_log(registry.root(), &entry).await;

    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(body_bytes))
        .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "build error").into_response())
}

fn extract_tokens(response: &Value, provider: &str) -> TokenCounts {
    match provider {
        "anthropic" => TokenCounts {
            input: response["usage"]["input_tokens"].as_u64().unwrap_or(0),
            output: response["usage"]["output_tokens"].as_u64().unwrap_or(0),
        },
        _ => TokenCounts {
            input: response["usage"]["prompt_tokens"].as_u64().unwrap_or(0),
            output: response["usage"]["completion_tokens"].as_u64().unwrap_or(0),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_matches_prefix_glob() {
        assert!(model_matches("claude-*", "claude-sonnet-4-6"));
        assert!(model_matches("claude-*", "claude-opus-4-6"));
        assert!(!model_matches("claude-*", "gpt-4o"));
        assert!(model_matches("openrouter/*", "openrouter/anthropic/claude-3.5-sonnet"));
        assert!(!model_matches("openrouter/*", "anthropic/claude-3.5-sonnet"));
    }

    #[test]
    fn model_matches_exact() {
        assert!(model_matches("claude-sonnet-4-6", "claude-sonnet-4-6"));
        assert!(!model_matches("claude-sonnet-4-6", "claude-sonnet-4-7"));
    }

    #[test]
    fn allowlist_default_patterns_accept_common_models() {
        let cfg = GuardrailConfig {
            rate_limit_rpm: 60,
            daily_budget_usd: 50.0,
            allowed_models: vec!["claude-*".into(), "anthropic/*".into(), "openrouter/*".into()],
        };
        assert!(is_model_allowed(&cfg, "claude-sonnet-4-6"));
        assert!(is_model_allowed(&cfg, "anthropic/claude-3.5-sonnet"));
        assert!(is_model_allowed(&cfg, "openrouter/meta-llama/llama-3"));
        assert!(!is_model_allowed(&cfg, "gpt-4o"));
        assert!(!is_model_allowed(&cfg, ""));
    }

    #[test]
    fn rate_limiter_caps_at_limit() {
        let mut w = RateWindow::default();
        let hash = "testhash";
        let limit = 3;
        let r1 = w.record_and_check(hash, limit);
        let r2 = w.record_and_check(hash, limit);
        let r3 = w.record_and_check(hash, limit);
        let r4 = w.record_and_check(hash, limit);
        assert!(r1.0);
        assert!(r2.0);
        assert!(r3.0);
        assert!(!r4.0, "4th request blocked (over RPM cap)");
        assert_eq!(r4.1, 3);
    }

    #[test]
    fn rate_limiter_independent_per_token_hash() {
        let mut w = RateWindow::default();
        let limit = 2;
        assert!(w.record_and_check("a", limit).0);
        assert!(w.record_and_check("a", limit).0);
        assert!(!w.record_and_check("a", limit).0);
        assert!(w.record_and_check("b", limit).0);
        assert!(w.record_and_check("b", limit).0);
        assert!(!w.record_and_check("b", limit).0);
    }

    /// Synthetic load smoke: 100 sequential requests against a 60 rpm cap
    /// → exactly 60 should pass and 40 should be 429-blocked.
    #[test]
    fn synthetic_load_blocks_after_limit() {
        let mut w = RateWindow::default();
        let limit = 60;
        let hash = "load_test";
        let mut allowed = 0;
        let mut blocked = 0;
        for _ in 0..100 {
            if w.record_and_check(hash, limit).0 {
                allowed += 1;
            } else {
                blocked += 1;
            }
        }
        assert_eq!(allowed, 60);
        assert_eq!(blocked, 40);
    }

    #[test]
    fn spend_ledger_tracks_per_token_per_day() {
        let mut l = SpendLedger::default();
        l.add("tok_a", "2026-04-16", 10.5);
        l.add("tok_a", "2026-04-16", 4.25);
        l.add("tok_a", "2026-04-17", 1.0);
        l.add("tok_b", "2026-04-16", 3.0);

        assert!((l.spent("tok_a", "2026-04-16") - 14.75).abs() < 1e-9);
        assert!((l.spent("tok_a", "2026-04-17") - 1.0).abs() < 1e-9);
        assert!((l.spent("tok_b", "2026-04-16") - 3.0).abs() < 1e-9);
        assert_eq!(l.spent("tok_c", "2026-04-16"), 0.0);
    }

    #[test]
    fn budget_cap_triggers_at_threshold() {
        let mut l = SpendLedger::default();
        let cap = 50.0;
        l.add("tok", "2026-04-16", 49.99);
        assert!(l.spent("tok", "2026-04-16") < cap);
        l.add("tok", "2026-04-16", 0.02);
        assert!(l.spent("tok", "2026-04-16") >= cap);
    }
}
