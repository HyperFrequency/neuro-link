# Phase G Research Memo

Status: OPEN. This memo lists what we know, what we suspect, and what must
be confirmed before neuro-link-recursive.io can take paying customers.

## 1. Obsidian Sync ToS

What I know from general knowledge (must be verified with primary sources
before launch — fold-in of a lawyer review would be safer than relying on
these notes):

- **Obsidian Sync is a paid consumer service** for syncing one user's vault
  across their own devices. It is sold per-person; multi-user collaboration
  on a shared vault is explicitly an emerging feature, not the default.
- **Programmatic access is not a published API.** Sync uses end-to-end
  encryption with keys derived from the user's password, so a third-party
  service (our VM) cannot decrypt vaults without the user's password — which
  they should not hand us.
- **The TurboVault MCP + Obsidian plugin pattern we already use in the CLI
  assumes the vault is on the user's local filesystem**, not in Obsidian Sync.
  That is fine for the self-host CLI case; it is a problem for the SaaS VM,
  which is remote from the user's machine.

Risk for SaaS:
- If we advertise "bring your Obsidian vault" we need a way to ship the vault
  to the VM. Obsidian Sync cannot be used as the transport — our VM is not
  a user device and cannot join the sync cluster.
- Bundling Obsidian Desktop into the VM and logging in as the user would
  almost certainly violate the ToS (running the client on infrastructure the
  end user does not own + per-device license semantics).

### Conclusion / recommendation for launch
- **Do not depend on Obsidian Sync.** Treat Obsidian as a client-side viewer
  the user runs locally against a vault that is synced to our VM via a
  different mechanism (git, rsync, S3, rclone, or our own sync daemon).
- **OR** ship a "file-system-only mode" where the VM has no Obsidian at all
  and the user interacts only via MCP + the web dashboard. See §2.

## 2. Alternative: file-system-only mode

Rationale: Obsidian is a *viewer* over markdown files. neuro-link-recursive
already owns the file layout (`02-KB-main/`, `03-ontology-main/`, etc.) and
exposes everything over MCP. For SaaS we can strip Obsidian entirely:

- VM stores markdown files in `/data/02-KB-main/` as usual.
- No Obsidian binary on the VM.
- No Obsidian Sync dependency.
- User reads / edits via:
  - MCP tool calls from Claude Code / Cursor (primary).
  - Dashboard viewer (web, read + simple edit).
  - Optional: mount `/data` over WebDAV / SSHFS to a local folder the user
    opens in their own Obsidian install. This keeps the viewer on the user's
    machine, where the license applies.

Benefit: no licensing risk, simpler image, Cloudflare Tunnel is sufficient.
Cost: users who *want* the Obsidian plugin experience need a sync step
(`rclone`, `git`, or the WebDAV mount above).

### Conclusion
- **Ship file-system-only mode for v1 launch.**
- Position "use your existing Obsidian vault" as an advanced / self-host path
  that requires the user to run the sync daemon locally.

## 3. Cost modeling — Fly.io infra cost vs tier pricing

Numbers are Fly's published list prices as of early 2026 (need to re-verify
with Fly's billing dashboard before any public announcement). All in USD/month.

### Per-machine compute (Fly pricing)
| Guest preset | vCPU | RAM | Fly list price |
|---|---|---|---|
| shared-cpu-1x | 1 | 2 GB | ~$3.89 |
| shared-cpu-2x | 2 | 4 GB | ~$15.55 |
| performance-2x | 2 | 8 GB | ~$62.00 |
| performance-4x | 4 | 16 GB | ~$124.00 |

### Per-volume storage
- ~$0.15 / GB-month for encrypted volumes.

### Bandwidth
- Free outbound included per machine (~100 GB/mo typical), then ~$0.02/GB.

### Cost per tier (rough)

