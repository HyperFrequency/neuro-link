// provisioner/provision.ts
//
// Single-file, SDK-free wrapper around Fly.io Machines API + Cloudflare API.
// Called by the Stripe webhook worker (or directly for manual provisioning).
//
// STATUS: scaffold. All API paths and payloads match the current Fly/CF docs
// as of 2026-04, but nothing has been end-to-end tested yet. Every function
// returns a typed result so a real worker can persist intermediate state and
// recover from partial failures.
//
// TODO(impl):
//   - Persist each step to the `users` table so retries are idempotent.
//   - Wire in a real DB (Postgres on Fly, Supabase, etc.).
//   - Add exponential backoff on 429/5xx from both APIs.
//   - Add teardown() for subscription cancellations.

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type Tier = "hobby" | "pro" | "team" | "enterprise";

export interface TierConfig {
    cpus: number;
    cpu_kind: "shared" | "performance";
    memory_mb: number;
    data_gb: number;
    state_gb: number;
    models_gb: number;
}

export const TIERS: Record<Tier, TierConfig> = {
    hobby:      { cpus: 1, cpu_kind: "shared",      memory_mb: 2048,  data_gb: 10,   state_gb: 5,   models_gb: 20 },
    pro:        { cpus: 2, cpu_kind: "shared",      memory_mb: 4096,  data_gb: 50,   state_gb: 20,  models_gb: 20 },
    team:       { cpus: 4, cpu_kind: "performance", memory_mb: 8192,  data_gb: 200,  state_gb: 50,  models_gb: 20 },
    enterprise: { cpus: 8, cpu_kind: "performance", memory_mb: 16384, data_gb: 1024, state_gb: 100, models_gb: 20 },
};

export interface ProvisionInput {
    clerkUserId: string;
    stripeCustomerId: string;
    tier: Tier;
    slug: string;   // unique subdomain, lowercase a-z0-9-
}

export interface ProvisionResult {
    mcpUrl: string;         // https://<slug>.neuro-link-recursive.io/mcp
    dashboardSubdomain: string;
    apiToken: string;
    flyMachineId: string;
    cfTunnelId: string;
    volumeIds: { data: string; state: string; models: string };
}

// ---------------------------------------------------------------------------
// Env
// ---------------------------------------------------------------------------

const env = {
    FLY_API_TOKEN:     process.env.FLY_API_TOKEN!,
    FLY_APP_NAME:      process.env.FLY_APP_NAME ?? "neuro-link-saas",
    FLY_REGION:        process.env.FLY_REGION ?? "iad",
    FLY_IMAGE_VERSION: process.env.FLY_IMAGE_VERSION ?? "latest",
    CF_API_TOKEN:      process.env.CF_API_TOKEN!,
    CF_ACCOUNT_ID:     process.env.CF_ACCOUNT_ID!,
    CF_ZONE_ID:        process.env.CF_ZONE_ID!,
    CF_ROOT_DOMAIN:    process.env.CF_ROOT_DOMAIN ?? "neuro-link-recursive.io",
    MODEL_SIGNED_URL:  process.env.MODEL_SIGNED_URL ?? "",
};

const FLY_API = "https://api.machines.dev/v1";
const CF_API  = "https://api.cloudflare.com/client/v4";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async function flyFetch(path: string, init: RequestInit = {}): Promise<any> {
    const res = await fetch(`${FLY_API}${path}`, {
        ...init,
        headers: {
            Authorization: `Bearer ${env.FLY_API_TOKEN}`,
            "Content-Type": "application/json",
            ...(init.headers ?? {}),
        },
    });
    if (!res.ok) throw new Error(`fly ${path} ${res.status}: ${await res.text()}`);
    return res.json();
}

async function cfFetch(path: string, init: RequestInit = {}): Promise<any> {
    const res = await fetch(`${CF_API}${path}`, {
        ...init,
        headers: {
            Authorization: `Bearer ${env.CF_API_TOKEN}`,
            "Content-Type": "application/json",
            ...(init.headers ?? {}),
        },
    });
    if (!res.ok) throw new Error(`cf ${path} ${res.status}: ${await res.text()}`);
    const body = await res.json();
    if (body.success === false) throw new Error(`cf ${path} errors: ${JSON.stringify(body.errors)}`);
    return body;
}

function b64(s: string): string {
    // Fly's `files[].raw_value` expects base64 of the file contents.
    return Buffer.from(s, "utf8").toString("base64");
}

function randomToken(): string {
    const bytes = new Uint8Array(32);
    crypto.getRandomValues(bytes);
    return Buffer.from(bytes).toString("base64url");
}

// ---------------------------------------------------------------------------
// Steps
// ---------------------------------------------------------------------------

