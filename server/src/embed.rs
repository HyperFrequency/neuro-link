use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResult {
    pub path: String,
    pub score: f64,
    pub preview: String,
}

fn resolve_embedding_config(root: &Path) -> (String, String, usize) {
    // Try to read from config/neuro-link.md frontmatter
    let config_path = root.join("config/neuro-link.md");
    let mut model = String::new();
    let mut dims: usize = 0;

    if let Ok(content) = fs::read_to_string(&config_path) {
        for line in content.lines() {
            if line.starts_with("embedding_model:") {
                model = line.split(':').nth(1).unwrap_or("").trim().to_string();
            }
            if line.starts_with("embedding_dims:") {
                dims = line.split(':').nth(1).unwrap_or("0").trim().parse().unwrap_or(0);
            }
        }
    }

    // Env var overrides
    if let Ok(env_model) = std::env::var("EMBEDDING_MODEL") {
        model = env_model;
    }

    // Defaults — Octen/Octen-Embedding-8B unquantized, 4096 dimensions
    // Served locally via scripts/embedding-server.py on port 8400
    let url = std::env::var("EMBEDDING_API_URL")
        .unwrap_or_else(|_| "http://localhost:8400/v1/embeddings".into());
    if model.is_empty() {
        model = "Octen/Octen-Embedding-8B".to_string();
    }
    if dims == 0 {
        dims = 4096;
    }

    (url, model, dims)
}

/// Pre-flight: verify the llama-server (or configured embedding backend) is
/// reachable before we silently no-op through a pile of pages. Returns an
/// actionable anyhow error when the `/v1/models` probe fails, so CI / the
/// operator sees a loud failure instead of a green "Embedded 0 pages" log.
pub(crate) async fn preflight_embedding_backend(
    client: &reqwest::Client,
    embedding_url: &str,
) -> Result<()> {
    // Derive the `/v1/models` probe URL from the configured embeddings URL.
    // Typical value: "http://localhost:8400/v1/embeddings" → "http://localhost:8400/v1/models".
    let probe_url = if let Some(idx) = embedding_url.rfind("/v1/") {
        let base = &embedding_url[..idx];
        format!("{base}/v1/models")
    } else {
        // Fall back to appending /v1/models on whatever base the user configured.
        let trimmed = embedding_url.trim_end_matches('/');
        format!("{trimmed}/v1/models")
    };

    match client
        .get(&probe_url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => Ok(()),
        Ok(resp) => Err(anyhow::anyhow!(
            "embed: llama-server at {probe_url} returned HTTP {} — start it or set EMBEDDING_API_URL env var",
            resp.status()
        )),
        Err(e) => Err(anyhow::anyhow!(
            "embed: llama-server at {probe_url} unreachable ({e}) — start it or set EMBEDDING_API_URL env var"
        )),
    }
}

/// Codex finding #6: pre-flight that the Qdrant collection exists and its
/// configured vector size matches the embedding dimension we're about to
/// write. Prior behavior silently accepted a mismatched / missing collection
/// and reported `count == N` even though every upsert was failing.
///
/// Returns `Ok(())` when the collection exists *and* its `vectors.size` (or
/// any named-vector entry's `size`) equals `expected_dims`. Returns a loud
/// error with the URL + body otherwise.
async fn preflight_qdrant_collection(
    client: &reqwest::Client,
    qdrant_url: &str,
    collection: &str,
    expected_dims: usize,
) -> Result<()> {
    let url = format!("{qdrant_url}/collections/{collection}");
    let resp = client
        .get(&url)
        .send()
        .await
        .with_context(|| format!("Qdrant collection preflight GET {url} failed"))?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!(
            "Qdrant collection '{collection}' not available (HTTP {status} from {url}): {body}"
        );
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .with_context(|| format!("Qdrant collection preflight: non-JSON response from {url}"))?;

    // Qdrant returns either a single unnamed vector config (`result.config.params.vectors.size`)
    // or a map of named vector configs (`result.config.params.vectors.<name>.size`). Accept
    // any configuration whose declared size matches — fail loudly otherwise.
    let vectors = &body["result"]["config"]["params"]["vectors"];
    let actual_dim = if let Some(size) = vectors.get("size").and_then(|v| v.as_u64()) {
        Some(size as usize)
    } else if let Some(map) = vectors.as_object() {
        map.values()
            .find_map(|v| v.get("size").and_then(|s| s.as_u64()))
            .map(|s| s as usize)
    } else {
        None
    };

    match actual_dim {
        Some(dim) if dim == expected_dims => Ok(()),
        Some(dim) => anyhow::bail!(
            "Qdrant collection '{collection}' has vector size {dim}, but embedder produces {expected_dims}-dim vectors — recreate the collection or reconfigure the embedder"
        ),
        None => anyhow::bail!(
            "Qdrant collection '{collection}' exists but its vector config is unreadable: {body}"
        ),
    }
}

