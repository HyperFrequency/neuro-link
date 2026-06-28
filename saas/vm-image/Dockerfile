# =============================================================================
# neuro-link-recursive per-user VM image
# =============================================================================
# Single container that bundles:
#   - neuro-link binary (Rust)
#   - llama.cpp server + Octen-Embedding GGUF (mounted as volume in prod)
#   - Qdrant (vector DB)
#   - Neo4j (graph DB)
#   - cloudflared (Cloudflare Tunnel daemon)
#   - Caddy (TLS reverse proxy fronting MCP + dashboard)
#   - supervisord (process manager)
#
# Built as a single "fat" image for Fly.io Machines deploys. For size, Octen
# GGUF (~15GB) is NOT baked in — it is pulled at first boot to a Fly Volume
# mounted at /models. See scripts/first-boot.sh.
#
# Target size target: ~3GB before models.
# =============================================================================

ARG NEURO_LINK_VERSION=dev

# -----------------------------------------------------------------------------
# Stage 1: Build neuro-link (Rust) — reused from server/Dockerfile pattern
# -----------------------------------------------------------------------------
FROM rust:1.82-slim AS rust-builder
WORKDIR /build
COPY server/Cargo.toml server/Cargo.lock* ./
COPY server/src/ src/
RUN cargo build --release

# -----------------------------------------------------------------------------
# Stage 2: Build llama.cpp server
# -----------------------------------------------------------------------------
FROM debian:bookworm-slim AS llama-builder
RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential cmake git ca-certificates curl pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
RUN git clone --depth 1 --branch b4000 https://github.com/ggerganov/llama.cpp.git
WORKDIR /src/llama.cpp
RUN cmake -B build -DLLAMA_CURL=OFF -DGGML_NATIVE=OFF \
    && cmake --build build --config Release -j --target llama-server

# -----------------------------------------------------------------------------
# Stage 3: Runtime
# -----------------------------------------------------------------------------
FROM debian:bookworm-slim

ARG NEURO_LINK_VERSION
ENV NEURO_LINK_VERSION=${NEURO_LINK_VERSION} \
    DEBIAN_FRONTEND=noninteractive \
    NLR_ROOT=/data \
    QDRANT_URL=http://127.0.0.1:6333 \
    NEO4J_HTTP_URL=http://127.0.0.1:7474 \
    LLAMA_URL=http://127.0.0.1:8081 \
    PATH=/usr/local/bin:/usr/local/sbin:/usr/bin:/usr/sbin:/bin:/sbin

# ---- System deps ------------------------------------------------------------
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates curl wget gnupg supervisor tini \
        openjdk-17-jre-headless \
        libgomp1 libstdc++6 \
    && rm -rf /var/lib/apt/lists/*

# ---- cloudflared ------------------------------------------------------------
RUN curl -fsSL \
      https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-linux-amd64 \
      -o /usr/local/bin/cloudflared \
    && chmod +x /usr/local/bin/cloudflared

# ---- Caddy ------------------------------------------------------------------
RUN curl -fsSL "https://caddyserver.com/api/download?os=linux&arch=amd64" \
      -o /usr/local/bin/caddy \
    && chmod +x /usr/local/bin/caddy

# ---- Qdrant (prebuilt static) ----------------------------------------------
ARG QDRANT_VERSION=v1.12.4
RUN curl -fsSL \
      "https://github.com/qdrant/qdrant/releases/download/${QDRANT_VERSION}/qdrant-x86_64-unknown-linux-gnu.tar.gz" \
      | tar -xz -C /usr/local/bin qdrant \
    && chmod +x /usr/local/bin/qdrant
COPY saas/vm-image/qdrant-config.yaml /etc/qdrant/config.yaml

# ---- Neo4j ------------------------------------------------------------------
ARG NEO4J_VERSION=5.24.0
RUN curl -fsSL \
      "https://dist.neo4j.org/neo4j-community-${NEO4J_VERSION}-unix.tar.gz" \
      | tar -xz -C /opt \
    && ln -s /opt/neo4j-community-${NEO4J_VERSION} /opt/neo4j
ENV NEO4J_HOME=/opt/neo4j
ENV PATH=${NEO4J_HOME}/bin:${PATH}

# ---- llama.cpp --------------------------------------------------------------
COPY --from=llama-builder /src/llama.cpp/build/bin/llama-server /usr/local/bin/llama-server

# ---- neuro-link binary ------------------------------------------------------
COPY --from=rust-builder /build/target/release/neuro-link /usr/local/bin/neuro-link

# ---- Supervisor + entrypoint ------------------------------------------------
COPY saas/vm-image/supervisor/supervisord.conf /etc/supervisor/supervisord.conf
COPY saas/vm-image/supervisor/programs/ /etc/supervisor/conf.d/
COPY saas/vm-image/caddy/Caddyfile /etc/caddy/Caddyfile
COPY saas/vm-image/scripts/entrypoint.sh /usr/local/bin/entrypoint.sh
COPY saas/vm-image/scripts/first-boot.sh /usr/local/bin/first-boot.sh
RUN chmod +x /usr/local/bin/entrypoint.sh /usr/local/bin/first-boot.sh

# ---- Data layout ------------------------------------------------------------
# /data    -> neuro-link tree (Fly Volume, per-user persistent)
# /models  -> GGUFs (Fly Volume, possibly shared read-only)
# /state   -> qdrant + neo4j storage (Fly Volume, per-user)
# /secrets -> Fly secrets mounted at runtime (cloudflared token, stripe, etc.)
RUN mkdir -p /data /models /state/qdrant /state/neo4j /secrets /var/log/neurolink

# Neo4j expects its dirs writable; we will re-point via env at runtime.
ENV NEO4J_server_directories_data=/state/neo4j/data \
    NEO4J_server_directories_logs=/var/log/neurolink \
    NEO4J_server_directories_run=/state/neo4j/run \
    NEO4J_AUTH=neo4j/neurolink1234

EXPOSE 443 80 8080
VOLUME ["/data", "/state", "/models"]

ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/entrypoint.sh"]
CMD ["/usr/bin/supervisord", "-c", "/etc/supervisor/supervisord.conf"]
