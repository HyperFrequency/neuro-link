# secrets/.env schema

All keys required unless marked optional. The setup skill populates missing
keys but never overwrites existing ones without `--force`.

## Authentication

```
# Shared bearer token for Obsidian plugin, Caddy auth proxy, TurboVault,
# and the neuro-link HTTP MCP server. Generate with: openssl rand -hex 32
NLR_API_TOKEN=<64-hex-chars>
```

This is the single most important secret. It's checked by Caddy before any
request reaches TurboVault, so a leaked token means public read/write on the
vault.

## External tunnel

```
# ngrok personal authtoken from https://dashboard.ngrok.com/get-started/your-authtoken
NGROK_AUTHTOKEN=<ngrok-token>

# Optional: reserved domain (paid tier). Omit for random URL each restart.
NGROK_DOMAIN=your-reserved-domain.ngrok.app
```

## Data stores

```
QDRANT_URL=http://localhost:6333
# No auth by default (localhost-only). If exposing Qdrant, add:
# QDRANT_API_KEY=<key>

NEO4J_URL=bolt://localhost:7687
NEO4J_AUTH=neo4j:<password>
```

## LLM providers

```
# HuggingFace — for model downloads and dataset access (ar5iv).
HF_TOKEN=hf_<token>

# OpenAI — for fallback LLM calls (e.g., when Claude is unavailable).
OPENAI_API_KEY=sk-...

# Anthropic — for primary LLM calls outside Claude Code.
ANTHROPIC_API_KEY=sk-ant-...
```

## Optional integrations

```
# InfraNodus — if using the hosted service instead of self-hosted.
INFRANODUS_API_KEY=<key>

# Augment Code / Auggie — if configured for semantic indexing.
AUGGIE_API_KEY=<key>

# Context7 — usually no auth required, but enterprise tier uses a token.
CONTEXT7_API_KEY=<key>
```

## arXiv bulk access

```
# AWS credentials for arXiv S3 requester-pays bucket. Cost: ~$0.09/GB egress.
# Only needed if ingesting arXiv papers that aren't in the ar5iv dataset.
AWS_ACCESS_KEY_ID=<key>
AWS_SECRET_ACCESS_KEY=<secret>
AWS_DEFAULT_REGION=us-east-1
```

## File permissions

Ensure `secrets/.env` is 0600:

```bash
chmod 600 secrets/.env
```

`.gitignore` already excludes `secrets/*`, but double-check with `git status`
before any commit.