pub async fn embed_wiki(root: &Path, qdrant_url: &str, recreate: bool) -> Result<usize> {
    let collection = "nlr_wiki";
    let client = reqwest::Client::new();
    let (embedding_url, embedding_model, embedding_dims) = resolve_embedding_config(root);

    // P06: fail loudly if the embedding backend is unreachable instead of
    // silently exiting with success after embedding 0 pages.
    preflight_embedding_backend(&client, &embedding_url).await?;

    if recreate {
        let _ = client
            .delete(format!("{qdrant_url}/collections/{collection}"))
            .send()
            .await;
        client
            .put(format!("{qdrant_url}/collections/{collection}"))
            .json(&serde_json::json!({
                "vectors": { "size": embedding_dims, "distance": "Cosine" }
            }))
            .send()
            .await
            .context("Failed to create Qdrant collection")?
            .error_for_status()
            .context("Qdrant rejected collection-create request")?;
    }

    preflight_qdrant_collection(&client, qdrant_url, collection, embedding_dims).await?;

    let kb = root.join("02-KB-main");
    let skip = ["schema.md", "index.md", "log.md"];
    let mut count = 0;

    for entry in WalkDir::new(&kb).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.extension().is_some_and(|e| e == "md")
            || skip.iter().any(|s| path.file_name().is_some_and(|f| f == *s))
        {
            continue;
        }
        let content = fs::read_to_string(path).unwrap_or_default();
        let rel = path.strip_prefix(&kb).unwrap_or(path).display().to_string();

        let embed_resp = client
            .post(&embedding_url)
            .json(&serde_json::json!({
                "model": embedding_model,
                "input": &content[..content.len().min(8000)]
            }))
            .send()
            .await;

        let vector = match embed_resp {
            Ok(resp) => {
                let body: serde_json::Value = resp.json().await.unwrap_or_default();
                body["data"][0]["embedding"]
                    .as_array()
                    .map(|a| a.iter().filter_map(|v| v.as_f64()).collect::<Vec<_>>())
                    .unwrap_or_default()
            }
            Err(_) => continue,
        };

        if vector.is_empty() {
            continue;
        }

        // Only count upserts that Qdrant actually accepted; previously the
        // counter was bumped on any send() resolution, making silent
        // rejects invisible in the success log.
        let point_id = uuid::Uuid::new_v4().to_string();
        let upsert_url = format!("{qdrant_url}/collections/{collection}/points");
        match client
            .put(&upsert_url)
            .json(&serde_json::json!({
                "points": [{
                    "id": point_id,
                    "vector": vector,
                    "payload": { "path": rel, "preview": &content[..content.len().min(500)] }
                }]
            }))
            .send()
            .await
        {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    count += 1;
                } else {
                    let body = resp.text().await.unwrap_or_default();
                    tracing::warn!(
                        "Qdrant upsert for {rel} failed: HTTP {status} from {upsert_url}: {body}"
                    );
                }
            }
            Err(err) => {
                tracing::warn!("Qdrant upsert for {rel} errored: {err}");
            }
        }
    }

    Ok(count)
}

