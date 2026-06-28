// app/api/stripe-webhook/route.ts
// Receives Stripe webhook events. Signature-verifies, then enqueues a
// provision job for checkout.session.completed.
//
// NOTE: the raw request body is needed for signature verification — we
// read it via req.text() rather than req.json().
//
// TODO(impl):
//   - Handle customer.subscription.deleted → enqueue teardown.
//   - Handle customer.subscription.updated → retier + resize machine.
//   - Swap direct fetch() for a real queue (Inngest / Temporal / SQS).

import { NextRequest, NextResponse } from "next/server";
import Stripe from "stripe";

export const runtime = "nodejs";

const stripe = new Stripe(process.env.STRIPE_SECRET_KEY ?? "", { apiVersion: "2024-10-28.acacia" as any });

function priceIdToTier(priceId: string | null): "hobby" | "pro" | "team" | "enterprise" | null {
    switch (priceId) {
        case process.env.STRIPE_PRICE_HOBBY:      return "hobby";
        case process.env.STRIPE_PRICE_PRO:        return "pro";
        case process.env.STRIPE_PRICE_TEAM:       return "team";
        case process.env.STRIPE_PRICE_ENTERPRISE: return "enterprise";
        default:                                   return null;
    }
}

export async function POST(req: NextRequest) {
    const sig = req.headers.get("stripe-signature");
    const secret = process.env.STRIPE_WEBHOOK_SECRET;
    if (!sig || !secret) return NextResponse.json({ error: "config" }, { status: 500 });

    const rawBody = await req.text();
    let event: Stripe.Event;
    try {
        event = stripe.webhooks.constructEvent(rawBody, sig, secret);
    } catch (err) {
        return NextResponse.json({ error: `bad signature: ${err}` }, { status: 400 });
    }

    if (event.type === "checkout.session.completed") {
        const session       = event.data.object as Stripe.Checkout.Session;
        const clerkUserId   = session.metadata?.clerk_user_id ?? null;
        const requestedSlug = session.metadata?.slug ?? null;
        const customerId    = typeof session.customer === "string" ? session.customer : null;

        // Derive tier from the first line item's price.
        const expanded = await stripe.checkout.sessions.retrieve(session.id, { expand: ["line_items"] });
        const priceId = expanded.line_items?.data[0]?.price?.id ?? null;
        const tier = priceIdToTier(priceId);

        if (!clerkUserId || !customerId || !tier || !requestedSlug) {
            console.error("[stripe-webhook] missing fields", { clerkUserId, customerId, tier, requestedSlug });
            return NextResponse.json({ error: "missing metadata" }, { status: 400 });
        }

        // Fire-and-forget to /api/provision. In prod this should be a queue enqueue
        // so the webhook returns in <1s.
        //
        // TODO(impl): await queue.enqueue("provision", { ... }) instead.
        fetch(`${process.env.NEXT_PUBLIC_APP_URL ?? ""}/api/provision`, {
            method: "POST",
            headers: {
                "Content-Type": "application/json",
                Authorization: `Bearer ${process.env.PROVISION_INTERNAL_TOKEN}`,
            },
            body: JSON.stringify({
                clerkUserId,
                stripeCustomerId: customerId,
                tier,
                slug: requestedSlug,
            }),
        }).catch(err => console.error("[stripe-webhook] enqueue failed", err));
    }

    // TODO(impl): subscription.deleted, subscription.updated.

    return NextResponse.json({ received: true });
}
