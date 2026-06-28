// app/page.tsx — marketing landing page.
// Intentionally dependency-free so it renders before any Clerk/Stripe wiring.

import Link from "next/link";

const TIERS = [
    { name: "Hobby",      price: "$19",  blurb: "Single user. 10 GB wiki. 2 GB RAM VM." },
    { name: "Pro",        price: "$49",  blurb: "50 GB wiki. 4 GB RAM. Heavy Obsidian vaults." },
    { name: "Team",       price: "$129", blurb: "Up to 5 seats. 200 GB wiki. Dedicated vCPUs." },
    { name: "Enterprise", price: "$299", blurb: "SSO, Cloudflare Access, custom domain, 1 TB." },
];

export default function Landing() {
    return (
        <main style={{ maxWidth: 880, margin: "0 auto", padding: "4rem 1.5rem" }}>
            <section>
                <h1 style={{ fontSize: "2.75rem", lineHeight: 1.1, margin: 0 }}>
                    Your agent&apos;s brain. On a dedicated server. In one click.
                </h1>
                <p style={{ fontSize: "1.15rem", color: "#444", marginTop: "1.25rem" }}>
                    neuro-link-recursive gives every AI agent a persistent wiki, reasoning ontology,
                    vector + graph memory, and a local embedding model — all served over MCP from
                    your own isolated VM. No shared database. No leaked prompts. No cold starts.
                </p>
                <div style={{ display: "flex", gap: "1rem", marginTop: "2rem" }}>
                    <Link
                        href="/signup"
                        style={{
                            padding: "0.75rem 1.5rem",
                            background: "#111",
                            color: "#fff",
                            borderRadius: 8,
                            textDecoration: "none",
                        }}
                    >
                        Start free trial
                    </Link>
                    <Link
                        href="#pricing"
                        style={{
                            padding: "0.75rem 1.5rem",
                            border: "1px solid #111",
                            color: "#111",
                            borderRadius: 8,
                            textDecoration: "none",
                        }}
                    >
                        See pricing
                    </Link>
                </div>
            </section>

            <section style={{ marginTop: "4rem" }}>
                <h2>What you get</h2>
                <ul style={{ lineHeight: 1.7 }}>
                    <li>Dedicated Fly.io VM with your own <code>&lt;slug&gt;.neuro-link-recursive.io</code> subdomain.</li>
                    <li>Bundled Qdrant + Neo4j + llama.cpp + Octen-Embedding-8B.</li>
                    <li>MCP endpoint ready to paste into Claude Code, Cursor, or any MCP client.</li>
                    <li>Auto-curated wiki, reasoning ontologies, recursive self-improvement.</li>
                </ul>
            </section>

            <section id="pricing" style={{ marginTop: "4rem" }}>
                <h2>Pricing</h2>
                <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit,minmax(200px,1fr))", gap: "1rem", marginTop: "1rem" }}>
                    {TIERS.map(t => (
                        <div key={t.name} style={{ border: "1px solid #ddd", borderRadius: 8, padding: "1.25rem" }}>
                            <h3 style={{ margin: 0 }}>{t.name}</h3>
                            <p style={{ fontSize: "1.5rem", margin: "0.5rem 0" }}>{t.price}<span style={{ fontSize: "0.9rem", color: "#666" }}>/mo</span></p>
                            <p style={{ color: "#555", margin: 0 }}>{t.blurb}</p>
                        </div>
                    ))}
                </div>
            </section>
        </main>
    );
}
