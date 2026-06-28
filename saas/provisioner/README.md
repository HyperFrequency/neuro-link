# Provisioner

Creates a dedicated Fly.io Machine + Cloudflare Tunnel subdomain for every paying user.

## Flow

```
Stripe checkout complete
       |
       v
/app/api/stripe-webhook (Next.js)
       |  customer_id, tier
       v
POST /app/api/provision  (internal)
       |
       +--> 1. mint api_token (random 32 bytes, base64url)
       +--> 2. Cloudflare API: create tunnel -> get tunnel_token
       +--> 3. Cloudflare API: create DNS CNAME  <slug>.neuro-link-recursive.io
       +--> 4. Fly API: create volume  nlr_data_<slug>
       +--> 5. Fly API: create volume  nlr_state_<slug>
       +--> 6. Fly API: create volume  nlr_models_<slug>  (or attach shared RO)
       +--> 7. Fly API: fly secrets set api-token, cloudflared-token, model-url
       +--> 8. Fly API: create machine from nlr-vm:<version> image
       +--> 9. persist row in users table:
                 user_id, stripe_customer, slug, fly_machine_id,
                 cf_tunnel_id, tier, mcp_url, api_token
```

## Resource tiers

| Tier | Price | CPU | RAM | VM size (Fly preset) | /data | /state | /models | Notes |
|------|-------|-----|-----|----------------------|-------|--------|---------|-------|
| Hobby | $19/mo | 1 vCPU shared | 2 GB | `shared-cpu-1x` | 10 GB | 5 GB | shared RO 20 GB | Single-user, no team sharing. |
| Pro | $49/mo | 2 vCPU shared | 4 GB | `shared-cpu-2x` | 50 GB | 20 GB | shared RO 20 GB | Suitable for heavy Obsidian vaults. |
| Team | $129/mo | 4 vCPU performance | 8 GB | `performance-2x` | 200 GB | 50 GB | dedicated 20 GB | Up to 5 seats (Clerk orgs). |
| Enterprise | $299/mo | 8 vCPU performance | 16 GB | `performance-4x` | 1 TB | 100 GB | dedicated 20 GB | SSO, private Cloudflare Access, custom domain. |

Volume sizes are ceilings enforced by the provisioner. Upgrades trigger a
`fly volumes extend` + `fly machine update` under the hood.

## Fly.io Machines API

All calls hit `https://api.machines.dev/v1/apps/${APP_NAME}/...` with header
`Authorization: Bearer $FLY_API_TOKEN`. The "app" is a single multi-tenant
Fly app (e.g. `neuro-link-saas`); each user gets a dedicated **machine**
inside that app.

### 1. Create data volume

```bash
curl -X POST "https://api.machines.dev/v1/apps/${APP_NAME}/volumes" \
  -H "Authorization: Bearer ${FLY_API_TOKEN}" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "nlr_data_'"${SLUG}"'",
    "region": "iad",
    "size_gb": 50,
    "fstype": "ext4",
    "encrypted": true
  }'
```

Repeat for `nlr_state_<slug>` (Qdrant/Neo4j) and `nlr_models_<slug>`
(or skip `models` and use a shared read-only volume per region).

### 2. Set per-user secrets

Fly secrets are per-app, not per-machine. To keep tenants isolated we write
them into the machine's env using the machine create payload (not
`fly secrets set`). The provisioner generates a one-time `init` script that
writes `/secrets/api-token` etc. inside the machine.

Alternative: use Fly's `file_secrets` on the machine config:

```json
"files": [
  { "guest_path": "/secrets/api-token",         "raw_value": "<base64>" },
  { "guest_path": "/secrets/cloudflared-token", "raw_value": "<base64>" },
  { "guest_path": "/secrets/model-url",         "raw_value": "<base64>" }
]
```

### 3. Create machine

```bash
curl -X POST "https://api.machines.dev/v1/apps/${APP_NAME}/machines" \
  -H "Authorization: Bearer ${FLY_API_TOKEN}" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "nlr-'"${SLUG}"'",
    "region": "iad",
    "config": {
      "image": "registry.fly.io/neuro-link-saas:vm-'"${IMAGE_VERSION}"'",
      "guest": { "cpu_kind": "shared", "cpus": 2, "memory_mb": 4096 },
      "mounts": [
        { "volume": "vol_data_xxx",   "path": "/data"   },
        { "volume": "vol_state_xxx",  "path": "/state"  },
        { "volume": "vol_models_xxx", "path": "/models" }
      ],
      "files": [
        { "guest_path": "/secrets/api-token",         "raw_value": "'"${B64_TOKEN}"'" },
        { "guest_path": "/secrets/cloudflared-token", "raw_value": "'"${B64_CF}"'"    }
      ],
      "services": [],
      "restart": { "policy": "always" },
      "auto_destroy": false,
      "checks": {
        "neuro-link-http": {
          "type": "http",
          "port": 8080,
          "path": "/healthz",
          "interval": "15s",
          "timeout": "2s"
        }
      }
    }
  }'
```

Note: `services` is empty because public ingress is handled by Cloudflare
Tunnel, not Fly's proxy. The machine has no public IP.

### 4. Upgrades / downgrades

