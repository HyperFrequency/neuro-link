use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn nlr_binary() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("target");
    // Use debug build for tests
    path.push("debug");
    path.push("neuro-link");
    path
}

struct McpClient {
    child: std::process::Child,
    stdin: Option<std::process::ChildStdin>,
    reader: BufReader<std::process::ChildStdout>,
    next_id: u64,
}

impl McpClient {
    fn spawn(nlr_root: &std::path::Path) -> Self {
        let binary = nlr_binary();
        if !binary.exists() {
            panic!("neuro-link binary not found at {:?}. Run `cargo build` first.", binary);
        }

        let mut child = Command::new(&binary)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .env("NLR_ROOT", nlr_root.to_str().unwrap())
            .spawn()
            .expect("failed to spawn nlr");

        let stdin = child.stdin.take().unwrap();
        let reader = BufReader::new(child.stdout.take().unwrap());

        let mut client = McpClient {
            child,
            stdin: Some(stdin),
            reader,
            next_id: 0,
        };

        // Send initialize
        let _ = client.send("initialize", None);
        client
    }

    fn send(&mut self, method: &str, params: Option<Value>) -> Value {
        self.next_id += 1;
        let mut msg = json!({
            "jsonrpc": "2.0",
            "method": method,
            "id": self.next_id,
        });
        if let Some(p) = params {
            msg["params"] = p;
        }
        let line = format!("{}\n", serde_json::to_string(&msg).unwrap());
        let stdin = self.stdin.as_mut().unwrap();
        stdin.write_all(line.as_bytes()).unwrap();
        stdin.flush().unwrap();

        let mut response = String::new();
        self.reader.read_line(&mut response).unwrap();
        serde_json::from_str(&response).unwrap()
    }

    fn call_tool(&mut self, name: &str, arguments: Option<Value>) -> Value {
        let mut params = json!({"name": name});
        if let Some(args) = arguments {
            params["arguments"] = args;
        }
        self.send("tools/call", Some(params))
    }

    fn tool_text(&mut self, name: &str, arguments: Option<Value>) -> String {
        let resp = self.call_tool(name, arguments);
        resp["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_string()
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        // Close stdin to signal EOF, then wait for the child to exit
        drop(self.stdin.take());
        let _ = self.child.wait();
    }
}

fn setup_nlr_root() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();

    let dirs = [
        "00-raw",
        "01-sorted/books",
        "01-sorted/arxiv",
        "01-sorted/medium",
        "01-sorted/huggingface",
        "01-sorted/github",
        "01-sorted/docs",
        "02-KB-main",
        "03-ontology-main/domain",
        "03-ontology-main/agent",
        "03-ontology-main/workflow",
        "04-KB-agents-workflows",
        "05-insights-gaps",
        "05-self-improvement-HITL/models",
        "06-self-improvement-recursive/harness-to-harness-comms",
        "06-progress-reports",
        "07-neuro-link-task",
        "08-code-docs/my-repos",
        "09-business-docs",
        "config",
        "state",
        "secrets",
        "scripts",
        "skills/neuro-link",
        "hooks",
    ];

    for d in &dirs {
        std::fs::create_dir_all(root.join(d)).unwrap();
    }

    std::fs::write(
        root.join("CLAUDE.md"),
        "# neuro-link-recursive\nTest scaffold.\n",
    )
    .unwrap();

    std::fs::write(
        root.join("02-KB-main/schema.md"),
        "---\ntitle: Schema\n---\n# Wiki Schema\n",
    )
    .unwrap();
    std::fs::write(root.join("02-KB-main/index.md"), "# Index\n").unwrap();
    std::fs::write(root.join("02-KB-main/log.md"), "# Mutation Log\n").unwrap();

    std::fs::write(
        root.join("config/neuro-link.md"),
        "---\nversion: 1\nauto_rag: true\n---\n# Master Config\n",
    )
    .unwrap();

    std::fs::write(
        root.join("config/harness-harness-comms.md"),
        "---\nversion: 1\nenabled: false\nbridge_mode: mcp2cli\n---\n# Harness Comms\n",
    )
    .unwrap();

    std::fs::write(
        root.join("state/heartbeat.json"),
        r#"{"status":"ok","last_check":"2026-01-01T00:00:00Z","errors":[]}"#,
    )
    .unwrap();
    std::fs::write(root.join("state/session_log.jsonl"), "").unwrap();
    std::fs::write(root.join("state/score_history.jsonl"), "").unwrap();
    std::fs::write(root.join("state/deviation_log.jsonl"), "").unwrap();

    tmp
}

// -------------------------------------------------------------------------
// Tests
// -------------------------------------------------------------------------

