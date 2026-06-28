// SPDX-License-Identifier: MIT
//
// VaultEventsClient tests — HTTP long-poll pull transport.
//
// These tests cover the spec laid out in the scope doc:
//   - subscribe issues the expected JSON-RPC body
//   - long-poll loop emits events and threads `since_seq`
//   - `dropped > 0` produces a synthetic Overflow event
//   - 401/403 from the server is terminal (no retry)
//   - transient errors back off exponentially
//   - abort mid-fetch calls unsubscribe best-effort
//   - shutdown path closes cleanly
//
// Network I/O is mocked via `globalThis.fetch` — we never hit the wire.

import "./_obsidian-mock";
import {
  afterEach,
  beforeEach,
  describe,
  expect,
  test,
} from "bun:test";
import {
  coerceToHttpUrl,
  extractFetchResult,
  extractSubscribeResult,
  normaliseVaultEvent,
  VaultEventsClient,
  type VaultEvent,
} from "../src/mcp-vault-events";

// ── pure helpers ──────────────────────────────────────────────────────

describe("extractSubscribeResult", () => {
  test("reads direct handle/created_at", () => {
    expect(
      extractSubscribeResult({ handle: "h-1", created_at: "2026-04-17T00:00:00Z" })
    ).toEqual({ handle: "h-1", createdAt: "2026-04-17T00:00:00Z" });
  });

  test("unwraps MCP content envelope", () => {
    const raw = {
      content: [
        {
          type: "text",
          text: JSON.stringify({ handle: "h-2", created_at: "2026-04-17T01:00:00Z" }),
        },
      ],
    };
    expect(extractSubscribeResult(raw)).toEqual({
      handle: "h-2",
      createdAt: "2026-04-17T01:00:00Z",
    });
  });

  test("tolerates missing created_at", () => {
    expect(extractSubscribeResult({ handle: "h-3" })).toEqual({
      handle: "h-3",
      createdAt: null,
    });
  });

  test("returns null when no handle found", () => {
    expect(extractSubscribeResult({})).toBeNull();
    expect(extractSubscribeResult(null)).toBeNull();
  });
});

describe("extractFetchResult", () => {
  test("reads direct events/next_seq/dropped", () => {
    const r = extractFetchResult({
      events: [{ seq: 1, event: { kind: "FileCreated", path: "foo.md" } }],
      next_seq: 2,
      dropped: 0,
    });
    expect(r).toEqual({
      events: [{ seq: 1, event: { kind: "FileCreated", path: "foo.md" } }],
      nextSeq: 2,
      dropped: 0,
    });
  });

  test("unwraps MCP content envelope", () => {
    const raw = {
      content: [
        {
          type: "text",
          text: JSON.stringify({
            events: [],
            next_seq: 42,
            dropped: 3,
          }),
        },
      ],
    };
    const r = extractFetchResult(raw);
    expect(r).toEqual({ events: [], nextSeq: 42, dropped: 3 });
  });

  test("defaults dropped to 0 when absent", () => {
    expect(extractFetchResult({ events: [], next_seq: 1 })).toEqual({
      events: [],
      nextSeq: 1,
      dropped: 0,
    });
  });

  test("returns null on malformed payloads", () => {
    expect(extractFetchResult({})).toBeNull();
    expect(extractFetchResult({ events: [] })).toBeNull();
    expect(extractFetchResult({ events: "wrong", next_seq: 1 })).toBeNull();
  });
});

describe("normaliseVaultEvent", () => {
  test("reads the pull-API nested shape", () => {
    expect(
      normaliseVaultEvent({
        seq: 7,
        event: { kind: "FileCreated", path: "a.md" },
      })
    ).toEqual({ kind: "FileCreated", path: "a.md" });
  });

  test("uses `from` for renames (pull-API field)", () => {
    expect(
      normaliseVaultEvent({
        seq: 3,
        event: { kind: "FileRenamed", path: "new.md", from: "old.md" },
      })
    ).toEqual({ kind: "FileRenamed", path: "new.md", oldPath: "old.md" });
  });

  test("falls back to flat shape (legacy WS payloads)", () => {
    expect(
      normaliseVaultEvent({ kind: "FileModified", path: "b.md" })
    ).toEqual({ kind: "FileModified", path: "b.md" });
  });

  test("rejects unknown kinds", () => {
    expect(
      normaliseVaultEvent({ seq: 1, event: { kind: "Explode", path: "boom.md" } })
    ).toBeNull();
  });
});

