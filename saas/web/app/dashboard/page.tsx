// app/dashboard/page.tsx
// Shows: MCP URL, API token, VM status, billing portal link.
// TODO(impl):
//   - Gate behind Clerk auth() check (redirect to /signup if unauthed).
//   - Read user row from DATABASE_URL.
//   - Poll /api/provision/status for VM liveness (Fly Machines API get).
//   - "Regenerate API token" action + "Open billing portal" (Stripe portal).

import { getCurrentUser } from "@/lib/auth-stub";
import { getUserRecord } from "@/lib/db-stub";

export const dynamic = "force-dynamic";

export default async function Dashboard() {
    const user = await getCurrentUser();          // TODO(impl): real Clerk auth()
    const record = await getUserRecord(user.id);  // TODO(impl): real DB read

    if (!record) {
        return (
            <main style={{ maxWidth: 640, margin: "0 auto", padding: "4rem 1.5rem" }}>
                <h1>Dashboard</h1>
                <p>No subscription found. <a href="/signup">Choose a plan</a>.</p>
            </main>
        );
    }

    return (
        <main style={{ maxWidth: 720, margin: "0 auto", padding: "4rem 1.5rem" }}>
            <h1>Dashboard</h1>

            <section style={{ marginTop: "2rem" }}>
                <h2>Your MCP endpoint</h2>
                <CopyRow label="MCP URL"  value={record.mcpUrl} />
                <CopyRow label="API token" value={record.apiToken} secret />
                <details style={{ marginTop: "1rem" }}>
                    <summary>Claude Code config</summary>
                    <pre style={preStyle}>
{JSON.stringify(
    {
        mcpServers: {
            "neuro-link-recursive": {
                type: "http",
                url: `${record.mcpUrl}`,
                headers: { Authorization: `Bearer ${record.apiToken}` },
            },
        },
    },
    null,
    2,
)}
                    </pre>
                </details>
            </section>

            <section style={{ marginTop: "2rem" }}>
                <h2>VM status</h2>
                <p>Machine: <code>{record.flyMachineId}</code></p>
                <p>Tier: <strong>{record.tier}</strong></p>
                {/* TODO(impl): fetch live state from Fly API and show started/stopped/failing. */}
                <p style={{ color: "#666" }}>Live status polling not wired yet.</p>
            </section>

            <section style={{ marginTop: "2rem" }}>
                <h2>Billing</h2>
                {/* TODO(impl): server action that creates a Stripe billing portal session. */}
                <a href="#" style={{ color: "#111", textDecoration: "underline" }}>
                    Open billing portal
                </a>
            </section>
        </main>
    );
}

function CopyRow({ label, value, secret }: { label: string; value: string; secret?: boolean }) {
    return (
        <div style={{ margin: "0.5rem 0" }}>
            <div style={{ fontSize: "0.85rem", color: "#666" }}>{label}</div>
            <code style={{ display: "block", padding: "0.5rem", background: "#f5f5f5", borderRadius: 4, wordBreak: "break-all" }}>
                {secret ? value.replace(/.(?=.{4})/g, "*") : value}
            </code>
        </div>
    );
}

const preStyle: React.CSSProperties = {
    padding: "0.75rem",
    background: "#f5f5f5",
    borderRadius: 4,
    overflowX: "auto",
    fontSize: "0.85rem",
};
