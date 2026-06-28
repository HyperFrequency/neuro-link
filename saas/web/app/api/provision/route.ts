// app/api/provision/route.ts
// Internal endpoint. Called by the Stripe webhook worker (or from the
// provisioner queue) with a valid PROVISION_INTERNAL_TOKEN. Kicks off a
// real VM provision via provisioner/provision.ts.
//
// In production this should be a queue consumer (Inngest, Temporal, etc.),
// not an HTTP endpoint — Fly/CF calls take 30-120s and can time out.
//
// TODO(impl):
//   - Swap for a proper background job runner.
//   - Persist step-by-step to DB for idempotent retries.

import { NextRequest, NextResponse } from "next/server";
import { provision, type Tier, type ProvisionInput } from "../../../../provisioner/provision";
import { upsertUserRecord } from "@/lib/db-stub";

export const runtime = "nodejs";

export async function POST(req: NextRequest) {
    // 1. Internal auth.
    const auth = req.headers.get("authorization");
    if (auth !== `Bearer ${process.env.PROVISION_INTERNAL_TOKEN}`) {
        return NextResponse.json({ error: "unauthorized" }, { status: 401 });
    }

    // 2. Parse + validate input.
    const body = (await req.json()) as Partial<ProvisionInput>;
    const { clerkUserId, stripeCustomerId, tier, slug } = body;
    if (!clerkUserId || !stripeCustomerId || !tier || !slug) {
        return NextResponse.json({ error: "missing fields" }, { status: 400 });
    }
    if (!/^[a-z0-9-]{3,32}$/.test(slug)) {
        return NextResponse.json({ error: "invalid slug" }, { status: 400 });
    }

    // 3. Provision (long-running; ~30-120s).
    try {
        const result = await provision({
            clerkUserId,
            stripeCustomerId,
            tier: tier as Tier,
            slug,
        });

        await upsertUserRecord({
            userId: clerkUserId,
            stripeCustomerId,
            tier: tier as Tier,
            slug,
            mcpUrl: result.mcpUrl,
            apiToken: result.apiToken,
            flyMachineId: result.flyMachineId,
            cfTunnelId: result.cfTunnelId,
        });

        return NextResponse.json({ ok: true, mcpUrl: result.mcpUrl });
    } catch (err) {
        console.error("[provision] failed", err);
        // TODO(impl): mark job failed in DB, alert ops.
        return NextResponse.json({ error: String(err) }, { status: 500 });
    }
}