describe("coerceToHttpUrl", () => {
  test("rewrites ws:// to http://", () => {
    expect(coerceToHttpUrl("ws://localhost:8080/mcp/ws")).toBe(
      "http://localhost:8080/mcp"
    );
  });

  test("rewrites wss:// to https://", () => {
    expect(coerceToHttpUrl("wss://example.com/mcp/ws")).toBe(
      "https://example.com/mcp"
    );
  });

  test("passes http(s) URLs through", () => {
    expect(coerceToHttpUrl("http://localhost:8080/mcp")).toBe(
      "http://localhost:8080/mcp"
    );
  });

  test("trims whitespace", () => {
    expect(coerceToHttpUrl("  ws://localhost:8080/mcp/ws  ")).toBe(
      "http://localhost:8080/mcp"
    );
  });
});

// ── end-to-end client tests ───────────────────────────────────────────

type FetchCall = {
  url: string;
  body: Record<string, unknown>;
  headers: Record<string, string>;
  signal: AbortSignal | undefined;
};

function buildClient(opts: {
  nextResponses: Array<ResponseLike | (() => ResponseLike | Promise<ResponseLike>)>;
  dispatcherGlob?: string;
  endpointUrl?: string;
}): {
  plugin: PluginStub;
  calls: FetchCall[];
  events: VaultEvent[];
  client: VaultEventsClient;
  lifetime: AbortController;
  originalFetch: typeof globalThis.fetch;
} {
  const calls: FetchCall[] = [];
  const events: VaultEvent[] = [];
  const lifetime = new AbortController();
  const plugin: PluginStub = {
    settings: {
      apiRouterPort: 8080,
      nlrRoot: "",
      subscription: {
        enabled: true,
        endpointUrl: opts.endpointUrl ?? "http://localhost:8080/mcp",
      },
      dispatcher: {
        watchGlob: opts.dispatcherGlob ?? "00-neuro-link/*.md",
      },
    },
    lifetimeSignal: lifetime.signal,
  };

  const originalFetch = globalThis.fetch;
  let i = 0;
  globalThis.fetch = (async (input: unknown, init?: RequestInit) => {
    const url = typeof input === "string" ? input : String(input);
    const parsedBody =
      init?.body && typeof init.body === "string"
        ? (JSON.parse(init.body) as Record<string, unknown>)
        : {};
    const hdrs: Record<string, string> = {};
    if (init?.headers) {
      for (const [k, v] of Object.entries(init.headers as Record<string, string>)) {
        hdrs[k] = v;
      }
    }
    calls.push({
      url,
      body: parsedBody,
      headers: hdrs,
      signal: init?.signal as AbortSignal | undefined,
    });

    // Always succeed on unsubscribe so shutdown can complete without
    // custom wiring in every test. Assertions about unsubscribe rely on
    // the recorded `calls` array.
    const params = parsedBody.params as { name?: string } | undefined;
    if (params?.name === "unsubscribe_vault_events") {
      return materialisedToResponse(
        {
          status: 200,
          json: { jsonrpc: "2.0", id: parsedBody.id ?? 0, result: { removed: true } },
        },
        init?.signal as AbortSignal | undefined
      );
    }

    const scripted = opts.nextResponses[i];
    i++;
    const signal = init?.signal as AbortSignal | undefined;
    // Once scripted responses are exhausted, park here until the client
    // aborts — mimics the server's long-poll blocking so we don't spin.
    if (scripted === undefined) {
      await new Promise<void>((resolve) => {
        if (!signal) return resolve();
        if (signal.aborted) return resolve();
        signal.addEventListener("abort", () => resolve(), { once: true });
      });
      const err = new Error("aborted") as Error & { name: string };
      err.name = "AbortError";
      throw err;
    }
    const materialised =
      typeof scripted === "function" ? await (scripted as () => ResponseLike | Promise<ResponseLike>)() : scripted;
    return materialisedToResponse(materialised, signal);
  }) as typeof globalThis.fetch;

  const client = new VaultEventsClient(plugin as unknown as never, async (e) => {
    events.push(e);
  });

  return { plugin, calls, events, client, lifetime, originalFetch };
}

