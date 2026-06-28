mod api;
mod bm25;
mod cli;
mod config;
mod crawl;
mod curate_backlog;
mod embed;
mod grade;
mod graph;
mod heartbeat;
mod init;
mod protocol;
mod resort;
mod security;
mod sessions;
mod state;
mod supervisor;
mod tools;
mod workflow;
mod worker;
mod watcher_inbox;

use anyhow::Result;
use clap::Parser;
use colored::Colorize;
use serde_json::Value;
use std::io::{self, BufRead, Write};
use tracing::{error, info};

use cli::{Cli, Commands, GraphAction, SessionsAction, TaskAction, WorkflowAction};
use protocol::{JsonRpcRequest, JsonRpcResponse};
use tools::ToolRegistry;

fn spawn_inbox_watcher(root: &std::path::Path) {
    let root = root.to_path_buf();
    tokio::spawn(async move {
        if let Err(e) = watcher_inbox::run(root).await {
            eprintln!("[watcher] inbox watcher stopped: {e}");
        }
    });
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        None | Some(Commands::Mcp) => run_mcp_server(),
        Some(cmd) => run_cli(cmd).await,
    }
}

// ── MCP server (existing behavior, unchanged) ──

fn run_mcp_server() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("neuro_link_mcp=info")
        .with_writer(io::stderr)
        .init();

    let nlr_root = config::resolve_nlr_root()?;
    info!("NLR_ROOT: {}", nlr_root.display());

    let registry = ToolRegistry::new(nlr_root.clone());
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                error!("stdin read error: {e}");
                break;
            }
        };

        if line.trim().is_empty() {
            continue;
        }

        let request: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let err_resp = JsonRpcResponse::error(
                    Some(Value::Null),
                    -32700,
                    format!("Parse error: {e}"),
                );
                let _ = writeln!(stdout, "{}", serde_json::to_string(&err_resp)?);
                let _ = stdout.flush();
                continue;
            }
        };

        if request.method == "notifications/initialized" {
            info!("Client initialized");
            continue;
        }

        let id = request.id.clone();

        let response = match request.method.as_str() {
            "initialize" => handle_initialize(id),
            "tools/list" => handle_tools_list(id, &registry),
            "tools/call" => handle_tools_call(id, &request, &registry),
            "resources/list" => handle_resources_list(id, &nlr_root),
            "resources/read" => handle_resources_read(id, &request, &nlr_root),
            "prompts/list" => handle_prompts_list(id),
            "prompts/get" => handle_prompts_get(id, &request),
            _ => JsonRpcResponse::error(
                id,
                -32601,
                format!("Method not found: {}", request.method),
            ),
        };

        writeln!(stdout, "{}", serde_json::to_string(&response)?)?;
        stdout.flush()?;
    }

    Ok(())
}

fn handle_initialize(id: Option<Value>) -> JsonRpcResponse {
    JsonRpcResponse::success(
        id,
        serde_json::json!({
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
    )
}

fn handle_tools_list(id: Option<Value>, registry: &ToolRegistry) -> JsonRpcResponse {
    let tools = registry.list_tools();
    JsonRpcResponse::success(id, serde_json::json!({ "tools": tools }))
}

fn handle_tools_call(
    id: Option<Value>,
    req: &JsonRpcRequest,
    registry: &ToolRegistry,
) -> JsonRpcResponse {
    let params = req.params.as_ref();

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
            serde_json::json!({
                "content": [{
                    "type": "text",
                    "text": result
                }]
            }),
        ),
        Err(e) => JsonRpcResponse::success(
            id,
            serde_json::json!({
                "content": [{
                    "type": "text",
                    "text": format!("Error: {e}")
                }],
                "isError": true
            }),
        ),
    }
}

fn handle_resources_list(id: Option<Value>, root: &std::path::Path) -> JsonRpcResponse {
    let kb = root.join("02-KB-main");
    let skip = ["schema.md", "index.md", "log.md"];
    let mut resources = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&kb) {
        fn walk(dir: &std::path::Path, kb: &std::path::Path, skip: &[&str], out: &mut Vec<Value>) {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.filter_map(|e| e.ok()) {
                    let path = entry.path();
                    if path.is_dir() {
                        walk(&path, kb, skip, out);
                    } else if path.extension().is_some_and(|e| e == "md")
                        && !skip.iter().any(|s| path.file_name().is_some_and(|f| f == *s))
                    {
                        let rel = path.strip_prefix(kb).unwrap_or(&path).display().to_string();
                        out.push(serde_json::json!({
                            "uri": format!("nlr://wiki/{rel}"),
                            "name": rel,
                            "mimeType": "text/markdown"
                        }));
                    }
                }
            }
        }
        let _ = entries; // consumed by walk
        walk(&kb, &kb, &skip, &mut resources);
    }

    JsonRpcResponse::success(id, serde_json::json!({ "resources": resources }))
}

