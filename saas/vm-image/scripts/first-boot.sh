#!/bin/sh
# One-shot initialization on first VM boot for a given user.
#
# Expected secret files (written by provisioner via `fly secrets set`):
#   /secrets/api-token          -- MCP bearer token for this user
#   /secrets/cloudflared-token  -- Cloudflare Tunnel token for this user's subdomain
#   /secrets/model-url          -- (optional) signed URL to download the GGUF
#
# Volumes expected (attached by Fly Machines API):
#   /data    -- neuro-link tree (00-raw, 01-sorted, 02-KB-main, ...)
#   /state   -- qdrant + neo4j storage
#   /models  -- GGUF models (may be empty on first boot)
set -eu

log() { echo "[first-boot] $*"; }

# -----------------------------------------------------------------------------
# 1. Ensure /data scaffolding exists. neuro-link init is idempotent.
# -----------------------------------------------------------------------------
if [ ! -f /data/config/neuro-link.md ]; then
    log "Initializing /data with neuro-link init"
    mkdir -p /data
    # `neuro-link init` creates the standard directory tree + default config.
    NLR_ROOT=/data /usr/local/bin/neuro-link init --root /data
fi

# -----------------------------------------------------------------------------
# 2. Fetch Octen GGUF if not already on volume.
# -----------------------------------------------------------------------------
MODEL_FILE=/models/Octen-Embedding-8B.f16.gguf
if [ ! -f "$MODEL_FILE" ]; then
    if [ -f /secrets/model-url ]; then
        MODEL_URL="$(cat /secrets/model-url)"
        log "Fetching Octen GGUF from signed URL"
        curl -fL --retry 3 -o "$MODEL_FILE" "$MODEL_URL"
    else
        log "WARNING: no model on volume and no /secrets/model-url. llama-server will fail."
    fi
fi

# -----------------------------------------------------------------------------
# 3. Neo4j one-time password reset (community edition ships with a force-set flow).
# -----------------------------------------------------------------------------
NEO4J_INIT_MARKER=/state/neo4j/.initialized
if [ ! -f "$NEO4J_INIT_MARKER" ]; then
    log "Initializing Neo4j data dir"
    mkdir -p /state/neo4j/data /state/neo4j/run
    # NEO4J_AUTH env is consumed by neo4j on first start; nothing else to do.
    touch "$NEO4J_INIT_MARKER"
fi

# -----------------------------------------------------------------------------
# 4. Sanity-check required secrets.
# -----------------------------------------------------------------------------
for f in /secrets/api-token /secrets/cloudflared-token; do
    if [ ! -s "$f" ]; then
        log "FATAL: required secret $f is missing or empty"
        exit 1
    fi
done

log "First-boot complete"
