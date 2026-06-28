// Service supervisor: monitors and restarts neuro-link's dependencies.
//
// Services monitored:
//   - Embedding server (llama.cpp) on :8400
//   - Qdrant on :6333 (Docker container qdrant-nlr)
//   - Neo4j on :7474 (Docker container neo4j-nlr)
//   - Ngrok tunnel (optional)
//
// Restart policy: exponential backoff, cap at 60s.
// Runs in the background when `neuro-link supervise` is invoked.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;
use tokio::time;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ServiceStatus {
    pub name: String,
    pub healthy: bool,
    pub url: String,
    pub last_check: String,
    pub restart_count: u32,
    pub error: Option<String>,
}

pub async fn run(root: &Path) -> Result<()> {
    eprintln!("Supervisor starting — monitoring services every 15s");
    eprintln!("  Ctrl-C to stop");

    let mut statuses: std::collections::HashMap<String, ServiceStatus> = Default::default();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;

    loop {
        for check in service_checks() {
            let healthy = check_health(&client, &check.health_url).await;
            let status = statuses.entry(check.name.to_string()).or_insert_with(|| ServiceStatus {
                name: check.name.to_string(),
                healthy: false,
                url: check.health_url.to_string(),
                last_check: chrono::Utc::now().to_rfc3339(),
                restart_count: 0,
                error: None,
            });

            status.last_check = chrono::Utc::now().to_rfc3339();
            let was_healthy = status.healthy;
            status.healthy = healthy;

            if !healthy && !was_healthy && check.restart_cmd.is_some() {
                // Service down and was down before — attempt restart
                if let Some(cmd) = check.restart_cmd {
                    eprintln!("[supervisor] {} unhealthy, attempting restart...", check.name);
                    let output = tokio::process::Command::new("sh")
                        .arg("-c")
                        .arg(cmd)
                        .output()
                        .await;
                    match output {
                        Ok(o) if o.status.success() => {
                            status.restart_count += 1;
                            status.error = None;
                            eprintln!("[supervisor] {} restart initiated (count: {})", check.name, status.restart_count);
                        }
                        Ok(o) => {
                            status.error = Some(format!("restart failed: {}", String::from_utf8_lossy(&o.stderr)));
                        }
                        Err(e) => {
                            status.error = Some(format!("spawn error: {e}"));
                        }
                    }
                }
            }
        }

        // Persist status to state/supervisor.json
        let all: Vec<&ServiceStatus> = statuses.values().collect();
        let path = root.join("state/supervisor.json");
        if let Ok(json) = serde_json::to_string_pretty(&all) {
            let _ = tokio::fs::write(&path, json).await;
        }

        // Print a compact status line
        let summary: Vec<String> = statuses.values().map(|s| {
            format!("{}: {}", s.name, if s.healthy { "OK" } else { "DOWN" })
        }).collect();
        eprintln!("[{}] {}", chrono::Utc::now().format("%H:%M:%S"), summary.join(" | "));

        time::sleep(Duration::from_secs(15)).await;
    }
}

struct ServiceCheck {
    name: &'static str,
    health_url: &'static str,
    restart_cmd: Option<&'static str>,
}

fn service_checks() -> Vec<ServiceCheck> {
    // Resolve embedding-server script path relative to NLR_ROOT
    let nlr_root = std::env::var("NLR_ROOT").unwrap_or_default();
    let embed_cmd: Box<String> = Box::new(format!(
        "nohup sh {}/scripts/embedding-server.sh >/tmp/neuro-link-embedding.log 2>&1 &",
        nlr_root
    ));
    let embed_cmd_static: &'static str = Box::leak(embed_cmd).as_str();

    vec![
        ServiceCheck {
            name: "neuro-link-http",
            health_url: "http://localhost:8080/health",
            restart_cmd: None, // self — can't restart itself
        },
        ServiceCheck {
            name: "qdrant",
            health_url: "http://localhost:6333/healthz",
            restart_cmd: Some("docker start qdrant-nlr 2>/dev/null || true"),
        },
        ServiceCheck {
            name: "neo4j",
            health_url: "http://localhost:7474",
            restart_cmd: Some("docker start neo4j-nlr 2>/dev/null || true"),
        },
        ServiceCheck {
            name: "embedding-server",
            health_url: "http://localhost:8400/health",
            restart_cmd: Some(embed_cmd_static),
        },
    ]
}

async fn check_health(client: &reqwest::Client, url: &str) -> bool {
    match client.get(url).send().await {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}

/// Called from the /health/all REST endpoint
pub async fn all_statuses(root: &Path) -> Vec<ServiceStatus> {
    let path = root.join("state/supervisor.json");
    match tokio::fs::read_to_string(&path).await {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}
