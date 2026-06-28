// SPDX-License-Identifier: MIT
//
// Clean-room. Not adapted from obsidian-copilot.
//
// Append-only trace logger for the @neuro agent. Every tool call (and its
// result, or the safety-refusal that stopped it) becomes one line in
// `04-Agent-Memory/logs.md`. The file format is human-friendly markdown
// bullets — not JSONL — so a user can open the file and read it without
// tooling. We still embed a single-line JSON envelope per entry so automated
// consumers can parse it back.
//
// Writes are serialised through a promise chain so two tool calls landing
// simultaneously can't interleave their lines.

import type { App, TFile } from "obsidian";

const DEFAULT_LOG_PATH = "04-Agent-Memory/logs.md";
const LOG_HEADER =
  "# Agent Memory Log\n\n" +
  "*Append-only record of @neuro tool calls. One entry per line. " +
  "Older entries never rewritten.*\n\n";

export interface TraceEntry {
  /** Identifier the agent used in its tool_call.id. */
  callId: string;
  tool: string;
  /** Raw JSON string from the model — we don't re-stringify. */
  arguments: string;
  /** "ok" | "refused" | "error" */
  outcome: "ok" | "refused" | "error";
  /** Short message (refusal reason or error summary). Truncated to 240 chars. */
  summary?: string;
  /** Optional conversation id so multi-agent sessions don't collide. */
  conversationId?: string;
  /** If the tool has a known reverse operation, show it here. */
  rollbackCommand?: string;
}

export interface TraceLogger {
  append(entry: TraceEntry): Promise<void>;
  /** Return the last N entries (newest first) for UI display. */
  tail(n: number): Promise<TraceEntry[]>;
}

/**
 * Real logger that writes into the vault. We go through the vault adapter
 * (not `fs`) so Obsidian's file index stays in sync.
 */
export class VaultTraceLogger implements TraceLogger {
  private app: App;
  private logPath: string;
  private chain: Promise<unknown> = Promise.resolve();

  constructor(app: App, logPath: string = DEFAULT_LOG_PATH) {
    this.app = app;
    this.logPath = logPath;
  }

  async append(entry: TraceEntry): Promise<void> {
    // Serialise to avoid interleaving. Each call waits on the previous one,
    // then appends its own line.
    const prev = this.chain;
    let resolve: () => void;
    const done = new Promise<void>((r) => {
      resolve = r;
    });
    this.chain = prev.then(() => done);
    try {
      await prev;
      await this.doAppend(entry);
    } finally {
      resolve!();
    }
  }

  async tail(n: number): Promise<TraceEntry[]> {
    const file = this.app.vault.getAbstractFileByPath(this.logPath);
    if (!file) return [];
    const tfile = file as TFile;
    const text = await this.app.vault.read(tfile);
    return parseTailLines(text, n);
  }

  private async doAppend(entry: TraceEntry): Promise<void> {
    const line = formatLine(entry);
    const existing = this.app.vault.getAbstractFileByPath(this.logPath);
    if (!existing) {
      // Ensure 04-Agent-Memory/ exists; swallow errors if it already does.
      const dir = this.logPath.split("/").slice(0, -1).join("/");
      if (dir) {
        try {
          await this.app.vault.createFolder(dir);
        } catch {
          /* folder exists */
        }
      }
      await this.app.vault.create(this.logPath, `${LOG_HEADER}${line}\n`);
      return;
    }
    const tfile = existing as TFile;
    const prev = await this.app.vault.read(tfile);
    const separator = prev.endsWith("\n") ? "" : "\n";
    await this.app.vault.modify(tfile, `${prev}${separator}${line}\n`);
  }
}

/**
 * In-memory logger — used by tests so they don't touch the vault.
 */
export class MemoryTraceLogger implements TraceLogger {
  readonly entries: TraceEntry[] = [];

  append(entry: TraceEntry): Promise<void> {
    this.entries.push(entry);
    return Promise.resolve();
  }

  tail(n: number): Promise<TraceEntry[]> {
    return Promise.resolve(this.entries.slice(-n).reverse());
  }
}

// ── helpers (exported for tests) ──────────────────────────────────────────

export function formatLine(entry: TraceEntry): string {
  const iso = new Date().toISOString();
  // Single-line JSON envelope (prefixed by `- `) so the file is still valid
  // markdown bullets. We truncate `arguments` to 400 chars to keep lines
  // readable; the full payload lives in the provider's raw response if
  // anyone needs it for postmortem.
  const truncatedArgs =
    entry.arguments.length > 400 ? `${entry.arguments.slice(0, 400)}…` : entry.arguments;
  const summary = entry.summary ? entry.summary.slice(0, 240) : undefined;
  const payload: Record<string, unknown> = {
    ts: iso,
    call_id: entry.callId,
    tool: entry.tool,
    outcome: entry.outcome,
    args: truncatedArgs,
  };
  if (summary) payload.summary = summary;
  if (entry.conversationId) payload.conv = entry.conversationId;
  if (entry.rollbackCommand) payload.rollback = entry.rollbackCommand;
  return `- ${JSON.stringify(payload)}`;
}

export function parseTailLines(text: string, n: number): TraceEntry[] {
  const lines = text.split("\n");
  const out: TraceEntry[] = [];
  // Walk backwards, skipping blanks and non-JSON bullets, until we have n.
  for (let i = lines.length - 1; i >= 0 && out.length < n; i--) {
    const line = lines[i].trim();
    if (!line.startsWith("- ")) continue;
    try {
      const raw = JSON.parse(line.slice(2)) as Record<string, unknown>;
      out.push({
        callId: String(raw.call_id ?? ""),
        tool: String(raw.tool ?? ""),
        arguments: String(raw.args ?? ""),
        outcome: (raw.outcome as TraceEntry["outcome"]) ?? "ok",
        summary: typeof raw.summary === "string" ? raw.summary : undefined,
        conversationId:
          typeof raw.conv === "string" ? raw.conv : undefined,
        rollbackCommand:
          typeof raw.rollback === "string" ? raw.rollback : undefined,
      });
    } catch {
      /* not a structured entry; skip */
    }
  }
  return out;
}
