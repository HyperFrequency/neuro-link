// lib/auth-stub.ts
// TODO(impl): delete this file and use `auth()` from @clerk/nextjs/server.

export async function getCurrentUser(): Promise<{ id: string; email: string }> {
    return { id: "stub_user", email: "stub@example.com" };
}
