// app/layout.tsx
// Root layout. TODO(impl): wrap in <ClerkProvider> from @clerk/nextjs once keys exist.
import type { ReactNode } from "react";

export const metadata = {
    title: "neuro-link-recursive",
    description: "Unified context, memory & behavior control plane for LLM agents.",
};

export default function RootLayout({ children }: { children: ReactNode }) {
    return (
        <html lang="en">
            <body style={{ fontFamily: "system-ui, sans-serif", margin: 0 }}>
                {children}
            </body>
        </html>
    );
}