```bash
# Resize guest
curl -X POST ".../machines/${MACHINE_ID}" -d '{ "config": { "guest": { "cpus": 4, "memory_mb": 8192 } } }'

# Grow volumes (online)
curl -X PUT ".../volumes/${VOL_ID}/extend" -d '{ "size_gb": 200 }'
```

## Cloudflare Tunnel API

Base: `https://api.cloudflare.com/client/v4/accounts/${CF_ACCOUNT_ID}`,
header `Authorization: Bearer ${CF_API_TOKEN}`.

### 1. Create a tunnel

```bash
curl -X POST ".../cfd_tunnel" \
  -H "Authorization: Bearer ${CF_API_TOKEN}" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "nlr-'"${SLUG}"'",
    "config_src": "cloudflare"
  }'
```

Response includes `id` (tunnel UUID) and `token` (base64, what
`cloudflared tunnel run --token` consumes). Store the token; we will
write it into the machine as `/secrets/cloudflared-token`.

### 2. Configure ingress for the tunnel

```bash
curl -X PUT ".../cfd_tunnel/${TUNNEL_ID}/configurations" \
  -H "Authorization: Bearer ${CF_API_TOKEN}" \
  -H "Content-Type: application/json" \
  -d '{
    "config": {
      "ingress": [
        { "hostname": "'"${SLUG}"'.neuro-link-recursive.io",
          "service":  "http://localhost:80" },
        { "service":  "http_status:404" }
      ]
    }
  }'
```

The tunnel runs inside the user VM and forwards to `http://localhost:80`
(Caddy), which fans out to neuro-link, Qdrant, Neo4j.

### 3. DNS CNAME for the subdomain

```bash
curl -X POST "https://api.cloudflare.com/client/v4/zones/${CF_ZONE_ID}/dns_records" \
  -H "Authorization: Bearer ${CF_API_TOKEN}" \
  -H "Content-Type: application/json" \
  -d '{
    "type":    "CNAME",
    "name":    "'"${SLUG}"'",
    "content": "'"${TUNNEL_ID}"'.cfargotunnel.com",
    "proxied": true
  }'
```

### 4. (Optional) Cloudflare Access policy

For paid tiers we front the Qdrant and Neo4j UIs with Cloudflare Access
so only the buyer's email can hit `/qdrant/` and `/neo4j/`. MCP JSON-RPC
itself is authenticated by the `api-token` bearer and can stay open.

## Stripe webhook handler outline

Event of interest: `checkout.session.completed`. Pseudo-code for
`/app/api/stripe-webhook/route.ts`:

```ts
export async function POST(req: Request) {
    // 1. Verify signature with STRIPE_WEBHOOK_SECRET. If bad, 400.
    const event = stripe.webhooks.constructEvent(body, sig, secret);

    if (event.type !== "checkout.session.completed") return new Response("ok");

    const session      = event.data.object as Stripe.Checkout.Session;
    const customerId   = session.customer as string;
    const tierPriceId  = session.metadata?.tier_price_id;
    const tier         = priceIdToTier(tierPriceId);  // "hobby" | "pro" | ...
    const clerkUserId  = session.metadata?.clerk_user_id;

    // 2. Idempotency: if users table already has a machine for this user, skip.
    if (await db.users.hasMachine(clerkUserId)) return new Response("already provisioned");

    // 3. Kick off provisioning. Long-running, so we enqueue and 200 fast.
    await queue.enqueue("provision", { clerkUserId, customerId, tier });

    return new Response("ok", { status: 200 });
}
```

The `provision` worker runs `provisioner/provision.ts` (sketched below),
which executes the Fly + Cloudflare calls in order and writes the result
to the `users` table. The dashboard polls `/api/provision/status`.

## Provisioner script

See `provision.ts` (TypeScript, runnable on Node or as an edge function).
It is a thin wrapper around `fetch` calls — no Fly/CF SDK dependency.

## Environment

```
FLY_API_TOKEN=fo1_...
FLY_APP_NAME=neuro-link-saas
FLY_REGION=iad
FLY_IMAGE_VERSION=v1.0.0

CF_API_TOKEN=...
CF_ACCOUNT_ID=...
CF_ZONE_ID=...
CF_ROOT_DOMAIN=neuro-link-recursive.io

STRIPE_SECRET_KEY=sk_live_...
STRIPE_WEBHOOK_SECRET=whsec_...
STRIPE_PRICE_HOBBY=price_...
STRIPE_PRICE_PRO=price_...
STRIPE_PRICE_TEAM=price_...
STRIPE_PRICE_ENTERPRISE=price_...

CLERK_SECRET_KEY=sk_live_...

MODEL_SIGNED_URL_BASE=https://models.neuro-link-recursive.io
```

## Open implementation items

- Decide: shared read-only `/models` volume per region vs per-user copy. Shared
  is ~15 GB of Octen GGUF; per-user multiplies that by user count.
- Decide: single-region (iad) to start or multi-region. Multi-region requires
  one Fly app per region + model replication.
- Decide: whether `nlr serve` needs a native health endpoint (see
  `checks.neuro-link-http`). Currently none — add `/healthz` before launch.
- Teardown script: when a Stripe subscription cancels, destroy machine,
  delete volumes, delete tunnel + CNAME, clear users row.
