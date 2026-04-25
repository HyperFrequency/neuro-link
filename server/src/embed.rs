use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

/// Vault roots scanned by `embed_wiki`, in order.
///
/// The new `vaults/` directory is the canonical entry point as of
/// 2026-04-24 (per `vaults/README.md`). `02-KB-main/` remains in the list
/// for backward compatibility — existing tests and deployments that never
/// populated `vaults/` still work. Each root is walked independently; a
/// missing directory is a silent no-op, not an error.
///
/// Path semantics: when a file is found under root `R`, the payload
/// `path` field is `file.strip_prefix(R)`. So `vaults/papers/a.md`
/// becomes `papers/a.md`, and `02-KB-main/b.md` becomes `b.md`.
/// This disambiguates the source of each indexed entry.
pub const DEFAULT_VAULT_DIRS: &[&str] = &["vaults", "02-KB-main"];

/// Process-global cache of the search-time workspace ID. Gate-38 DDD2:
/// switched from OnceLock<Option<String>> to Mutex<Option<String>> so a
/// first-call MISS doesn't poison the process forever — we cache only
/// successful resolutions; misses leave the slot at None so a later
/// call after `nlr_rag_embed` creates the cache file can resolve and
/// populate. Once a Some lands, it stays for the process lifetime
/// (preserves CCC2's "no mid-process tenant shift" property for the
/// hot path while making first-miss recovery automatic).
static SEARCH_WORKSPACE_ID: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Return a stable workspace identifier. Resolution order:
///   1. NLR_WORKSPACE_ID env (wins; no fs side effects)
///   2. Cached `<root>/.nlr-workspace-id` file
///   3. Fresh UUIDv4 persisted via O_CREAT|O_EXCL to that file
///
/// Gate-34 ZZ2 + Gate-35 AAA2: returns Result so persistence failures
/// propagate; creation is ATOMIC via OpenOptions::create_new. Two
/// concurrent first-run callers race to create the file — one wins,
/// the other observes AlreadyExists and re-reads to converge on the
/// winning ID. Without this, racing embed_wiki calls could mint
/// distinct UUIDs and split one workspace across two Qdrant
/// namespaces.
pub fn resolve_workspace_id(root: &Path) -> Result<String> {
    if let Ok(env_id) = std::env::var("NLR_WORKSPACE_ID") {
        if !env_id.trim().is_empty() {
            return Ok(env_id.trim().to_string());
        }
    }
    let cache = root.join(".nlr-workspace-id");
    // Fast path: file already persisted, just read it.
    if let Ok(persisted) = fs::read_to_string(&cache) {
        let trimmed = persisted.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    // Slow path: write the UUID to a uniquely-named temp file (PID +
    // UUID-suffixed so concurrent processes don't collide on the temp
    // path), then `rename` it into the cache path. Gate-38 DDD1:
    // hard_link is unsupported on FAT/exFAT/SMB/many FUSE mounts;
    // rename is portable. Race semantics: rename overwrites silently,
    // so the final cache content is whichever rename landed last —
    // we then read-after-rename to discover the surviving UUID and
    // converge on it. Gate-38 DDD3: parent-directory fsync after the
    // rename + temp cleanup makes the new directory entry durable
    // across power loss / host crash.
    use std::io::Write;
    let new_id = uuid::Uuid::new_v4().to_string();
    let line = format!("{new_id}\n");
    let temp_suffix = format!("tmp-{}-{}", std::process::id(), uuid::Uuid::new_v4());
    let temp = cache.with_extension(temp_suffix);
    let temp_cleanup = || {
        let _ = fs::remove_file(&temp);
    };
    {
        let mut f = std::fs::File::create(&temp).with_context(|| {
            format!(
                "failed to create temp workspace id at {} — set NLR_WORKSPACE_ID explicitly or fix path writability",
                temp.display()
            )
        })?;
        if let Err(e) = f.write_all(line.as_bytes()).and_then(|_| f.sync_all()) {
            temp_cleanup();
            return Err(anyhow::Error::from(e).context(format!(
                "failed to write/sync workspace id to {}",
                temp.display()
            )));
        }
    }
    // Gate-39 EEE1: restore no-replace publication via hard_link.
    // rename overwrites silently and let two racing first-run callers
    // each return a different UUID even though the final cache held
    // only one — splitting one workspace across two namespaces. Hard
    // link is unsupported on a few oddball filesystems (FAT/exFAT,
    // some SMB/FUSE) but on those NLR_WORKSPACE_ID is the documented
    // explicit-set escape hatch.
    match std::fs::hard_link(&temp, &cache) {
        Ok(()) => {
            temp_cleanup();
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            // Another caller landed first. Their content was synced
            // before they linked, so the file is non-empty.
            temp_cleanup();
            let persisted = fs::read_to_string(&cache).with_context(|| {
                format!("workspace id file {} exists but unreadable", cache.display())
            })?;
            let trimmed = persisted.trim();
            if trimmed.is_empty() {
                anyhow::bail!(
                    "workspace id file {} exists but is empty (concurrent create may have stalled or interrupted; \
                     delete the empty file and re-run, or set NLR_WORKSPACE_ID explicitly)",
                    cache.display()
                );
            }
            return Ok(trimmed.to_string());
        }
        Err(e) => {
            temp_cleanup();
            return Err(anyhow::Error::from(e).context(format!(
                "failed to link workspace id into {} (filesystems without hard_link support \
                 such as FAT/exFAT or some SMB/FUSE mounts must set NLR_WORKSPACE_ID explicitly)",
                cache.display()
            )));
        }
    }
    // Gate-38 DDD3 (best-effort): durably persist the directory entry.
    // Documented as best-effort: on platforms where directory open-
    // for-write is denied (Windows, some sandboxes), this is a no-op.
    // File contents were sync_all'd above, so the window for losing
    // the new directory entry across crashes is narrow but non-zero.
    if let Some(parent) = cache.parent() {
        if let Ok(dir) = std::fs::File::open(parent) {
            let _ = dir.sync_all();
        }
    }
    Ok(new_id)
}

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

    let skip = ["schema.md", "index.md", "log.md"];
    let mut count = 0;

    // Walk each vault root in DEFAULT_VAULT_DIRS. A missing directory is a
    // silent no-op so we can ship the new `vaults/` convention without
    // breaking existing deployments that only have `02-KB-main/`. The
    // reverse is also true — a repo that has already moved everything to
    // `vaults/` doesn't need a placeholder `02-KB-main/`.
    // Gate-31 WW2 + Gate-32 XX1: dedupe by relative path across roots,
    // but only AFTER a successful upsert.
    // Gate-32 XX2 + Gate-33 YY2: workspace-scoped UUID v5 keyed off a
    // STABLE workspace identifier (NLR_WORKSPACE_ID env or persisted
    // <root>/.nlr-workspace-id file), NOT the canonical filesystem
    // path. Path-based namespaces broke retries across worktrees,
    // CI temp dirs, or moved checkouts — same logical repo got new
    // namespaces and re-embeds piled up duplicates. The stable ID is
    // cached in the file on first call; re-embeds in any path of the
    // same repo continue overwriting the same point IDs.
    let mut seen_rels: std::collections::HashSet<String> = std::collections::HashSet::new();
    let workspace_id = resolve_workspace_id(root)?;
    let workspace_ns =
        uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, workspace_id.as_bytes());
    for vault_name in DEFAULT_VAULT_DIRS {
        let vault_root = root.join(vault_name);
        if !vault_root.is_dir() {
            continue;
        }

        for entry in WalkDir::new(&vault_root).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.extension().is_some_and(|e| e == "md")
                || skip.iter().any(|s| path.file_name().is_some_and(|f| f == *s))
            {
                continue;
            }
            let rel = path
                .strip_prefix(&vault_root)
                .unwrap_or(path)
                .display()
                .to_string();
            if seen_rels.contains(&rel) {
                continue;  // already SUCCESSFULLY upserted from an earlier root
            }
            let content = fs::read_to_string(path).unwrap_or_default();

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

            // Only count upserts that Qdrant actually accepted; previously
            // the counter was bumped on any send() resolution, making
            // silent rejects invisible in the success log.
            // Gate-31 WW2 + Gate-32 XX2: deterministic UUID v5 keyed
            // off rel path WITHIN a workspace-scoped namespace. Same
            // workspace + same rel path → same point ID (re-embeds
            // overwrite). Different workspace + same rel path →
            // different point ID (no cross-workspace clobber on a
            // shared Qdrant deployment).
            let point_id = uuid::Uuid::new_v5(&workspace_ns, rel.as_bytes()).to_string();
            let upsert_url = format!("{qdrant_url}/collections/{collection}/points");
            match client
                .put(&upsert_url)
                .json(&serde_json::json!({
                    "points": [{
                        "id": point_id,
                        "vector": vector,
                        "payload": {
                            "path": rel,
                            "preview": &content[..content.len().min(500)],
                            "workspace_id": &workspace_id
                        }
                    }]
                }))
                .send()
                .await
            {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        count += 1;
                        // Gate-32 XX1: mark rel as seen ONLY after
                        // successful upsert. A failed vaults/x.md
                        // earlier in the loop (or upstream) leaves
                        // 02-KB-main/x.md eligible for retry from
                        // the legacy root.
                        seen_rels.insert(rel.clone());
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

    // Gate-33 YY1 + Gate-34 ZZ1: filter searches by workspace_id, and
    // resolve the ID from the SAME source embed_wiki uses — NLR_ROOT
    // env or NLR_WORKSPACE_ID env — not cwd. The MCP server commonly
    // runs from a different cwd than the repo root; cwd-based lookup
    // would silently fall through to an unfiltered query and leak
    // another tenant's documents on a shared Qdrant collection.
    // Resolution order:
    //   1. NLR_WORKSPACE_ID env
    //   2. <NLR_ROOT>/.nlr-workspace-id (NLR_ROOT must be set for the
    //      MCP server to find its vault — same source of truth here)
    // If neither resolves, NLR_ALLOW_UNFILTERED_SEARCH=1 is required
    // to fall through to unfiltered (single-workspace dev ergonomics).
    // Otherwise we fail closed instead of leaking cross-workspace.
    let mut search_body = serde_json::json!({
        "vector": vector,
        "limit": limit,
        "with_payload": true
    });
    // Gate-37 CCC2 + Gate-38 DDD2: cache workspace_id per process to
    // prevent mid-process tenant shift, but DON'T memoize a miss —
    // a first call before `.nlr-workspace-id` exists must not brick
    // every later call. Mutex-protected Option: read first; if None,
    // try to resolve; if resolution succeeds, cache and return; if
    // it fails, leave the slot at None so a later call (after embed
    // populated the cache) can resolve.
    let resolved_id = {
        let mut slot = SEARCH_WORKSPACE_ID.lock().unwrap();
        if let Some(ref id) = *slot {
            Some(id.clone())
        } else {
            let resolved = (|| -> Option<String> {
                if let Ok(env_id) = std::env::var("NLR_WORKSPACE_ID") {
                    if !env_id.trim().is_empty() {
                        return Some(env_id.trim().to_string());
                    }
                }
                if let Ok(nlr_root) = crate::config::resolve_nlr_root() {
                    let cache = nlr_root.join(".nlr-workspace-id");
                    if let Ok(persisted) = fs::read_to_string(&cache) {
                        let id = persisted.trim();
                        if !id.is_empty() {
                            return Some(id.to_string());
                        }
                    }
                }
                None
            })();
            if let Some(ref id) = resolved {
                *slot = Some(id.clone());
            }
            resolved
        }
    };
    if let Some(id) = resolved_id {
        search_body["filter"] = serde_json::json!({
            "must": [{
                "key": "workspace_id",
                "match": {"value": id}
            }]
        });
    } else if std::env::var("NLR_ALLOW_UNFILTERED_SEARCH").as_deref() != Ok("1") {
        anyhow::bail!(
            "search_wiki: no workspace_id resolvable (set NLR_WORKSPACE_ID or NLR_ROOT, \
             or NLR_ALLOW_UNFILTERED_SEARCH=1 to opt into unfiltered single-workspace mode)"
        );
    }
    let resp = client
        .post(format!("{qdrant_url}/collections/nlr_wiki/points/search"))
        .json(&search_body)
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