async function createVolume(name: string, sizeGb: number): Promise<string> {
    const body = await flyFetch(`/apps/${env.FLY_APP_NAME}/volumes`, {
        method: "POST",
        body: JSON.stringify({
            name,
            region: env.FLY_REGION,
            size_gb: sizeGb,
            fstype: "ext4",
            encrypted: true,
        }),
    });
    return body.id as string;
}

async function createCfTunnel(slug: string): Promise<{ id: string; token: string }> {
    const body = await cfFetch(`/accounts/${env.CF_ACCOUNT_ID}/cfd_tunnel`, {
        method: "POST",
        body: JSON.stringify({ name: `nlr-${slug}`, config_src: "cloudflare" }),
    });
    return { id: body.result.id, token: body.result.token };
}

async function configureCfTunnel(tunnelId: string, hostname: string): Promise<void> {
    await cfFetch(`/accounts/${env.CF_ACCOUNT_ID}/cfd_tunnel/${tunnelId}/configurations`, {
        method: "PUT",
        body: JSON.stringify({
            config: {
                ingress: [
                    { hostname, service: "http://localhost:80" },
                    { service: "http_status:404" },
                ],
            },
        }),
    });
}

async function createCfDns(slug: string, tunnelId: string): Promise<void> {
    await cfFetch(`/zones/${env.CF_ZONE_ID}/dns_records`, {
        method: "POST",
        body: JSON.stringify({
            type: "CNAME",
            name: slug,
            content: `${tunnelId}.cfargotunnel.com`,
            proxied: true,
        }),
    });
}

async function createMachine(args: {
    slug: string;
    tier: Tier;
    volumeIds: { data: string; state: string; models: string };
    apiToken: string;
    cfTunnelToken: string;
}): Promise<string> {
    const cfg = TIERS[args.tier];
    const body = await flyFetch(`/apps/${env.FLY_APP_NAME}/machines`, {
        method: "POST",
        body: JSON.stringify({
            name: `nlr-${args.slug}`,
            region: env.FLY_REGION,
            config: {
                image: `registry.fly.io/${env.FLY_APP_NAME}:vm-${env.FLY_IMAGE_VERSION}`,
                guest: {
                    cpu_kind: cfg.cpu_kind,
                    cpus: cfg.cpus,
                    memory_mb: cfg.memory_mb,
                },
                mounts: [
                    { volume: args.volumeIds.data,   path: "/data"   },
                    { volume: args.volumeIds.state,  path: "/state"  },
                    { volume: args.volumeIds.models, path: "/models" },
                ],
                files: [
                    { guest_path: "/secrets/api-token",         raw_value: b64(args.apiToken) },
                    { guest_path: "/secrets/cloudflared-token", raw_value: b64(args.cfTunnelToken) },
                    { guest_path: "/secrets/model-url",         raw_value: b64(env.MODEL_SIGNED_URL) },
                ],
                restart: { policy: "always" },
                auto_destroy: false,
                // No `services` block: public ingress is handled by cloudflared
                // inside the machine, not by Fly's proxy. Machine has no public IP.
                checks: {
                    "neuro-link-http": {
                        type: "http",
                        port: 8080,
                        path: "/healthz",
                        interval: "15s",
                        timeout: "2s",
                    },
                },
            },
        }),
    });
    return body.id as string;
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

export async function provision(input: ProvisionInput): Promise<ProvisionResult> {
    const cfg = TIERS[input.tier];
    const hostname = `${input.slug}.${env.CF_ROOT_DOMAIN}`;
    const apiToken = randomToken();

    // 1. Cloudflare tunnel + DNS
    const tunnel = await createCfTunnel(input.slug);
    await configureCfTunnel(tunnel.id, hostname);
    await createCfDns(input.slug, tunnel.id);

    // 2. Fly volumes
    const [dataVol, stateVol, modelsVol] = await Promise.all([
        createVolume(`nlr_data_${input.slug}`,   cfg.data_gb),
        createVolume(`nlr_state_${input.slug}`,  cfg.state_gb),
        createVolume(`nlr_models_${input.slug}`, cfg.models_gb),
    ]);

    // 3. Fly machine
    const machineId = await createMachine({
        slug: input.slug,
        tier: input.tier,
        volumeIds: { data: dataVol, state: stateVol, models: modelsVol },
        apiToken,
        cfTunnelToken: tunnel.token,
    });

    return {
        mcpUrl: `https://${hostname}/mcp`,
        dashboardSubdomain: hostname,
        apiToken,
        flyMachineId: machineId,
        cfTunnelId: tunnel.id,
        volumeIds: { data: dataVol, state: stateVol, models: modelsVol },
    };
}

// TODO(impl): export async function teardown(userId: string): Promise<void> { ... }
