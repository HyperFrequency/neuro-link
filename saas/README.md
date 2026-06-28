# saas/ — Phase G scaffolding

SaaS architecture for **neuro-link-recursive.io**. Every paying user gets a
dedicated Fly.io VM running the full neuro-link stack, reachable at
`<slug>.neuro-link-recursive.io` via Cloudflare Tunnel.

**Status:** scaffolding + research only. Gated on open questions in
[`RESEARCH.md`](./RESEARCH.md). Not a working deployment.

## Layout

```
saas/
  RESEARCH.md           research memo (Obsidian ToS, cost model, open questions)
  web/                  Next.js 15 marketing + dashboard (scaffold only; no npm install)
  vm-image/             Dockerfile for per-user VMs (neuro-link + Qdrant + Neo4j + llama + cloudflared + Caddy)
  provisioner/          Fly.io + Cloudflare API client called by the Stripe webhook
```

## High-level flow

1. User signs up (Clerk) and picks a tier.
2. Stripe Checkout completes → webhook → `POST /api/provision` with tier + slug.
3. Provisioner calls Cloudflare (tunnel + DNS) then Fly (volumes + machine).
4. VM boots, pulls GGUF, starts all services via supervisord.
5. Dashboard displays the MCP URL + API token for the user.

## What is NOT in this scaffold

- Working Clerk/Stripe wiring (stubs + TODOs only).
- Postgres schema or DB driver (in-memory stub in `web/lib/db-stub.ts`).
- CI, infra-as-code (Terraform/Pulumi) for the Fly app itself.
- Teardown on subscription cancel (see open items in `provisioner/README.md`).
- Health endpoint on `neuro-link serve` (see `RESEARCH.md` §4.5).

## What IS in this scaffold

- Full Dockerfile for the per-user VM with supervisord/Caddy/cloudflared.
- Typed, SDK-free TypeScript provisioner that hits the real Fly + Cloudflare
  endpoints.
- Next.js App Router page tree with landing, signup, dashboard, Stripe webhook,
  and internal provision endpoint.
- Research memo with cost model, licensing risk, and the blocking open
  questions that must be answered before public launch.
