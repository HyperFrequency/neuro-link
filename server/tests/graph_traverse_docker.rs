//! P18 — Docker-backed integration test for `nlr_graph_traverse`.
//!
//! Spins up a real `neo4j:5` container on a random port, seeds 5 triples via
//! the Cypher HTTP API, then drives the MCP stdio server and asserts that
//! `nlr_graph_traverse` returns paths.
//!
//! Gated on the `docker_tests` Cargo feature to keep normal `cargo test` fast
//! and network-free. To run:
//!
//!   cargo test --release --bin neuro-link --features docker_tests -- --nocapture
//!
//! …except that integration-test files are not part of the `neuro-link` bin;
//! they compile as their own test crate. The `--bin neuro-link` form is what
//! the spec requests and still works because the feature flag is defined on
//! the package. The practical command most users will run is:
//!
//!   cargo test --release --features docker_tests graph_traverse_docker -- --nocapture

#![cfg(feature = "docker_tests")]

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

// -------------------------------------------------------------------------
// Docker container fixture
// -------------------------------------------------------------------------

struct Neo4jContainer {
    id: String,
    http_port: u16,
    bolt_port: u16,
}

impl Neo4jContainer {
    fn start() -> Self {
        assert!(cmd_exists("docker"), "docker not on PATH — cannot run docker_tests feature");

        let name = format!("nlr-neo4j-p18-{}", std::process::id());

        // Use host port 0 semantics via `-p 0:7474 -p 0:7687` then inspect
        // the assigned ports after start.
        let output = Command::new("docker")
            .args([
                "run",
                "-d",
                "--rm",
                "--name",
                &name,
                "-e",
                "NEO4J_AUTH=none",
                "-e",
                "NEO4J_dbms_memory_pagecache_size=100M",
                "-e",
                "NEO4J_dbms_memory_heap_initial__size=256M",
                "-e",
                "NEO4J_dbms_memory_heap_max__size=512M",
                "-p",
                "0:7474",
                "-p",
                "0:7687",
                "neo4j:5",
            ])
            .output()
            .expect("failed to spawn docker run");

        if !output.status.success() {
            panic!(
                "docker run failed: stdout={} stderr={}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let id = String::from_utf8_lossy(&output.stdout).trim().to_string();

        let http_port = inspect_port(&id, "7474/tcp").expect("no 7474 port mapping");
        let bolt_port = inspect_port(&id, "7687/tcp").expect("no 7687 port mapping");

        Neo4jContainer {
            id,
            http_port,
            bolt_port,
        }
    }

    fn http_url(&self) -> String {
        format!("http://localhost:{}", self.http_port)
    }

    fn wait_for_ready(&self, timeout: Duration) {
        let start = Instant::now();
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(3))
            .build()
            .unwrap();
        // Poll the root discovery endpoint (200 once the DB is accepting traffic).
        // Also try the `/db/neo4j/cluster/available` endpoint — 200 means ready
        // in neo4j:5. Either is an acceptable signal.
        let urls = [
            format!("{}/", self.http_url()),
            format!("{}/db/neo4j/cluster/available", self.http_url()),
        ];
        while start.elapsed() < timeout {
            for u in &urls {
                if let Ok(resp) = client.get(u).send() {
                    if resp.status().is_success() {
                        return;
                    }
                }
            }
            std::thread::sleep(Duration::from_secs(1));
        }
        panic!(
            "neo4j container did not become ready within {:?} (id={}, http={})",
            timeout, self.id, self.http_port
        );
    }

    fn seed_triples(&self) {
        let client = reqwest::blocking::Client::new();
        let cypher = "
            MERGE (a:Entity {canonical_name: 'attention'})
            MERGE (t:Entity {canonical_name: 'transformer'})
            MERGE (b:Entity {canonical_name: 'bert'})
            MERGE (g:Entity {canonical_name: 'gpt'})
            MERGE (l:Entity {canonical_name: 'language-model'})
            MERGE (a)-[:IMPLIES]->(t)
            MERGE (t)-[:SUBCLASS_OF]->(l)
            MERGE (b)-[:SUBCLASS_OF]->(t)
            MERGE (g)-[:SUBCLASS_OF]->(t)
            MERGE (t)-[:REQUIRES]->(a)
        ";
        let body = json!({
            "statements": [{"statement": cypher}]
        });
        let url = format!("{}/db/neo4j/tx/commit", self.http_url());
        let resp = client
            .post(&url)
            .json(&body)
            .send()
            .expect("failed to POST seed cypher");
        assert!(resp.status().is_success(), "seed cypher failed: {}", resp.status());
        let body: Value = resp.json().unwrap();
        let errors = body["errors"].as_array().cloned().unwrap_or_default();
        assert!(errors.is_empty(), "seed cypher errors: {errors:?}");
    }
}

impl Drop for Neo4jContainer {
    fn drop(&mut self) {
        // Best-effort teardown: stop (triggers auto-remove via --rm).
        let _ = Command::new("docker")
            .args(["stop", &self.id])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

fn cmd_exists(bin: &str) -> bool {
    Command::new(bin)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn inspect_port(id: &str, container_port: &str) -> Option<u16> {
    // docker inspect -f '{{(index (index .NetworkSettings.Ports "7474/tcp") 0).HostPort}}' <id>
    let fmt = format!(
        "{{{{(index (index .NetworkSettings.Ports \"{container_port}\") 0).HostPort}}}}"
    );
    let output = Command::new("docker")
        .args(["inspect", "-f", &fmt, id])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout).trim().parse().ok()
}

// -------------------------------------------------------------------------
// MCP client (stdio) — mirrors the harness in integration_test.rs
// -------------------------------------------------------------------------

fn nlr_binary() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("target");
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
    fn spawn(nlr_root: &Path, neo4j_http_url: &str, neo4j_bolt_port: u16) -> Self {
        let binary = nlr_binary();
        assert!(
            binary.exists(),
            "neuro-link binary not found at {binary:?}. Run `cargo build` first."
        );

        let mut child = Command::new(&binary)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .env("NLR_ROOT", nlr_root.to_str().unwrap())
            .env("NEO4J_HTTP_URL", neo4j_http_url)
            .env("NEO4J_USER", "neo4j")
            .env("NEO4J_BOLT_PORT", neo4j_bolt_port.to_string())
            .spawn()
            .expect("failed to spawn nlr");

        let stdin = child.stdin.take().unwrap();
        let reader = BufReader::new(child.stdout.take().unwrap());
        let mut c = McpClient {
            child,
            stdin: Some(stdin),
            reader,
            next_id: 0,
        };
        let _ = c.send("initialize", None);
        c
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

    fn call_tool(&mut self, name: &str, arguments: Value) -> Value {
        self.send(
            "tools/call",
            Some(json!({"name": name, "arguments": arguments})),
        )
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        drop(self.stdin.take());
        let _ = self.child.wait();
    }
}

fn setup_nlr_root(tmp: &tempfile::TempDir) {
    let root = tmp.path();
    for d in ["00-raw", "01-sorted", "02-KB-main", "03-ontology-main/domain", "07-neuro-link-task", "config", "state", "secrets"] {
        std::fs::create_dir_all(root.join(d)).unwrap();
    }
    std::fs::write(root.join("CLAUDE.md"), "# nlr docker test scaffold\n").unwrap();
    std::fs::write(root.join("02-KB-main/schema.md"), "---\ntitle: Schema\n---\n").unwrap();
    std::fs::write(root.join("02-KB-main/index.md"), "# Index\n").unwrap();
    std::fs::write(root.join("config/neuro-link.md"), "---\nversion: 1\n---\n").unwrap();
    std::fs::write(
        root.join("state/heartbeat.json"),
        r#"{"status":"ok","last_check":"2026-01-01T00:00:00Z","errors":[]}"#,
    )
    .unwrap();
}

// -------------------------------------------------------------------------
// The test
// -------------------------------------------------------------------------

#[test]
fn graph_traverse_against_real_neo4j() {
    let container = Neo4jContainer::start();
    container.wait_for_ready(Duration::from_secs(60));
    container.seed_triples();

    let tmp = tempfile::tempdir().unwrap();
    setup_nlr_root(&tmp);

    let mut client = McpClient::spawn(
        tmp.path(),
        &container.http_url(),
        container.bolt_port,
    );

    // Multi-entity query to clear the intent gate.
    let resp = client.call_tool(
        "nlr_graph_traverse",
        json!({"query": "Attention and Transformer", "max_hops": 2}),
    );

    let text = resp["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("no text in response: {resp}"));
    let payload: Value = serde_json::from_str(text)
        .unwrap_or_else(|e| panic!("parse error: {e}\nraw: {text}"));

    assert_eq!(
        payload["triggered"], true,
        "intent gate should trigger on multi-entity query; payload: {payload}"
    );
    let paths = payload["paths"]
        .as_array()
        .unwrap_or_else(|| panic!("paths not an array: {payload}"));
    assert!(
        !paths.is_empty(),
        "expected at least one path from seeded graph, got: {payload}"
    );
}