| Tier | Compute | Volumes (data+state+models) | Est infra | Sell | Margin |
|------|---------|----------------------------|-----------|------|--------|
| Hobby      | shared-1x $3.89  | (10+5+20) GB * $0.15 = $5.25 | ~$9   | $19  | ~52% |
| Pro        | shared-2x $15.55 | (50+20+20) GB * $0.15 = $13.50 | ~$29 | $49  | ~40% |
| Team       | perf-2x $62.00   | (200+50+20) GB * $0.15 = $40.50 | ~$102 | $129 | ~21% |
| Enterprise | perf-4x $124.00  | (1024+100+20) GB * $0.15 = $171.60 | ~$296 | $299 | ~1% (!!) |

**Margins look wrong at Enterprise.** Two fixes:
1. Reduce the baseline storage included in Enterprise (e.g. 500 GB, not 1 TB).
2. Charge for storage overage above a threshold ($X per 100 GB extra).
3. Increase Enterprise list price (typical B2B list: $499-$999).

### Shared-model optimization
Across all tiers the 15-20 GB `/models` volume is identical (Octen GGUF).
If we use a **single region-level read-only volume** shared across machines,
per-user models cost drops from $3 to near-zero. This single optimization
changes Pro margin from 40% to 46% and enterprise from 1% to 6% — still bad
at the top but materially better at Hobby/Pro.

### Egress sensitivity
If a user runs heavy MCP queries (say 200 GB/mo outbound) at ~$0.02/GB we pay
$4 on top. Not a meaningful hit at Pro and up, but Hobby is already tight.

### Conclusion
- **Enterprise tier needs re-pricing or reduced defaults.** Current $299 is
  close to break-even.
- **Implement shared read-only `/models` volume** per region before launch.
- **Publish an overage policy for storage above the included quota.**

## 4. Open questions that block launch

Ordered by expected impact:

1. **Model licensing.** Is Octen-Embedding-8B GGUF redistributable for commercial
   SaaS? Need to read the underlying model card + any base-model licenses.
   Fallback: BGE-M3 or GTE-Qwen2 (Apache 2.0) instead of Octen.

2. **Data residency.** Do we offer EU-resident VMs from day 1? If yes, we need
   a Fly region in Frankfurt/Amsterdam AND a model volume replicated there.

3. **Backup strategy.** Fly volumes are snapshotted daily by Fly — is that
   enough, or do we also need off-platform backups (B2, S3) for disaster
   recovery? Per-user volumes make restore-to-point-in-time non-trivial.

4. **DDoS & abuse on the tunnel.** Cloudflare Tunnel puts Cloudflare in front
   of every user subdomain for free, but we still need per-tenant rate
   limits on the MCP endpoint. Does neuro-link need a native rate limiter?

5. **Health checks.** `neuro-link serve` currently has no `/healthz`. Without
   it, Fly can't detect a stuck machine, and the dashboard can't show green.
   Small but blocking.

6. **Secrets handling.** We plan to push the API token via Fly's
   `files[].raw_value` (base64 in the machine config). Does Fly encrypt those
   at rest on the machine metadata store? Alternative: pull them from a
   secrets service at boot. Need confirmation.

7. **Clerk ↔ Stripe ↔ DB schema.** We don't have a source-of-truth schema
   for `users` yet (slug uniqueness, tier upgrades, cancellation lifecycle,
   org/team seats). Must be nailed before writing any DB code.

8. **Cloudflare Tunnel limits.** Cloudflare has a per-account cap on active
   tunnels (historically ~1000). At scale we will need either a paid CF plan
   with higher limits OR a different ingress model (shared CF Tunnel routing
   by Host header to a gateway that proxies to per-user VMs).

9. **Teardown flow.** On subscription cancellation: grace period? Data
   export window? Immediate delete? GDPR right-to-erasure timeline?

10. **Obsidian Sync ToS** — see §1. Primary-source verification required
    before we mention Obsidian anywhere in marketing copy.