type ResponseLike =
  | {
      status: number;
      json?: unknown;
      /** If set, the mock rejects with this error (simulates network failure). */
      networkError?: Error;
    };

function materialisedToResponse(
  r: ResponseLike,
  signal: AbortSignal | undefined
): Response {
  if (r.networkError) {
    // Simulate fetch() rejecting before returning a Response.
    throw r.networkError;
  }
  if (signal?.aborted) {
    const err = new Error("aborted") as Error & { name: string };
    err.name = "AbortError";
    throw err;
  }
  const bodyStr = JSON.stringify(r.json ?? {});
  return new Response(bodyStr, {
    status: r.status,
    headers: { "Content-Type": "application/json" },
  });
}

let lastFetch: typeof globalThis.fetch;
beforeEach(() => {
  lastFetch = globalThis.fetch;
});
afterEach(() => {
  globalThis.fetch = lastFetch;
});

describe("VaultEventsClient.subscribe", () => {
  test("test_subscribe_succeeds: POST body shape + handle stored", async () => {
    const { client, calls } = buildClient({
      nextResponses: [
        {
          status: 200,
          json: {
            jsonrpc: "2.0",
            id: 1,
            result: { handle: "sub-42", created_at: "2026-04-17T00:00:00Z" },
          },
        },
        // Subsequent long-poll calls park on abort in the mock.
      ],
    });

    await client.connect();
    // Wait for subscribe + first long-poll to be issued.
    await waitForFetchCalls(calls, 2);
    await client.disconnect();

    const subCall = calls[0];
    expect(subCall.url).toBe("http://localhost:8080/mcp");
    expect(subCall.body.method).toBe("tools/call");
    const params = subCall.body.params as { name: string; arguments: unknown };
    expect(params.name).toBe("subscribe_vault_events");
    expect(params.arguments).toEqual({
      filter: { globs: ["00-neuro-link/*.md"], kinds: ["FileCreated"] },
    });

    expect(subCall.headers["Content-Type"]).toBe("application/json");
    // No token — nlrRoot empty — so Authorization header is absent.
    expect(subCall.headers.Authorization).toBeUndefined();

    // Client internally stored the handle; verify via first fetch call.
    const fetchCall = calls.find(
      (c) => (c.body.params as { name?: string }).name === "fetch_vault_events"
    );
    expect(fetchCall).toBeDefined();
    const fetchArgs = (fetchCall!.body.params as { arguments: { handle: string } })
      .arguments;
    expect(fetchArgs.handle).toBe("sub-42");
  });
});

