/** @type {import('next').NextConfig} */
const nextConfig = {
    reactStrictMode: true,
    // The Stripe webhook needs the raw body; opt that route out of Next's default
    // body parsing. In the App Router this is handled per-route via `export const runtime`.
    experimental: { serverActions: { bodySizeLimit: "2mb" } },
};

export default nextConfig;
