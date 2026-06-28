/**
 * MCP vault-events client — HTTP long-poll pull transport.
 *
 * Drives the TurboVault pull API (merged in ahuserious/turbovault PR #3):
 *
 *   subscribe_vault_events(filter)         -> { handle, created_at }
 *   fetch_vault_events(handle, since_seq?, timeout_ms?, max_events?)
 *                                          -> { events, next_seq, dropped }
 *   unsubscribe_vault_events(handle)       -> { removed }
 *
 * Lifecycle:
 *   connect()            — subscribe once, spawn the long-poll loop.
 *   disconnect()         — best-effort unsubscribe + stop looping.
 *
 * This replaces src/mcp-subscription.ts (WebSocket push) per the PR #27
 * adversarial review. The WS-specific blockers resolved by this swap:
 *
 *   • Bearer-token-in-query-param (blocker #1) — HTTP header auth.
 *   • Auth-failure reconnect storm (blocker #2) — 401/403 are terminal.
 *   • Unbounded pendingRequests map (should-fix) — no in-flight map; every
 *     fetch is a single awaited POST.
 *   • Timer leaks on disconnect — the long-poll is driven by an
 *     `AbortController` and `AbortSignal`, no stray setTimeouts.
 */

import type NLRPlugin from "./main";
import * as fs from "fs";
import * as path from "path";

export type VaultEventKind =
  | "FileCreated"
  | "FileModified"
  | "FileDeleted"
  | "FileRenamed"
  /**
   * Synthetic event emitted when the server reports `dropped > 0` — the
   * dispatcher's reaction is to log + optionally re-scan. Not produced
   * by the server directly.
   */
  | "Overflow";

export interface VaultEvent {
  kind: VaultEventKind;
  /** For `Overflow`, `path` holds a synthetic marker (`"<overflow>"`). */
  path: string;
  /** Present on FileRenamed — the previous path. */
  oldPath?: string;
  /** Optional epoch ms from the server. */
  timestamp?: number;
  /** On `Overflow`, how many events the server dropped. */
  droppedCount?: number;
}

type EventHandler = (event: VaultEvent) => void | Promise<void>;

interface JSONRPCRequest {
  jsonrpc: "2.0";
  id: number;
  method: string;
  params: unknown;
}

interface JSONRPCResponse {
  jsonrpc: "2.0";
  id: number;
  result?: unknown;
  error?: { code: number; message: string; data?: unknown };
}

/** Exponential backoff schedule from the spec: 1s, 2s, 4s, 16s, cap 30s. */
const BACKOFF_SCHEDULE_MS = [1_000, 2_000, 4_000, 16_000, 30_000];
/** Long-poll timeout requested from the server. */
const LONG_POLL_TIMEOUT_MS = 15_000;
/** Events per page. Matches the server-side default. */
const MAX_EVENTS_PER_POLL = 256;
/**
 * HTTP `fetch` timeout budget. Server advertises ~15s long-poll plus
 * round-trip — we need a floor above that to avoid false transients.
 */
const FETCH_TIMEOUT_MS = LONG_POLL_TIMEOUT_MS + 10_000;

export class VaultEventsClient {
  private plugin: NLRPlugin;
  private handler: EventHandler;
  private handle: string | null = null;
  private createdAt: string | null = null;
  /** Sequence cursor passed as `since_seq` on the next fetch. */
  private sinceSeq = 0;
  /**
   * Local request-id counter; every HTTP POST is independent, so this is
   * purely for diagnostics in server logs — we don't keep a pending-request
   * map.
   */
  private nextRequestId = 1;
  /** Scheduled for the long-poll loop. `running === false` means stopped. */
  private running = false;
  /** Abort controller bound to the current in-flight `fetch`. */
  private inflight: AbortController | null = null;
  /** External disconnect latch. */
  private stopped = false;
  /**
   * Terminal-state flag — set when we hit a non-recoverable condition
   * (auth failure, subscribe 404, etc.). Emits once so consumers can
   * distinguish "server unreachable, will retry" from "will never
   * reconnect without operator action".
   */
  private terminated = false;
  /** Promise that resolves when the long-poll loop exits — for tests. */
  private loopDonePromise: Promise<void> | null = null;