describe("VaultEventsClient.longPoll", () => {
  test("test_long_poll_loop_emits_events: two events emitted in order", async () => {
    const { client, events } = buildClient({
      nextResponses: [
        {
          status: 200,
          json: {
            jsonrpc: "2.0",
            id: 1,
            result: { handle: "h", created_at: null },
          },
        },
        {
          status: 200,
          json: {
            jsonrpc: "2.0",
            id: 2,
            result: {
              events: [
                { seq: 1, event: { kind: "FileCreated", path: "one.md" } },
                { seq: 2, event: { kind: "FileCreated", path: "two.md" } },
              ],
              next_seq: 2,
              dropped: 0,
            },
          },
        },
        // Subsequent fetches park on abort.
      ],
    });

    await client.connect();
    await waitForEvents(events, 2);
    await client.disconnect();

    const fileEvents = events.filter((e) => e.kind === "FileCreated");
    expect(fileEvents.map((e) => e.path)).toEqual(["one.md", "two.md"]);
  });

  test("test_long_poll_respects_since_seq: cursor threads across calls", async () => {
    const { client, calls } = buildClient({
      nextResponses: [
        {
          status: 200,
          json: {
            jsonrpc: "2.0",
            id: 1,
            result: { handle: "h", created_at: null },
          },
        },
        {
          status: 200,
          json: {
            jsonrpc: "2.0",
            id: 2,
            result: {
              events: [{ seq: 10, event: { kind: "FileCreated", path: "a.md" } }],
              next_seq: 10,
              dropped: 0,
            },
          },
        },
        {
          status: 200,
          json: {
            jsonrpc: "2.0",
            id: 3,
            result: {
              events: [{ seq: 17, event: { kind: "FileCreated", path: "b.md" } }],
              next_seq: 17,
              dropped: 0,
            },
          },
        },
        // Then park.
      ],
    });

    await client.connect();
    // Wait for subscribe + 2 fetches + 1 parked call = 4 total.
    await waitForFetchCalls(calls, 4);
    await client.disconnect();

    const fetchCalls = calls.filter(
      (c) => (c.body.params as { name?: string }).name === "fetch_vault_events"
    );
    expect(fetchCalls.length).toBeGreaterThanOrEqual(2);
    const firstArgs = (fetchCalls[0].body.params as { arguments: { since_seq: number } })
      .arguments;
    const secondArgs = (fetchCalls[1].body.params as { arguments: { since_seq: number } })
      .arguments;
    // First fetch opens at 0; second must use the server-returned next_seq.
    expect(firstArgs.since_seq).toBe(0);
    expect(secondArgs.since_seq).toBe(10);
  });

  test("test_overflow_event_on_dropped: synthetic Overflow when dropped > 0", async () => {
    const { client, events } = buildClient({
      nextResponses: [
        {
          status: 200,
          json: {
            jsonrpc: "2.0",
            id: 1,
            result: { handle: "h", created_at: null },
          },
        },
        {
          status: 200,
          json: {
            jsonrpc: "2.0",
            id: 2,
            result: {
              events: [{ seq: 5, event: { kind: "FileCreated", path: "c.md" } }],
              next_seq: 5,
              dropped: 4,
            },
          },
        },
        // Then park.
      ],
    });

    await client.connect();
    await waitForEvents(events, 2); // Overflow + FileCreated
    await client.disconnect();

    const overflow = events.find((e) => e.kind === "Overflow" && e.path === "<overflow>");
    expect(overflow).toBeDefined();
    expect(overflow!.droppedCount).toBe(4);
    const created = events.find((e) => e.kind === "FileCreated" && e.path === "c.md");
    expect(created).toBeDefined();
  });
});

