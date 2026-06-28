// app/signup/page.tsx
// Signup + tier selection. Real implementation: Clerk <SignUp /> then Stripe Checkout.
// Scaffold below is static; wire it up once keys exist.

"use client";

import { useState } from "react";

const TIERS = [
    { id: "hobby",      name: "Hobby",      price: "$19",  priceEnv: "STRIPE_PRICE_HOBBY" },
    { id: "pro",        name: "Pro",        price: "$49",  priceEnv: "STRIPE_PRICE_PRO" },
    { id: "team",       name: "Team",       price: "$129", priceEnv: "STRIPE_PRICE_TEAM" },
    { id: "enterprise", name: "Enterprise", price: "$299", priceEnv: "STRIPE_PRICE_ENTERPRISE" },
];

export default function Signup() {
    const [tier, setTier] = useState<string>("pro");
    const [busy, setBusy] = useState(false);

    async function handleCheckout() {
        setBusy(true);
        // TODO(impl):
        //   1. Ensure user is signed in via Clerk (useUser()).
        //   2. POST /api/checkout with { tier, slug } -> returns Stripe session url.
        //   3. window.location = session.url.
        alert(`TODO: create Stripe Checkout session for tier=${tier}`);
        setBusy(false);
    }

    return (
        <main style={{ maxWidth: 560, margin: "0 auto", padding: "4rem 1.5rem" }}>
            <h1>Create your brain</h1>

            {/* TODO(impl): <SignUp routing="hash" signInUrl="/signup" /> from @clerk/nextjs */}
            <div style={{ padding: "1rem", border: "1px dashed #aaa", borderRadius: 8, marginBottom: "2rem" }}>
                <strong>Clerk sign-up goes here.</strong>
                <p style={{ color: "#666", margin: "0.5rem 0 0" }}>
                    Wire <code>@clerk/nextjs</code> and drop <code>&lt;SignUp /&gt;</code> in.
                </p>
            </div>

            <h2>Pick a tier</h2>
            <div style={{ display: "grid", gap: "0.5rem" }}>
                {TIERS.map(t => (
                    <label
                        key={t.id}
                        style={{
                            display: "flex",
                            justifyContent: "space-between",
                            padding: "0.75rem 1rem",
                            border: tier === t.id ? "2px solid #111" : "1px solid #ccc",
                            borderRadius: 8,
                            cursor: "pointer",
                        }}
                    >
                        <span>
                            <input
                                type="radio"
                                name="tier"
                                checked={tier === t.id}
                                onChange={() => setTier(t.id)}
                                style={{ marginRight: "0.5rem" }}
                            />
                            {t.name}
                        </span>
                        <span>{t.price}/mo</span>
                    </label>
                ))}
            </div>

            <button
                onClick={handleCheckout}
                disabled={busy}
                style={{
                    marginTop: "1.5rem",
                    width: "100%",
                    padding: "0.85rem",
                    background: "#111",
                    color: "#fff",
                    border: "none",
                    borderRadius: 8,
                    fontSize: "1rem",
                    cursor: "pointer",
                }}
            >
                {busy ? "..." : "Continue to checkout"}
            </button>
        </main>
    );
}