  constructor(plugin: NLRPlugin, handler: EventHandler) {
    this.plugin = plugin;
    this.handler = handler;
    // Bind to plugin lifetime so unload triggers a clean teardown.
    plugin.lifetimeSignal.addEventListener(
      "abort",
      () => {
        void this.disconnect();
      },
      { once: true }
    );
  }

  /**
   * Subscribe once, then drive the long-poll loop. Returns after the
   * subscribe call resolves (or fails) — the loop runs in the background.
   */
  async connect(): Promise<void> {
    if (this.stopped || this.terminated) return;

    try {
      await this.subscribe();
    } catch (e) {
      const err = e as Error & { code?: number; status?: number };
      if (isAuthError(err)) {
        this.terminate(`subscribe auth-rejected: ${err.message}`);
        return;
      }
      console.warn(`NLR vault-events: subscribe failed — ${err.message}`);
      // Transient — schedule a retry using the backoff ladder, starting
      // from the top. We piggy-back on the same long-poll loop: kick it
      // off so it handles the retry backoff uniformly.
    }

    this.running = true;
    this.loopDonePromise = this.longPollLoop();
    // Errors inside the loop are handled there; surface nothing here.
    this.loopDonePromise.catch((e: unknown) => {
      const err = e as Error;
      console.warn(`NLR vault-events: loop crashed — ${err.message}`);
    });
  }

  async disconnect(): Promise<void> {
    if (this.stopped) return;
    this.stopped = true;
    this.running = false;
    // Cancel any in-flight long-poll so the loop wakes up promptly.
    if (this.inflight) {
      try {
        this.inflight.abort();
      } catch {
        /* already settled */
      }
      this.inflight = null;
    }
    // Wait for the loop to exit before sending the unsubscribe — otherwise
    // we'd race the final fetch and potentially fire unsubscribe while a
    // fetch is still being processed by the server.
    if (this.loopDonePromise) {
      try {
        await this.loopDonePromise;
      } catch {
        /* swallowed; loop already logs */
      }
    }
    // Best-effort unsubscribe. A short timeout keeps shutdown snappy even
    // if the server is unreachable.
    if (this.handle) {
      try {
        await this.rpc(
          "unsubscribe_vault_events",
          { handle: this.handle },
          { timeoutMs: 2_000, retryOnAuth: false }
        );
      } catch (e) {
        // Failure during teardown is expected (server may already be down);
        // log at debug level only.
        console.debug("NLR vault-events: unsubscribe failed during shutdown", e);
      }
      this.handle = null;
    }
  }

  // ── internal: subscribe + long-poll loop ───────────────────────────────

  private async subscribe(): Promise<void> {
    const watchGlob = this.plugin.settings.dispatcher.watchGlob;
    const result = (await this.rpc("subscribe_vault_events", {
      filter: {
        globs: [watchGlob],
        kinds: ["FileCreated"],
      },
    })) as unknown;
    const parsed = extractSubscribeResult(result);
    if (!parsed) {
      throw new Error("subscribe_vault_events returned no handle");
    }
    this.handle = parsed.handle;
    this.createdAt = parsed.createdAt;
    this.sinceSeq = 0;
  }