describe("VaultEventsClient.errorHandling", () => {
  test("test_auth_error_terminal: 401 during subscribe → terminal, no retry", async () => {
    const { client, calls, events } = buildClient({
      nextResponses: [
        { status: 401, json: {} },
        // Scripted only once — if the client retried we'd hit the parked
        // branch and the test would time out on the disconnect instead of
        // returning cleanly.
      ],
    });

    await client.connect();
    // Give the loop a generous window to (incorrectly) retry — if it were
    // going to.
    await new Promise((r) => setTimeout(r, 100));
    await client.disconnect();

    // Exactly one subscribe attempt; no fetch issued.
    expect(calls.length).toBe(1);
    expect(
      (calls[0].body.params as { name: string }).name
    ).toBe("subscribe_vault_events");

    // Terminal marker emitted so consumers can render dead state.
    const terminal = events.find(
      (e) => e.kind === "Overflow" && e.path === "<terminal>"
    );
    expect(terminal).toBeDefined();
  });

  test("test_transient_error_exponential_backoff: first retry ~1s", async () => {
    const { client, calls } = buildClient({
      nextResponses: [
        {
          status: 200,
          json: {
            jsonrpc: "2.0",
            id: 1,
            result: { handle: "h", created_at: null },
          },
        },
        // Transient failure — HTTP 502.
        { status: 502, json: {} },
        // Recovery — empty page.
        {
          status: 200,
          json: {
            jsonrpc: "2.0",
            id: 3,
            result: { events: [], next_seq: 0, dropped: 0 },
          },
        },
        // Subsequent calls park.
      ],
    });

    const started = Date.now();
    await client.connect();
    // Wait until we've seen subscribe + failed fetch + recovery fetch + next parked = 4 calls.
    await waitForFetchCalls(calls, 4);
    const elapsed = Date.now() - started;
    await client.disconnect();

    // Expect at least the first-step backoff (1000 ms) between the failing
    // fetch and the retry. Allow generous upper bound for CI jitter.
    expect(elapsed).toBeGreaterThanOrEqual(900);
    expect(elapsed).toBeLessThan(5_000);
  });

  test("test_abort_during_long_poll: abort signals the in-flight fetch", async () => {
    // The mock fetch below resolves only when the abort signal fires, so
    // the subscribe call doesn't complete naturally — the only way the
    // test exits the `connect()` await is if disconnect() aborts it.
    const { client, calls, plugin } = buildClient({
      nextResponses: [
        // subscribe hangs until abort signal.
        () =>
          new Promise<ResponseLike>((resolve, reject) => {
            // Wait a tick so `calls[0]` is populated with the AbortSignal.
            setTimeout(() => {
              const c = calls[0];
              if (!c?.signal) {
                resolve({ status: 200, json: { jsonrpc: "2.0", id: 1, result: {} } });
                return;
              }
              c.signal.addEventListener("abort", () => {
                const err = new Error("aborted") as Error & { name: string };
                err.name = "AbortError";
                reject(err);
              }, { once: true });
            }, 5);
          }),
      ],
    });

    const connectPromise = client.connect();
    // Give the subscribe call a moment to be issued.
    await new Promise((r) => setTimeout(r, 20));
    const disc = client.disconnect();
    await connectPromise;
    await disc;

    expect(calls.length).toBe(1);
    expect((calls[0].body.params as { name: string }).name).toBe(
      "subscribe_vault_events"
    );
    void plugin; // silence unused var lint
  });

  test("test_unsubscribe_on_shutdown: clean teardown calls unsubscribe", async () => {
    const { client, calls } = buildClient({
      nextResponses: [
        {
          status: 200,
          json: {
            jsonrpc: "2.0",
            id: 1,
            result: { handle: "sub-shutdown", created_at: null },
          },
        },
        // Subscribe resolves; next long-poll parks on abort. The mock
        // also auto-responds to unsubscribe_vault_events.
      ],
    });

    await client.connect();
    await waitForFetchCalls(calls, 2); // subscribe + parked long-poll
    await client.disconnect();

    const unsub = calls.find(
      (c) => (c.body.params as { name?: string }).name === "unsubscribe_vault_events"
    );
    expect(unsub).toBeDefined();
    const args = (unsub!.body.params as { arguments: { handle: string } }).arguments;
    expect(args.handle).toBe("sub-shutdown");
  });
});

// ── utility helpers ───────────────────────────────────────────────────

interface PluginStub {
  settings: {
    apiRouterPort: number;
    nlrRoot: string;
    subscription: { enabled: boolean; endpointUrl: string };
    dispatcher: { watchGlob: string };
  };
  lifetimeSignal: AbortSignal;
}

/** Resolve after the microtask queue drains a few times. */
async function flushMicrotasks(): Promise<void> {
  for (let i = 0; i < 5; i++) {
    await new Promise((r) => setTimeout(r, 0));
  }
}

async function waitForEvents(events: VaultEvent[], n: number, timeoutMs = 2_000): Promise<void> {
  const start = Date.now();
  while (events.length < n) {
    if (Date.now() - start > timeoutMs) {
      throw new Error(
        `timed out waiting for ${n} events (saw ${events.length}: ${JSON.stringify(events)})`
      );
    }
    await new Promise((r) => setTimeout(r, 10));
  }
}

async function waitForFetchCalls(
  calls: FetchCall[],
  n: number,
  timeoutMs = 3_000
): Promise<void> {
  const start = Date.now();
  while (calls.length < n) {
    if (Date.now() - start > timeoutMs) {
      throw new Error(
        `timed out waiting for ${n} fetch calls (saw ${calls.length})`
      );
    }
    await new Promise((r) => setTimeout(r, 10));
  }
}