fn handle_resources_read(
    id: Option<Value>,
    req: &JsonRpcRequest,
    root: &std::path::Path,
) -> JsonRpcResponse {
    let uri = req
        .params
        .as_ref()
        .and_then(|p| p.get("uri"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let rel_path = uri.strip_prefix("nlr://wiki/").unwrap_or(uri);

    // Block traversal: reject .., absolute paths, and null bytes
    if rel_path.contains("..") || rel_path.starts_with('/') || rel_path.contains('\0') {
        return JsonRpcResponse::error(id, -32602, "Invalid path: traversal not allowed".into());
    }

    let kb_root = root.join("02-KB-main");
    let full_path = kb_root.join(rel_path);

    // Canonicalize and verify resolved path is under kb_root
    let canonical = match full_path.canonicalize() {
        Ok(p) => p,
        Err(e) => return JsonRpcResponse::error(id, -32602, format!("Resource not found: {e}")),
    };
    let kb_canonical = kb_root.canonicalize().unwrap_or(kb_root);
    if !canonical.starts_with(&kb_canonical) {
        return JsonRpcResponse::error(id, -32602, "Access denied: path outside knowledge base".into());
    }

    match std::fs::read_to_string(&canonical) {
        Ok(content) => JsonRpcResponse::success(
            id,
            serde_json::json!({
                "contents": [{
                    "uri": uri,
                    "mimeType": "text/markdown",
                    "text": content
                }]
            }),
        ),
        Err(e) => JsonRpcResponse::error(id, -32602, format!("Resource not found: {e}")),
    }
}

fn handle_prompts_list(id: Option<Value>) -> JsonRpcResponse {
    let prompts = serde_json::json!([
        {
            "name": "wiki-curate",
            "description": "Synthesize raw sources into a wiki page following the Karpathy LLM-Wiki pattern",
            "arguments": [
                { "name": "topic", "description": "Topic to curate", "required": true }
            ]
        },
        {
            "name": "rag-query",
            "description": "Query the knowledge base for relevant context",
            "arguments": [
                { "name": "query", "description": "Search query", "required": true }
            ]
        },
        {
            "name": "brain-scan",
            "description": "Scan the knowledge base for pending tasks, stale pages, and gaps",
            "arguments": []
        }
    ]);
    JsonRpcResponse::success(id, serde_json::json!({ "prompts": prompts }))
}

fn handle_prompts_get(id: Option<Value>, req: &JsonRpcRequest) -> JsonRpcResponse {
    let name = req
        .params
        .as_ref()
        .and_then(|p| p.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let args = req
        .params
        .as_ref()
        .and_then(|p| p.get("arguments"))
        .cloned()
        .unwrap_or(Value::Object(Default::default()));

    let messages = match name {
        "wiki-curate" => {
            let topic = args.get("topic").and_then(|v| v.as_str()).unwrap_or("unknown");
            serde_json::json!([{
                "role": "user",
                "content": {
                    "type": "text",
                    "text": format!("Curate a wiki page for the topic '{}'. Read relevant sources from 00-raw/ and 01-sorted/, synthesize them following the schema in 02-KB-main/schema.md, and create a structured wiki page with YAML frontmatter, citations, contradictions, and open questions.", topic)
                }
            }])
        }
        "rag-query" => {
            let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
            serde_json::json!([{
                "role": "user",
                "content": {
                    "type": "text",
                    "text": format!("Search the neuro-link knowledge base for: {}", query)
                }
            }])
        }
        "brain-scan" => {
            serde_json::json!([{
                "role": "user",
                "content": {
                    "type": "text",
                    "text": "Run a brain scan: check for pending tasks in 07-neuro-link-task/, find stale wiki pages (>30 days), identify knowledge gaps, and report any failures in the deviation log."
                }
            }])
        }
        _ => {
            return JsonRpcResponse::error(id, -32602, format!("Prompt not found: {name}"));
        }
    };

    JsonRpcResponse::success(id, serde_json::json!({ "messages": messages }))
}

// ── CLI dispatch ──

async fn run_cli(cmd: Commands) -> Result<()> {
    let root = config::resolve_nlr_root().unwrap_or_else(|_| {
        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
    });

    match cmd {
        Commands::Mcp => unreachable!(),

        Commands::Init => {
            init::init(&root)?;
            println!("{} Initialized at {}", "OK".green().bold(), root.display());
        }

        Commands::Status => {
            let health = heartbeat::check_health(&root)?;
            let icon = if health.status == "ok" {
                "OK".green().bold()
            } else {
                "ERR".red().bold()
            };
            println!("{icon} {}", root.display());
            println!("  wiki pages:    {}", health.wiki_pages);
            println!("  pending tasks: {}", health.pending_tasks);
            if !health.errors.is_empty() {
                for e in &health.errors {
                    println!("  {}: {e}", "!".red());
                }
            }

            // NLR_ROOT resolution diagnostic (P05).
            // Reports env var, contents of ~/.claude/state/nlr_root, whether
            // the resolved directory exists, and warns on mismatch.
            let diag = config::diagnose_nlr_root();
            let env_disp = if diag.env.is_empty() {
                "<unset>".to_string()
            } else {
                diag.env.clone()
            };
            let file_disp = if diag.file.is_empty() {
                "<missing>".to_string()
            } else {
                diag.file.clone()
            };
            println!(
                "  NLR_ROOT resolution: env={} file={} dir_exists={}",
                env_disp, file_disp, diag.dir_exists
            );
            if diag.mismatch {
                println!(
                    "  {}: NLR_ROOT env ({}) != ~/.claude/state/nlr_root ({})",
                    "WARN".yellow().bold(),
                    env_disp,
                    file_disp
                );
            }
        }

        Commands::Ingest { sources, parallel } => {
            if sources.is_empty() {
                anyhow::bail!("Provide at least one URL or file path");
            }
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()?;

            if parallel && sources.len() > 1 {
                let pb = indicatif::ProgressBar::new(sources.len() as u64);
                pb.set_style(
                    indicatif::ProgressStyle::default_bar()
                        .template("{spinner} [{bar:30}] {pos}/{len} {msg}")?
                        .progress_chars("=> "),
                );
                let results = crawl::parallel_crawl(&sources, 5).await;
                for r in &results {
                    if let Some(err) = &r.error {
                        println!("{} {} — {err}", "FAIL".red(), r.url);
                    } else {
                        let slug = slug_from_url(&r.url);
                        match crawl::ingest_content(&root, &slug, &r.text, &r.url) {
                            Ok(ing) if ing.duplicate => {
                                println!("{} {} (duplicate)", "SKIP".yellow(), r.url)
                            }
                            Ok(ing) => println!(
                                "{} {} -> {}/{}",
                                "OK".green(),
                                r.url,
                                ing.domain,
                                ing.slug
                            ),
                            Err(e) => println!("{} {} — {e}", "FAIL".red(), r.url),
                        }
                    }
                    pb.inc(1);
                }
                pb.finish_and_clear();
            } else {
                for src in &sources {
                    if src.starts_with("http://") || src.starts_with("https://") {
                        let r = crawl::crawl_url(src, &client).await;
                        if let Some(err) = &r.error {
                            println!("{} {src} — {err}", "FAIL".red());
                            continue;
                        }
                        let slug = slug_from_url(src);
                        match crawl::ingest_content(&root, &slug, &r.text, src) {
                            Ok(ing) if ing.duplicate => {
                                println!("{} {src} (duplicate)", "SKIP".yellow())
                            }
                            Ok(ing) => println!(
                                "{} {src} -> {}/{}",
                                "OK".green(),
                                ing.domain,
                                ing.slug
                            ),
                            Err(e) => println!("{} {src} — {e}", "FAIL".red()),
                        }
                    } else {
                        let path = std::path::Path::new(src);
                        if !path.exists() {
                            println!("{} {src} — file not found", "FAIL".red());
                            continue;
                        }
                        let content = std::fs::read_to_string(path)?;
                        let slug = path
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        match crawl::ingest_content(&root, &slug, &content, src) {
                            Ok(ing) if ing.duplicate => {
                                println!("{} {src} (duplicate)", "SKIP".yellow())
                            }
                            Ok(ing) => println!(
                                "{} {src} -> {}/{}",
                                "OK".green(),
                                ing.domain,
                                ing.slug
                            ),
                            Err(e) => println!("{} {src} — {e}", "FAIL".red()),
                        }
                    }
                }
            }
        }

        Commands::Curate { topic } => {
            println!(
                "To curate '{}', use the {} skill in Claude Code:",
                topic.bold(),
                "wiki-curate".cyan()
            );
            println!("  /wiki-curate {topic}");
        }

        Commands::Search { query, limit } => {
            let kb = root.join("02-KB-main");
            let skip = ["schema.md", "index.md", "log.md"];
            let query_lc = query.to_lowercase();
            let mut hits = Vec::new();
            for entry in walkdir::WalkDir::new(&kb)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let path = entry.path();
                if !path.extension().is_some_and(|e| e == "md")
                    || skip
                        .iter()
                        .any(|s| path.file_name().is_some_and(|f| f == *s))
                {
                    continue;
                }
                let content = std::fs::read_to_string(path).unwrap_or_default();
                if content.to_lowercase().contains(&query_lc) {
                    let rel = path
                        .strip_prefix(&kb)
                        .unwrap_or(path)
                        .display()
                        .to_string();
                    hits.push(rel);
                    if hits.len() >= limit {
                        break;
                    }
                }
            }
            if hits.is_empty() {
                println!("No results for '{query}'");
            } else {
                for h in &hits {
                    println!("  {h}");
                }
            }
        }

        Commands::Embed { recreate } => {
            let qdrant_url = std::env::var("QDRANT_URL")
                .unwrap_or_else(|_| "http://localhost:6333".into());
            let count = embed::embed_wiki(&root, &qdrant_url, recreate).await?;
            println!("{} Embedded {count} pages into Qdrant", "OK".green());
        }

        Commands::Resort => {
            let report = resort::resort(&root)?;
            println!(
                "{} Resort: moved {} slug(s), skipped {} (already classified)",
                "OK".green().bold(),
                report.moved,
                report.skipped
            );
        }

        Commands::CurateBacklog { limit } => {
            let report =
                curate_backlog::drain(&root, limit, &curate_backlog::ClaudeCurator)?;
            println!(
                "{} Curate-backlog: attempted {}, ok {}, failed {}, remaining {}",
                "OK".green().bold(),
                report.attempted,
                report.succeeded,
                report.failed,
                report.remaining
            );
        }

        Commands::Grade { session, wiki } => {
            let do_both = !session && !wiki;
            if session || do_both {
                let sg = grade::grade_session(&root)?;
                println!("{}", "Session Grades".bold());
                println!(
                    "  calls: {} | success: {} ({:.0}%)",
                    sg.total_calls,
                    sg.success_count,
                    sg.success_rate * 100.0
                );
                if !sg.tool_distribution.is_empty() {
                    let mut tools: Vec<_> = sg.tool_distribution.iter().collect();
                    tools.sort_by(|a, b| b.1.cmp(a.1));
                    for (tool, count) in tools.iter().take(10) {
                        println!("    {tool}: {count}");
                    }
                }
                grade::append_score(&root, "session_success_rate", sg.success_rate, Some(0.95))?;
            }
            if wiki || do_both {
                let wg = grade::grade_wiki(&root)?;
                println!("{}", "Wiki Grades".bold());
                println!("  pages: {}", wg.total_pages);
                println!("  open questions: {}", wg.open_questions);
                println!("  contradictions: {}", wg.contradictions);
                println!("  stale (>30d): {}", wg.stale_pages);
                println!("  confidence mode: {}", wg.avg_confidence);
            }
        }

        Commands::Heartbeat { interval } => {
            println!("Starting heartbeat daemon (interval: {interval}s)");
            heartbeat::run_daemon(&root, interval).await;
        }

        Commands::Scan => {
            let health = heartbeat::check_health(&root)?;
            println!("{}", "Brain Scan".bold());
            println!("  status:        {}", health.status);
            println!("  wiki pages:    {}", health.wiki_pages);
            println!("  pending tasks: {}", health.pending_tasks);
            if !health.errors.is_empty() {
                println!("  {}", "Issues:".red());
                for e in &health.errors {
                    println!("    - {e}");
                }
            }
            // Staleness
            let kb = root.join("02-KB-main");
            let now = chrono::Utc::now().date_naive();
            let mut stale_count = 0;
            for entry in walkdir::WalkDir::new(&kb)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let path = entry.path();
                if !path.extension().is_some_and(|e| e == "md") {
                    continue;
                }
                let content = std::fs::read_to_string(path).unwrap_or_default();
                if let Some(line) = content.lines().find(|l| l.starts_with("last_updated:")) {
                    let date_part = line.split(':').nth(1).unwrap_or("").trim();
                    if let Ok(date) = chrono::NaiveDate::parse_from_str(date_part, "%Y-%m-%d") {
                        if (now - date).num_days() > 30 {
                            stale_count += 1;
                        }
                    }
                }
            }
            println!("  stale pages:   {stale_count}");
        }

        Commands::Tasks { action } => match action {
            None => {
                let filter = "all";
                let task_dir = root.join("07-neuro-link-task");
                if !task_dir.is_dir() {
                    println!("No tasks directory");
                    return Ok(());
                }
                let mut count = 0;
                for entry in std::fs::read_dir(&task_dir)? {
                    let path = entry?.path();
                    if path.extension().is_some_and(|e| e == "md") {
                        let fname = path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        println!("  {fname}");
                        count += 1;
                    }
                }
                if count == 0 {
                    println!("No tasks matching filter '{filter}'");
                }
            }
            Some(TaskAction::List { status }) => {
                let filter = status;
                let task_dir = root.join("07-neuro-link-task");
                if !task_dir.is_dir() {
                    println!("No tasks directory");
                    return Ok(());
                }
                let mut count = 0;
                for entry in std::fs::read_dir(&task_dir)? {
                    let path = entry?.path();
                    if path.extension().is_some_and(|e| e == "md") {
                        let content = std::fs::read_to_string(&path).unwrap_or_default();
                        if filter != "all" && !content.contains(&format!("status: {filter}")) {
                            continue;
                        }
                        let fname = path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        println!("  {fname}");
                        count += 1;
                    }
                }
                if count == 0 {
                    println!("No tasks matching filter '{filter}'");
                }
            }
            Some(TaskAction::Create {
                title,
                task_type,
                priority,
            }) => {
                let args = serde_json::json!({
                    "title": title,
                    "type": task_type,
                    "priority": priority,
                });
                let result = tools::tasks::call("nlr_task_create", &args, &root)?;
                println!("{} {result}", "OK".green());
            }
        },

        Commands::Config { name } => {
            let name = name.as_deref().unwrap_or("neuro-link");
            let path = root.join("config").join(format!("{name}.md"));
            if !path.exists() {
                anyhow::bail!("Config not found: {name}");
            }
            let fm = config::parse_frontmatter(&path)?;
            for (k, v) in &fm {
                println!("  {}: {}", k.bold(), v);
            }
        }

        Commands::Serve { port, bind, token, insecure_no_auth, tunnel, tunnel_domain, no_watch } => {
            // Initialise tracing so watcher_inbox + other modules' info!/warn!/error! surface.
            // Other command paths (Start/Worker/Supervise/MCP) already call this; Serve was missed.
            let _ = tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
                )
                .with_target(false)
                .try_init();
            if insecure_no_auth {
                std::env::set_var("NLR_INSECURE_NO_AUTH", "1");
                eprintln!("{} Running WITHOUT authentication (--insecure-no-auth)", "WARN".yellow().bold());
            }
            // Handle token: auto-generate if "auto", use env var, or use provided
            if let Some(ref t) = token {
                if t == "auto" {
                    use rand::Rng;
                    let generated: String = rand::rng()
                        .sample_iter(&rand::distr::Alphanumeric)
                        .take(32)
                        .map(char::from)
                        .collect();
                    std::env::set_var("NLR_API_TOKEN", &generated);
                    eprintln!("{} API token (auto): {}", "OK".green().bold(), generated);
                    // Persist to secrets/.env if possible
                    let env_path = root.join("secrets/.env");
                    if let Ok(existing) = std::fs::read_to_string(&env_path) {
                        if !existing.contains("NLR_API_TOKEN") {
                            let _ = std::fs::write(
                                &env_path,
                                format!("{existing}\nNLR_API_TOKEN={generated}\n"),
                            );
                        }
                    }
                } else {
                    std::env::set_var("NLR_API_TOKEN", t);
                }
            }

            let registry = std::sync::Arc::new(ToolRegistry::new(root.clone()));
            let app = api::build_router(registry);
            let addr = format!("{bind}:{port}");
            if !no_watch {
                spawn_inbox_watcher(&root);
            }

            // A-fu5: spawn the heartbeat daemon in-process so state/heartbeat.json
            // stays fresh without requiring a separate `neuro-link heartbeat` process.
            // Default cadence 60s; reads NLR_HEARTBEAT_INTERVAL env var for override.
            {
                let hb_root = root.clone();
                let interval_secs: u64 = std::env::var("NLR_HEARTBEAT_INTERVAL")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(60);
                tokio::spawn(async move {
                    heartbeat::run_daemon(&hb_root, interval_secs).await;
                });
                eprintln!(
                    "{} Heartbeat daemon spawned (interval={interval_secs}s)",
                    "OK".green().bold()
                );
            }

            let listener = tokio::net::TcpListener::bind(&addr).await?;
            eprintln!("{} HTTP server listening on {addr}", "OK".green().bold());
            eprintln!("  POST /mcp           — MCP JSON-RPC");
            eprintln!("  GET  /health        — health check (no auth)");
            eprintln!("  GET  /api/v1/...    — REST API");

            if tunnel {
                // Start ngrok tunnel in background
                let mut cmd = vec!["ngrok".to_string(), "http".to_string(), port.to_string()];
                if let Some(ref domain) = tunnel_domain {
                    cmd.push(format!("--domain={domain}"));
                }
                eprintln!("{} Starting ngrok tunnel...", "OK".green().bold());
                let _ngrok = tokio::process::Command::new(&cmd[0])
                    .args(&cmd[1..])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();
                match _ngrok {
                    Ok(_) => {
                        if let Some(ref domain) = tunnel_domain {
                            eprintln!("  Tunnel: https://{domain}");
                        } else {
                            eprintln!("  Tunnel: check ngrok dashboard for URL");
                        }
                    }
                    Err(e) => eprintln!("{} ngrok not found: {e}", "WARN".yellow().bold()),
                }
            }

            axum::serve(listener, app).await?;
        }

        Commands::Graph { action } => match action {
            GraphAction::AddFact {
                subject,
                predicate,
                object,
            } => {
                let valid_from = chrono::Utc::now().to_rfc3339();
                graph::add_fact(&subject, &predicate, &object, &valid_from).await?;
                println!(
                    "{} ({subject})-[{predicate}]->({object})",
                    "OK".green()
                );
            }
            GraphAction::Query { topic } => {
                let facts = graph::query_facts(&topic).await?;
                if facts.is_empty() {
                    println!("No facts found for '{topic}'");
                } else {
                    for f in &facts {
                        println!(
                            "  ({}) -[{}]-> ({})  [{}]",
                            f.subject, f.predicate, f.object, f.valid_from
                        );
                    }
                }
            }
        },

        Commands::Worker { interval } => {
            tracing_subscriber::fmt()
                .with_env_filter(
                    std::env::var("RUST_LOG").unwrap_or_else(|_| "neuro_link_mcp=info".into()),
                )
                .with_writer(io::stderr)
                .try_init()
                .ok();
            println!(
                "{} Self-improvement worker (interval {} min) at {}",
                "OK".green().bold(),
                interval,
                root.display()
            );
            worker::run(root.clone(), interval).await?;
        }

        Commands::Supervise => {
            tracing_subscriber::fmt()
                .with_env_filter(
                    std::env::var("RUST_LOG").unwrap_or_else(|_| "neuro_link_mcp=info".into()),
                )
                .with_writer(io::stderr)
                .try_init()
                .ok();
            println!(
                "{} Supervisor started at {}",
                "OK".green().bold(),
                root.display()
            );
            supervisor::run(&root).await?;
        }

        Commands::Sessions { action } => {
            let claude_root = sessions::claude_root();
            let vault_from_config = || -> Option<std::path::PathBuf> {
                let config = root.join("config/neuro-link.md");
                let content = std::fs::read_to_string(&config).ok()?;
                for line in content.lines() {
                    if let Some(v) = line.strip_prefix("obsidian_vault:") {
                        let p = v.trim();
                        if !p.is_empty() {
                            return Some(std::path::PathBuf::from(p));
                        }
                    }
                }
                None
            };

            match action {
                SessionsAction::Parse { since, vault } => {
                    let vault_path = vault.or_else(vault_from_config).unwrap_or_else(|| root.clone());
                    let since_dt = since.as_ref().and_then(|s| {
                        chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
                            .ok()
                            .and_then(|d| d.and_hms_opt(0, 0, 0))
                            .map(|dt| dt.and_utc())
                    });

                    let mut parsed = if let Some(dt) = since_dt {
                        sessions::parser::parse_sessions_since(&claude_root, dt)?
                    } else {
                        sessions::parser::parse_all_sessions(&claude_root)?
                    };
                    println!("{} Parsed {} sessions from {}", "OK".green().bold(), parsed.len(), claude_root.display());

                    for s in parsed.iter_mut() {
                        s.quality_flags = sessions::quality::analyze(s);
                    }

                    let mut written = 0;
                    for s in &parsed {
                        match sessions::markdown::write_session_markdown(s, &vault_path) {
                            Ok(p) => {
                                println!("  → {}", p.display());
                                written += 1;
                            }
                            Err(e) => eprintln!("  {} {e}", "FAIL".red()),
                        }
                    }
                    println!("{} Wrote {} markdown files to {}", "OK".green().bold(), written, vault_path.join("sessions").display());
                }

                SessionsAction::List { days } => {
                    let days = days.unwrap_or(7);
                    let since_dt = chrono::Utc::now() - chrono::Duration::days(days as i64);
                    let sessions_list = sessions::parser::parse_sessions_since(&claude_root, since_dt)?;
                    println!("{}", format!("Sessions in last {} days: {}", days, sessions_list.len()).bold());
                    for s in &sessions_list {
                        let total_tools: u32 = s.tools_used.values().sum();
                        println!(
                            "  [{}] {} — {} turns, {} tool calls, {} skills, ${:.2}",
                            s.started.format("%Y-%m-%d %H:%M"),
                            &s.session_id[..s.session_id.len().min(8)],
                            s.turns.len(),
                            total_tools,
                            s.skills_invoked.len(),
                            s.tokens.cost_usd_estimate
                        );
                    }
                }

                SessionsAction::Scan { days } => {
                    let days = days.unwrap_or(7);
                    let since_dt = chrono::Utc::now() - chrono::Duration::days(days as i64);
                    let mut sessions_list = sessions::parser::parse_sessions_since(&claude_root, since_dt)?;

                    let mut total_flags = 0;
                    let mut flag_counts: std::collections::HashMap<&str, u32> = Default::default();
                    for s in sessions_list.iter_mut() {
                        s.quality_flags = sessions::quality::analyze(s);
                        for f in &s.quality_flags {
                            *flag_counts.entry(f.flag_type.as_str()).or_default() += 1;
                        }
                        total_flags += s.quality_flags.len();
                    }

                    println!("{}", format!("Quality Scan ({} days)", days).bold());
                    println!("  Sessions analyzed: {}", sessions_list.len());
                    println!("  Total flags: {}", total_flags);
                    if flag_counts.is_empty() {
                        println!("  {} No issues detected", "✓".green());
                    } else {
                        println!("  By type:");
                        let mut sorted: Vec<_> = flag_counts.into_iter().collect();
                        sorted.sort_by(|a, b| b.1.cmp(&a.1));
                        for (kind, n) in sorted {
                            println!("    {}: {}", kind, n);
                        }
                    }

                    // Worst offenders
                    let mut by_session: Vec<_> = sessions_list.iter()
                        .filter(|s| !s.quality_flags.is_empty())
                        .collect();
                    by_session.sort_by(|a, b| b.quality_flags.len().cmp(&a.quality_flags.len()));
                    if !by_session.is_empty() {
                        println!("  Worst offenders:");
                        for s in by_session.iter().take(5) {
                            println!(
                                "    [{}] {} flags — {}",
                                &s.session_id[..s.session_id.len().min(8)],
                                s.quality_flags.len(),
                                s.started.format("%Y-%m-%d %H:%M")
                            );
                        }
                    }
                }

                SessionsAction::ExportLlm { vault } => {
                    let vault_path = vault.or_else(vault_from_config).unwrap_or_else(|| root.clone());
                    let llm_logs = root.join("state/llm_logs");
                    if !llm_logs.is_dir() {
                        println!("{} No llm_logs directory at {}", "INFO".cyan(), llm_logs.display());
                        return Ok(());
                    }
                    let written = sessions::llm_export::export_llm_sessions(&llm_logs, &vault_path)?;
                    println!("{} Wrote {} markdown files to {}", "OK".green().bold(), written.len(), vault_path.join("llm-sessions").display());
                }

                SessionsAction::Watch { vault, interval } => {
                    let vault_path = vault.or_else(vault_from_config).unwrap_or_else(|| root.clone());
                    println!("{} Watching {} → {}", "OK".green().bold(), claude_root.display(), vault_path.display());
                    sessions::watcher::run(claude_root, vault_path, interval).await?;
                }
            }
        }

        Commands::Start {
            port,
            bind,
            token,
            insecure_no_auth,
            tunnel,
            tunnel_domain,
            worker_interval,
            no_embedding,
            no_worker,
            no_supervise,
            no_sessions_watch,
            no_watch,
        } => {
            tracing_subscriber::fmt()
                .with_env_filter(
                    std::env::var("RUST_LOG").unwrap_or_else(|_| "neuro_link_mcp=info".into()),
                )
                .with_writer(io::stderr)
                .try_init()
                .ok();

            // Set NLR_ROOT for child processes
            std::env::set_var("NLR_ROOT", root.as_os_str());

            // Handle auth token
            if insecure_no_auth {
                std::env::set_var("NLR_INSECURE_NO_AUTH", "1");
                eprintln!("{} Running WITHOUT auth (--insecure-no-auth)", "WARN".yellow().bold());
            }
            let resolved_token = if let Some(t) = token {
                if t == "auto" {
                    use rand::Rng;
                    let generated: String = rand::rng()
                        .sample_iter(&rand::distr::Alphanumeric)
                        .take(32)
                        .map(char::from)
                        .collect();
                    std::env::set_var("NLR_API_TOKEN", &generated);
                    eprintln!("{} API token (auto): {}", "OK".green().bold(), generated);
                    // Persist
                    let env_path = root.join("secrets/.env");
                    if let Ok(existing) = std::fs::read_to_string(&env_path) {
                        if !existing.contains("NLR_API_TOKEN=") || existing.contains("NLR_API_TOKEN=\n") {
                            let _ = std::fs::write(
                                &env_path,
                                format!("{existing}\nNLR_API_TOKEN={generated}\n"),
                            );
                        }
                    }
                    generated
                } else {
                    std::env::set_var("NLR_API_TOKEN", &t);
                    t
                }
            } else {
                // Load from secrets/.env if not provided
                let env_path = root.join("secrets/.env");
                if let Ok(content) = std::fs::read_to_string(&env_path) {
                    for line in content.lines() {
                        if let Some(rest) = line.strip_prefix("NLR_API_TOKEN=") {
                            let val = rest.trim().to_string();
                            if !val.is_empty() {
                                std::env::set_var("NLR_API_TOKEN", &val);
                            }
                            break;
                        }
                    }
                }
                std::env::var("NLR_API_TOKEN").unwrap_or_default()
            };

            eprintln!();
            eprintln!("{}", "━━━ neuro-link all-in-one ━━━".bold());
            eprintln!("  Root:     {}", root.display());
            eprintln!("  HTTP:     http://{bind}:{port}");
            eprintln!("  Dash:     http://localhost:{port}/");
            eprintln!("  MCP:      http://localhost:{port}/mcp");
            eprintln!("  LLM prox: http://localhost:{port}/llm/v1");
            eprintln!("  Token:    {}", &resolved_token[..resolved_token.len().min(12)]);
            eprintln!();

            let mut handles = Vec::new();

            // 1) Embedding server (spawned subprocess)
            if !no_embedding {
                let script = root.join("scripts/embedding-server.sh");
                if script.exists() {
                    eprintln!("[start] launching embedding server (llama-server with Octen-8B)");
                    let log_file = std::fs::File::create("/tmp/neuro-link-embedding.log").ok();
                    let (stdout, stderr) = if let Some(f) = log_file {
                        let f2 = f.try_clone().unwrap();
                        (std::process::Stdio::from(f), std::process::Stdio::from(f2))
                    } else {
                        (std::process::Stdio::null(), std::process::Stdio::null())
                    };
                    let _ = tokio::process::Command::new("sh")
                        .arg(&script)
                        .stdout(stdout)
                        .stderr(stderr)
                        .env("NLR_ROOT", root.as_os_str())
                        .spawn();
                    eprintln!("  → logs: /tmp/neuro-link-embedding.log");
                } else {
                    eprintln!("{} embedding-server.sh not found at {}", "WARN".yellow(), script.display());
                }
            }

            // 2) Supervisor task
            if !no_supervise {
                let root_clone = root.clone();
                handles.push(tokio::spawn(async move {
                    eprintln!("[start] supervisor loop running");
                    let _ = supervisor::run(&root_clone).await;
                }));
            }

            // 3) Worker task
            if !no_worker {
                let root_clone = root.clone();
                handles.push(tokio::spawn(async move {
                    eprintln!("[start] self-improvement worker running (interval {worker_interval}m)");
                    let _ = worker::run(root_clone, worker_interval).await;
                }));
            }

            // 3.5) Session watcher task
            if !no_sessions_watch {
                let claude_root = sessions::claude_root();
                let vault_path = {
                    let config = root.join("config/neuro-link.md");
                    let mut vp = root.clone();
                    if let Ok(content) = std::fs::read_to_string(&config) {
                        for line in content.lines() {
                            if let Some(v) = line.strip_prefix("obsidian_vault:") {
                                let p = v.trim();
                                if !p.is_empty() {
                                    vp = std::path::PathBuf::from(p);
                                    break;
                                }
                            }
                        }
                    }
                    vp
                };
                handles.push(tokio::spawn(async move {
                    eprintln!("[start] session watcher running ({} → {})", claude_root.display(), vault_path.display());
                    let _ = sessions::watcher::run(claude_root, vault_path, 30).await;
                }));
            }

            if !no_watch {
                let root_clone = root.clone();
                handles.push(tokio::spawn(async move {
                    eprintln!("[start] inbox watcher running");
                    let _ = watcher_inbox::run(root_clone).await;
                }));
            }

            // 4) Ngrok tunnel (if requested)
            if tunnel {
                let mut cmd = vec!["ngrok".to_string(), "http".to_string(), port.to_string()];
                if let Some(ref domain) = tunnel_domain {
                    cmd.push(format!("--domain={domain}"));
                }
                eprintln!("[start] launching ngrok tunnel");
                let _ = tokio::process::Command::new(&cmd[0])
                    .args(&cmd[1..])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();
            }

            // 5) HTTP server (foreground, this blocks until Ctrl-C)
            eprintln!("[start] HTTP server: http://{bind}:{port}");
            let registry = std::sync::Arc::new(ToolRegistry::new(root.clone()));
            let app = api::build_router(registry);
            let addr = format!("{bind}:{port}");
            let listener = tokio::net::TcpListener::bind(&addr).await?;
            axum::serve(listener, app).await?;

            // Cleanup (unreachable under normal exit)
            for h in handles { h.abort(); }
        }

        Commands::Workflow { action } => match action {
            WorkflowAction::Create { name } => {
                let id = workflow::create_workflow(&root, &name)?;
                println!("{} workflow '{name}' id={id}", "OK".green());
            }
            WorkflowAction::Transition { id, state } => {
                workflow::transition(&root, &id, &state)?;
                println!("{} {id} -> {state}", "OK".green());
            }
            WorkflowAction::List => {
                let wfs = workflow::list_workflows(&root)?;
                if wfs.is_empty() {
                    println!("No workflows");
                } else {
                    for wf in &wfs {
                        println!(
                            "  {} {} [{}] ({})",
                            wf.id, wf.name, wf.state, wf.created
                        );
                    }
                }
            }
        },
    }

    Ok(())
}

fn slug_from_url(url: &str) -> String {
    url.split('/')
        .last()
        .unwrap_or("page")
        .split('?')
        .next()
        .unwrap_or("page")
        .split('#')
        .next()
        .unwrap_or("page")
        .replace(|c: char| !c.is_alphanumeric() && c != '-' && c != '_', "-")
        .trim_matches('-')
        .to_string()
}