  /**
   * Main loop. Each iteration:
   *   1. Call fetch_vault_events with the current cursor.
   *   2. On success: reset backoff, emit events, bump `since_seq`.
   *   3. On transient error: sleep per backoff ladder.
   *   4. On auth/terminal error: bail.
   * Exits when `running === false` (disconnect or terminate).
   */
  private async longPollLoop(): Promise<void> {
    let backoffIdx = 0;
    while (this.running && !this.stopped && !this.terminated) {
      // If subscribe failed initially, `handle` is null — try to recover
      // before attempting a fetch. This folds subscribe-retry into the
      // same backoff ladder as fetch-retry.
      if (!this.handle) {
        try {
          await this.subscribe();
          backoffIdx = 0;
        } catch (e) {
          const err = e as Error & { code?: number; status?: number };
          if (isAuthError(err)) {
            this.terminate(`subscribe auth-rejected: ${err.message}`);
            return;
          }
          await this.sleepOrAbort(this.backoffMsAt(backoffIdx));
          backoffIdx = Math.min(backoffIdx + 1, BACKOFF_SCHEDULE_MS.length - 1);
          continue;
        }
      }

      try {
        const page = await this.fetchPage();
        // Reset backoff on any successful fetch (even an empty one).
        backoffIdx = 0;
        if (!page) continue; // aborted mid-flight; loop header will exit.

        if (page.dropped > 0) {
          await this.emit({
            kind: "Overflow",
            path: "<overflow>",
            droppedCount: page.dropped,
          });
        }

        for (const entry of page.events) {
          const evt = normaliseVaultEvent(entry);
          if (!evt) continue;
          await this.emit(evt);
          if (!this.running) return;
        }

        this.sinceSeq = page.nextSeq;
      } catch (e) {
        if (this.stopped || this.terminated) return;
        const err = e as Error & { code?: number; status?: number };
        if (isAuthError(err)) {
          this.terminate(`fetch auth-rejected: ${err.message}`);
          return;
        }
        // Transient — back off and retry.
        console.warn(`NLR vault-events: fetch failed (transient) — ${err.message}`);
        await this.sleepOrAbort(this.backoffMsAt(backoffIdx));
        backoffIdx = Math.min(backoffIdx + 1, BACKOFF_SCHEDULE_MS.length - 1);
      }
    }
  }

  private async fetchPage(): Promise<FetchPage | null> {
    if (!this.handle) throw new Error("no subscription handle");
    const result = (await this.rpc("fetch_vault_events", {
      handle: this.handle,
      since_seq: this.sinceSeq,
      timeout_ms: LONG_POLL_TIMEOUT_MS,
      max_events: MAX_EVENTS_PER_POLL,
    })) as unknown;
    return extractFetchResult(result);
  }

  private backoffMsAt(idx: number): number {
    const i = Math.max(0, Math.min(idx, BACKOFF_SCHEDULE_MS.length - 1));
    return BACKOFF_SCHEDULE_MS[i];
  }

  /**
   * Sleep that wakes up immediately on disconnect. Used between retries.
   * We deliberately share the abort controller pattern with `fetch` so
   * a single `disconnect()` call cancels both.
   */
  private async sleepOrAbort(ms: number): Promise<void> {
    if (this.stopped || this.terminated) return;
    return new Promise((resolve) => {
      const timer = setTimeout(() => {
        this.plugin.lifetimeSignal.removeEventListener("abort", onAbort);
        resolve();
      }, ms);
      const onAbort = (): void => {
        clearTimeout(timer);
        resolve();
      };
      this.plugin.lifetimeSignal.addEventListener("abort", onAbort, {
        once: true,
      });
    });
  }

  /** Emit once; swallow handler errors (they shouldn't crash the loop). */
  private async emit(event: VaultEvent): Promise<void> {
    try {
      await Promise.resolve(this.handler(event));
    } catch (e) {
      const err = e as Error;
      console.warn("NLR vault-events: handler error", err.message);
    }
  }

  private terminate(reason: string): void {
    if (this.terminated) return;
    this.terminated = true;
    this.running = false;
    if (this.inflight) {
      try {
        this.inflight.abort();
      } catch {
        /* already settled */
      }
      this.inflight = null;
    }
    console.warn(`NLR vault-events: terminal — ${reason}`);
    // Emit a synthetic event so the dispatcher can mark the subscription
    // as dead in UI without a separate callback.
    void this.emit({
      kind: "Overflow",
      path: "<terminal>",
      droppedCount: -1,
    });
  }

