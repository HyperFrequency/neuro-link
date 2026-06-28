# neuro-link-recursive.io web

Next.js 15 App Router app. Marketing landing + authenticated dashboard + webhooks.

## Layout

```
app/
  page.tsx                  marketing landing
  layout.tsx                root layout (wrap in <ClerkProvider> in impl)
  signup/page.tsx           Clerk sign-up + tier picker + Stripe Checkout
  dashboard/page.tsx        MCP URL, API token, VM status, billing
  api/
    provision/route.ts      internal; triggers provisioner/provision.ts
    stripe-webhook/route.ts checkout.session.completed -> /api/provision
lib/
  auth-stub.ts              TODO(impl): replace with @clerk/nextjs auth()
  db-stub.ts                TODO(impl): replace with Postgres
```

## Setup (after research gating is resolved)

```bash
cp .env.example .env.local
# fill in Clerk, Stripe, Fly, Cloudflare keys
npm install
npm run dev
```

## Not yet wired

- `@clerk/nextjs` provider + protected routes.
- Real Stripe Checkout session creation at `/api/checkout` (TODO).
- Postgres. Currently an in-memory stub in `lib/db-stub.ts`.
- Background queue. The webhook currently fires `/api/provision` directly —
  replace with Inngest / Temporal / SQS for production.
