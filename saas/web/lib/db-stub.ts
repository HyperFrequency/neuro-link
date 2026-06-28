// lib/db-stub.ts
// TODO(impl): replace in-memory stub with real Postgres (Drizzle / Prisma / pg).

import type { Tier } from "../../provisioner/provision";

export interface UserRecord {
    userId: string;
    stripeCustomerId: string;
    tier: Tier;
    slug: string;
    mcpUrl: string;
    apiToken: string;
    flyMachineId: string;
    cfTunnelId: string;
}

const store = new Map<string, UserRecord>();

export async function getUserRecord(userId: string): Promise<UserRecord | null> {
    return store.get(userId) ?? null;
}

export async function upsertUserRecord(r: UserRecord): Promise<void> {
    store.set(r.userId, r);
}