#[test]
fn test_cli_status() {
    let tmp = setup_nlr_root();
    let mut client = McpClient::spawn(tmp.path());
    let text = client.tool_text("nlr_scan_health", None);
    let data: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(data["status"], "ok");
    assert_eq!(data["errors"], json!([]));
}

#[test]
fn test_cli_init_creates_structure() {
    let tmp = setup_nlr_root();
    let root = tmp.path();
    // Verify the setup created expected directories
    assert!(root.join("00-raw").is_dir());
    assert!(root.join("02-KB-main").is_dir());
    assert!(root.join("07-neuro-link-task").is_dir());
    assert!(root.join("config").is_dir());
    assert!(root.join("state").is_dir());

    // Verify MCP server starts and responds to initialize
    let mut client = McpClient::spawn(root);
    let resp = client.send("initialize", None);
    assert_eq!(resp["result"]["serverInfo"]["name"], "neuro-link-recursive");
}

#[test]
fn test_cli_ingest_url() {
    let tmp = setup_nlr_root();
    let mut client = McpClient::spawn(tmp.path());
    let text = client.tool_text(
        "nlr_ingest",
        Some(json!({
            "slug": "test-article",
            "content": "Test article body for ingestion",
            "url": "https://example.com/test",
            "source_type": "web",
        })),
    );
    let data: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(data["status"], "ingested");
    assert!(tmp.path().join("00-raw/test-article/source.md").exists());
    assert!(tmp.path().join("00-raw/test-article/metadata.json").exists());
}

#[test]
fn test_cli_search_empty() {
    let tmp = setup_nlr_root();
    let mut client = McpClient::spawn(tmp.path());
    let text = client.tool_text(
        "nlr_wiki_search",
        Some(json!({"query": "xyznonexistent123"})),
    );
    let hits: Vec<Value> = serde_json::from_str(&text).unwrap();
    assert!(hits.is_empty());
}

#[test]
fn test_cli_tasks_list_empty() {
    let tmp = setup_nlr_root();
    let mut client = McpClient::spawn(tmp.path());
    let text = client.tool_text(
        "nlr_task_list",
        Some(json!({"status_filter": "pending"})),
    );
    let tasks: Vec<Value> = serde_json::from_str(&text).unwrap();
    assert!(tasks.is_empty());
}

#[test]
fn test_cli_tasks_create() {
    let tmp = setup_nlr_root();
    let mut client = McpClient::spawn(tmp.path());
    let text = client.tool_text(
        "nlr_task_create",
        Some(json!({
            "title": "Ingest NautilusTrader docs",
            "type": "ingest",
            "priority": 1,
            "body": "Scrape release notes.",
        })),
    );
    assert!(text.contains("Created"));

    let task_dir = tmp.path().join("07-neuro-link-task");
    let files: Vec<_> = std::fs::read_dir(&task_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "md"))
        .collect();
    assert_eq!(files.len(), 1);

    let content = std::fs::read_to_string(files[0].path()).unwrap();
    assert!(content.contains("status: pending"));
    assert!(content.contains("priority: 1"));
}

#[test]
fn test_cli_config_read() {
    let tmp = setup_nlr_root();
    let mut client = McpClient::spawn(tmp.path());
    let text = client.tool_text(
        "nlr_config_read",
        Some(json!({"name": "neuro-link"})),
    );
    let data: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(data["version"], "1");
}

#[test]
fn test_cli_heartbeat() {
    let tmp = setup_nlr_root();
    let mut client = McpClient::spawn(tmp.path());
    let text = client.tool_text("nlr_state_heartbeat", Some(json!({"action": "read"})));
    let data: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(data["status"], "ok");
}

#[test]
fn test_cli_scan() {
    let tmp = setup_nlr_root();
    let mut client = McpClient::spawn(tmp.path());
    let text = client.tool_text(
        "nlr_scan_staleness",
        Some(json!({"threshold_days": 30})),
    );
    let stale: Vec<Value> = serde_json::from_str(&text).unwrap();
    assert!(stale.is_empty());
}

#[test]
fn test_cli_grade_session() {
    let tmp = setup_nlr_root();
    let mut client = McpClient::spawn(tmp.path());
    // Log a tool invocation first
    client.call_tool(
        "nlr_state_log",
        Some(json!({"tool": "test-tool", "exit_code": 0})),
    );
    // Read back the log to confirm it was written
    let log_path = tmp.path().join("state/session_log.jsonl");
    let log_content = std::fs::read_to_string(&log_path).unwrap();
    assert!(!log_content.trim().is_empty());
    let entry: Value = serde_json::from_str(log_content.trim().lines().last().unwrap()).unwrap();
    assert_eq!(entry["tool"], "test-tool");
    assert_eq!(entry["success"], true);
}