pub async fn search_wiki(
    query: &str,
    qdrant_url: &str,
    limit: usize,
) -> Result<Vec<SearchResult>> {
    let client = reqwest::Client::new();
    let embedding_url = std::env::var("EMBEDDING_API_URL")
        .unwrap_or_else(|_| "http://localhost:8400/v1/embeddings".into());
    let embedding_model = std::env::var("EMBEDDING_MODEL")
        .unwrap_or_else(|_| "Octen/Octen-Embedding-8B".into());

    let embed_resp = client
        .post(&embedding_url)
        .json(&serde_json::json!({
            "model": embedding_model,
            "input": query
        }))
        .send()
        .await
        .context("Failed to get query embedding")?;

    let embed_body: serde_json::Value = embed_resp.json().await?;
    let vector: Vec<f64> = embed_body["data"][0]["embedding"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_f64()).collect())
        .unwrap_or_default();

    if vector.is_empty() {
        anyhow::bail!("Empty embedding vector");
    }

    let resp = client
        .post(format!("{qdrant_url}/collections/nlr_wiki/points/search"))
        .json(&serde_json::json!({
            "vector": vector,
            "limit": limit,
            "with_payload": true
        }))
        .send()
        .await
        .context("Qdrant search failed")?;

    let body: serde_json::Value = resp.json().await?;
    let results = body["result"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|r| SearchResult {
                    path: r["payload"]["path"].as_str().unwrap_or("").to_string(),
                    score: r["score"].as_f64().unwrap_or(0.0),
                    preview: r["payload"]["preview"].as_str().unwrap_or("").to_string(),
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(results)
}

// Codex finding #6 regression tests: prove that `embed_wiki` now surfaces
// Qdrant failures instead of silently reporting success.
//
// Three wiremock cases, as specified in the Codex report:
//   (a) missing collection          → GET /collections returns 404 → Err
//   (b) wrong vector dimension      → GET ok but size != embedder dim → Err
//   (c) happy path                  → GET ok + PUT /points ok → count == N
//
// `resolve_embedding_config` reads EMBEDDING_API_URL / EMBEDDING_MODEL from
// the process env. Tests that mutate those must be serialized — cargo runs
// tests in parallel threads within a process, so a shared Mutex guards each
// body.
#[cfg(test)]
mod qdrant_upsert_tests {
    use super::*;
    use std::sync::Mutex;
    use wiremock::matchers::{method, path, path_regex};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    // Serialize tests that mutate process-global env vars.
    static ENV_GUARD: Mutex<()> = Mutex::new(());

    const EMBED_DIM: usize = 4096;

    fn make_vault(tmp: &tempfile::TempDir, num_pages: usize) -> std::path::PathBuf {
        let root = tmp.path().to_path_buf();
        let kb = root.join("02-KB-main");
        std::fs::create_dir_all(&kb).unwrap();
        // Minimal config so resolve_embedding_config sees our dims (env vars
        // only override model/url, not dims).
        std::fs::create_dir_all(root.join("config")).unwrap();
        std::fs::write(
            root.join("config/neuro-link.md"),
            format!(
                "---\nembedding_model: test-model\nembedding_dims: {EMBED_DIM}\n---\n# config\n"
            ),
        )
        .unwrap();
        for i in 0..num_pages {
            std::fs::write(kb.join(format!("page-{i}.md")), format!("# Page {i}\n\nbody")).unwrap();
        }
        root
    }

    async fn mount_embedding_endpoints(server: &MockServer, dims: usize) {
        // /v1/models for preflight.
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{"id": "test-model"}]
            })))
            .mount(server)
            .await;
        // /v1/embeddings returns a `dims`-length vector for every request.
        let vec: Vec<f64> = vec![0.1; dims];
        Mock::given(method("POST"))
            .and(path("/v1/embeddings"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{"embedding": vec}]
            })))
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn missing_collection_returns_err() {
        let _guard = ENV_GUARD.lock().unwrap();
        let server = MockServer::start().await;
        mount_embedding_endpoints(&server, EMBED_DIM).await;

        // GET /collections/nlr_wiki → 404
        Mock::given(method("GET"))
            .and(path("/collections/nlr_wiki"))
            .respond_with(ResponseTemplate::new(404).set_body_string("Not found"))
            .mount(&server)
            .await;

        let tmp = tempfile::tempdir().unwrap();
        let root = make_vault(&tmp, 2);
        std::env::set_var("EMBEDDING_API_URL", format!("{}/v1/embeddings", server.uri()));
        std::env::set_var("EMBEDDING_MODEL", "test-model");

        let result = embed_wiki(&root, &server.uri(), false).await;
        assert!(
            result.is_err(),
            "expected embed_wiki to return Err when collection is missing, got {:?}",
            result
        );
        let msg = format!("{:#}", result.err().unwrap());
        assert!(msg.contains("nlr_wiki"), "error should mention collection: {msg}");
        assert!(msg.contains("404"), "error should mention HTTP status: {msg}");
    }

    #[tokio::test]
    async fn wrong_dimension_returns_err() {
        let _guard = ENV_GUARD.lock().unwrap();
        let server = MockServer::start().await;
        mount_embedding_endpoints(&server, EMBED_DIM).await;

        // GET /collections/nlr_wiki returns a collection with size=1024
        // (mismatch vs our 4096-dim embedder).
        Mock::given(method("GET"))
            .and(path("/collections/nlr_wiki"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "result": {
                    "config": {
                        "params": {
                            "vectors": { "size": 1024, "distance": "Cosine" }
                        }
                    }
                }
            })))
            .mount(&server)
            .await;

        let tmp = tempfile::tempdir().unwrap();
        let root = make_vault(&tmp, 2);
        std::env::set_var("EMBEDDING_API_URL", format!("{}/v1/embeddings", server.uri()));
        std::env::set_var("EMBEDDING_MODEL", "test-model");

        let result = embed_wiki(&root, &server.uri(), false).await;
        assert!(
            result.is_err(),
            "expected Err when collection dim mismatches embedder; got {:?}",
            result
        );
        let msg = format!("{:#}", result.err().unwrap());
        assert!(msg.contains("1024"), "error should mention actual dim: {msg}");
        assert!(msg.contains("4096"), "error should mention expected dim: {msg}");
    }

    #[tokio::test]
    async fn happy_path_counts_successful_upserts() {
        let _guard = ENV_GUARD.lock().unwrap();
        let server = MockServer::start().await;
        mount_embedding_endpoints(&server, EMBED_DIM).await;

        // GET /collections/nlr_wiki → matching dim.
        Mock::given(method("GET"))
            .and(path("/collections/nlr_wiki"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "result": {
                    "config": {
                        "params": {
                            "vectors": { "size": EMBED_DIM, "distance": "Cosine" }
                        }
                    }
                }
            })))
            .mount(&server)
            .await;

        // PUT /collections/nlr_wiki/points → 200 for each page.
        Mock::given(method("PUT"))
            .and(path_regex(r"^/collections/nlr_wiki/points$"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "result": { "operation_id": 0, "status": "completed" },
                "status": "ok",
                "time": 0.0
            })))
            .mount(&server)
            .await;

        let num_pages = 3;
        let tmp = tempfile::tempdir().unwrap();
        let root = make_vault(&tmp, num_pages);
        std::env::set_var("EMBEDDING_API_URL", format!("{}/v1/embeddings", server.uri()));
        std::env::set_var("EMBEDDING_MODEL", "test-model");

        let count = embed_wiki(&root, &server.uri(), false).await.expect("happy path");
        assert_eq!(count, num_pages, "all {num_pages} pages should count as embedded");
    }
}