  /**
   * POST an MCP `tools/call` to the configured endpoint. Returns the
   * decoded `result` block. Throws on HTTP / JSON-RPC errors.
   */
  private async rpc(
    toolName: string,
    args: unknown,
    opts?: { timeoutMs?: number; retryOnAuth?: boolean }
  ): Promise<unknown> {
    const url = this.resolveEndpointUrl();
    const token = this.readBearerToken();
    const id = this.nextRequestId++;
    const body: JSONRPCRequest = {
      jsonrpc: "2.0",
      id,
      method: "tools/call",
      params: { name: toolName, arguments: args },
    };

    const headers: Record<string, string> = {
      "Content-Type": "application/json",
      Accept: "application/json",
    };
    if (token) {
      headers["Authorization"] = `Bearer ${token}`;
    }

    const controller = new AbortController();
    this.inflight = controller;
    const timeoutMs = opts?.timeoutMs ?? FETCH_TIMEOUT_MS;
    const timeoutTimer = setTimeout(() => {
      controller.abort();
    }, timeoutMs);

    try {
      const resp = await fetch(url, {
        method: "POST",
        headers,
        body: JSON.stringify(body),
        signal: controller.signal,
      });

      if (resp.status === 401 || resp.status === 403) {
        const err = new Error(`HTTP ${resp.status}`) as Error & {
          status: number;
        };
        err.status = resp.status;
        throw err;
      }
      if (!resp.ok) {
        throw new Error(`HTTP ${resp.status}`);
      }

      const parsed = (await resp.json()) as JSONRPCResponse;
      if (parsed.error) {
        const err = new Error(parsed.error.message) as Error & { code: number };
        err.code = parsed.error.code;
        throw err;
      }
      // MCP tools/call wraps the actual tool result inside a `content[]`
      // envelope. The extractors below know how to unwrap both shapes.
      return parsed.result;
    } finally {
      clearTimeout(timeoutTimer);
      // Only clear inflight if it's still ours — disconnect() may have
      // already cleared it.
      if (this.inflight === controller) this.inflight = null;
    }
  }

  private resolveEndpointUrl(): string {
    // `migrateSettings` handles the `wsUrl` → `endpointUrl` rename and
    // the `ws(s)://` → `http(s)://` rewrite. `coerceToHttpUrl` is
    // belt-and-braces for users who hand-edit the data.json between
    // releases and skip the migration path.
    const configured = this.plugin.settings.subscription.endpointUrl;
    if (configured) return coerceToHttpUrl(configured);
    const port = this.plugin.settings.apiRouterPort || 8080;
    return `http://localhost:${port}/mcp`;
  }

  private readBearerToken(): string {
    // Source of truth: secrets/.env — same NLR_API_TOKEN the rest of the
    // plugin uses. See mcp-setup.ts for the canonical pattern.
    const nlrRoot = this.plugin.settings.nlrRoot;
    if (!nlrRoot) return "";
    try {
      const envPath = path.join(nlrRoot, "secrets", ".env");
      if (!fs.existsSync(envPath)) return "";
      const content = fs.readFileSync(envPath, "utf-8");
      const match = content.match(/NLR_API_TOKEN=(.+)/);
      return match ? match[1].trim() : "";
    } catch {
      return "";
    }
  }
}

// ── helpers (exported for tests) ───────────────────────────────────────

interface FetchPage {
  events: Array<{ seq: number; event: unknown }>;
  nextSeq: number;
  dropped: number;
}

export function extractSubscribeResult(
  result: unknown
): { handle: string; createdAt: string | null } | null {
  if (!result || typeof result !== "object") return null;

  // Direct shape: { handle, created_at }
  const direct = result as { handle?: unknown; created_at?: unknown };
  if (typeof direct.handle === "string") {
    return {
      handle: direct.handle,
      createdAt: typeof direct.created_at === "string" ? direct.created_at : null,
    };
  }

  // MCP envelope: { content: [{ type: "text", text: "<json>" }] }
  const content = (result as { content?: Array<{ text?: string }> }).content;
  if (Array.isArray(content)) {
    for (const block of content) {
      if (typeof block?.text !== "string") continue;
      try {
        const parsed = JSON.parse(block.text) as {
          handle?: unknown;
          created_at?: unknown;
        };
        if (typeof parsed.handle === "string") {
          return {
            handle: parsed.handle,
            createdAt:
              typeof parsed.created_at === "string" ? parsed.created_at : null,
          };
        }
      } catch {
        /* not JSON — try next block */
      }
    }
  }
  return null;
}

