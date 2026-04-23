---
title: neuro-link-recursive — overview
last_updated: 2026-04-20
---

# neuro-link-recursive — overview

neuro-link-recursive is the unified context / memory / behavior
control-plane that hosts the rest of the HyperFrequency agent stack.
It's a Rust server (`server/target/release/neuro-link`) that fronts a
**hybrid retrieval pipeline** (qmd: BM25 + dense vector + Qwen-based
reranker + Qwen-based query expansion), a **Qdrant** vector database
(4096-dim cosine collections for the llm-wiki, math symbols, and per-
tool RefGraph nodes), a **Neo4j** graph database (reasoning ontologies
and RefGraph structural edges), and a **vault** of markdown files at
`/Users/DanBot/Dev/neuro-link/` organized into ten numbered directories
from raw ingest (`01-raw/`) through LLM-synthesized wiki pages
(`02-KB-main/`) through ontologies (`03-Ontology-main/`) through
human-in-the-loop review queues (`05-insights-HITL/`) through recursive
self-improvement reports (`06-Recursive/`).

Agents interact via two **MCP namespaces**: `nlr_*` (internal,
schema-enforced, the canonical write path for `02-KB-main/`) and `tv_*`
(public-facing TurboVault HTTP server proxied through Caddy + ngrok +
bearer auth, exposing 47 generic vault-operation tools). The `nlr_*`
path is the safe default for any LLM touching the llm-wiki; `tv_*` is
reserved for link-graph / centrality / vault-health queries that don't
mutate schema-bound content.

The system is **recursive** in the sense that the vault includes its
own self-improvement proposals under `06-Recursive/` and
`07-self-improvement-HITL/`; approved improvements feed back into the
install package, hook configuration, and skill definitions so the
agent's own behavior evolves with use. Install is via a 15-step
`install.sh` that provisions Qdrant + Neo4j + llama-server (hosting the
Octen-Embedding-8B f16 model on port 8400) + the three qmd GGUFs via
`huggingface-cli`, plus the nlr-extension steps (pinelsp + Serena Pine
mod) for trading-strategy-specific language support.