export function extractFetchResult(result: unknown): FetchPage | null {
  if (!result || typeof result !== "object") return null;

  const pickFromObject = (o: Record<string, unknown>): FetchPage | null => {
    const events = o.events;
    const nextSeq = o.next_seq;
    const dropped = o.dropped;
    if (!Array.isArray(events)) return null;
    if (typeof nextSeq !== "number") return null;
    return {
      events: events as Array<{ seq: number; event: unknown }>,
      nextSeq,
      dropped: typeof dropped === "number" ? dropped : 0,
    };
  };

  const direct = pickFromObject(result as Record<string, unknown>);
  if (direct) return direct;

  // MCP envelope.
  const content = (result as { content?: Array<{ text?: string }> }).content;
  if (Array.isArray(content)) {
    for (const block of content) {
      if (typeof block?.text !== "string") continue;
      try {
        const parsed = JSON.parse(block.text) as Record<string, unknown>;
        const inner = pickFromObject(parsed);
        if (inner) return inner;
      } catch {
        /* not JSON — try next block */
      }
    }
  }
  return null;
}

/**
 * The pull API nests the event under `.event`: each entry looks like
 *   { seq: <num>, event: { kind, path, from? } }
 * where `from` is the pre-rename path (vs. the WS push shape's `old_path`).
 * We also tolerate the flat shape for backwards-compat with tests that
 * crib the old WS-era payload.
 */
export function normaliseVaultEvent(raw: unknown): VaultEvent | null {
  if (!raw || typeof raw !== "object") return null;
  const entry = raw as Record<string, unknown>;

  // Pull-API shape: { seq, event: {...} }
  const nested = entry.event;
  const source = (nested && typeof nested === "object"
    ? (nested as Record<string, unknown>)
    : entry);

  const kind = source.kind ?? source.event_kind ?? source.type;
  const pathVal = source.path ?? source.file_path;
  if (typeof kind !== "string" || typeof pathVal !== "string") return null;
  if (
    kind !== "FileCreated" &&
    kind !== "FileModified" &&
    kind !== "FileDeleted" &&
    kind !== "FileRenamed"
  ) {
    return null;
  }

  const evt: VaultEvent = { kind, path: pathVal };
  // `from` is the pull-API field; `old_path`/`oldPath` is the legacy WS
  // shape. Preserve both so the dispatcher gets a uniform `oldPath`.
  const renamedFrom = source.from ?? source.old_path ?? source.oldPath;
  if (typeof renamedFrom === "string") evt.oldPath = renamedFrom;
  const ts = source.timestamp;
  if (typeof ts === "number") evt.timestamp = ts;
  return evt;
}

function isAuthError(err: { status?: number; code?: number }): boolean {
  if (err.status === 401 || err.status === 403) return true;
  // JSON-RPC -32001 is the conventional "unauthorized" code.
  if (err.code === -32001) return true;
  return false;
}

/**
 * Compat shim for the settings rename. Existing installs may still have
 * `ws://localhost:8080/mcp/ws` on disk; rewrite to the HTTP equivalent
 * so they keep working after the transport swap. If the path still ends
 * in `/mcp/ws`, trim the trailing `/ws` because the HTTP endpoint is
 * just `/mcp`.
 */
export function coerceToHttpUrl(raw: string): string {
  let url = raw.trim();
  if (url.startsWith("ws://")) url = "http://" + url.substring(5);
  else if (url.startsWith("wss://")) url = "https://" + url.substring(6);
  // Drop the `/ws` suffix that only WS transport needed.
  url = url.replace(/\/mcp\/ws(\/?)$/, "/mcp$1");
  return url;
}
