/**
 * New-spec dispatcher.
 *
 * Fires when a file is created under `00-neuro-link/*.md` (TOP-LEVEL ONLY —
 * we explicitly skip `00-neuro-link/tasks/` so the task specs we emit don't
 * re-trigger the loop).
 *
 * Flow:
 *   1. Debounce 500ms after FileCreated so the editor has a chance to flush
 *      its trailing newline. This avoids reading a partial file before the
 *      user is done writing.
 *   2. Read frontmatter + body via the plugin's vault API.
 *   3. Render the prompt at `.claude/skills/neuro-link/prompts/new-spec-to-task.md`
 *      with the content substituted in. Fall back to a built-in prompt if
 *      that file doesn't exist (useful for first-run and tests).
 *   4. Call `plugin.llm.tool_use(...)` — a single call, the model emits the
 *      task spec JSON via a `emit_task_spec` tool. If tool_use isn't supported
 *      by the active provider we degrade to parsing the plain-text output.
 *   5. Write to `00-neuro-link/tasks/<slug>.md`, handling slug collisions by
 *      suffixing `-1`, `-2`, ... until a free name is found.
 */

import { Notice, TFile } from "obsidian";
import type NLRPlugin from "../main";
import type { VaultEvent } from "../mcp-vault-events";
import { LLMProviderError } from "../providers/base";
import * as fs from "fs";
import * as path from "path";
import {
  sanitiseSlug,
  validateSpec,
  renderTaskMarkdown,
  hashContent,
  FALLBACK_PROMPT,
  type TaskSpec,
} from "./new-spec-helpers";

export { sanitiseSlug, FALLBACK_PROMPT, type TaskSpec };

const TASK_EMIT_TOOL = "emit_task_spec";
/** Cap on stale-content retries before the dispatcher gives up. */
const MAX_STALE_RETRIES = 1;

export class NewSpecDispatcher {
  private plugin: NLRPlugin;
  private inflight = new Set<string>();
  private debounces = new Map<string, ReturnType<typeof setTimeout>>();
  /**
   * Serialises slug-selection + vault.create across all in-flight dispatches.
   * Prior to this lock, two parallel drops whose LLM-generated slugs
   * collided could both see the same base path as free, both call
   * `vault.create`, and race — the loser's error was swallowed as a warning
   * (see PR #26 adversarial review, should-fix #13). Holding the lock for
   * the existence-check + create window makes the suffix assignment
   * atomic from any concurrent dispatcher's point of view.
   */
  private writeChain: Promise<unknown> = Promise.resolve();

  constructor(plugin: NLRPlugin) {
    this.plugin = plugin;
  }

  /**
   * Runs `fn` under the write-lock. Other callers queue until the current
   * one resolves. Errors propagate to the caller — the chain keeps going
   * so a single bad dispatch doesn't permanently block the queue.
   */
  private async runWithWriteLock<T>(fn: () => Promise<T>): Promise<T> {
    const prev = this.writeChain;
    // `next` resolves after `fn()` settles; subsequent callers chain off
    // this promise regardless of success/failure.
    let resolve: () => void;
    const next = new Promise<void>((r) => {
      resolve = r;
    });
    this.writeChain = prev.then(() => next);
    try {
      await prev;
      return await fn();
    } finally {
      resolve!();
    }
  }

  /**
   * Cold-start catch-up scan. Walks `00-neuro-link/*.md` (top-level only)
   * for files modified in the last `lookbackMs` whose `source:` frontmatter
   * doesn't yet appear in any `00-neuro-link/tasks/*.md`, and queues each
   * one through `handle({kind: "FileCreated", ...})`.
   *
   * This covers the window between `plugin.onload` finishing and the
   * vault-event subscription actually connecting — during that 50-300 ms
   * (or longer, over network), a file dropped by the user is invisible
   * to the server-side subscription. The review (should-fix #9) notes
   * this becomes worse with the long-poll pull transport, so this
   * catch-up path is intentionally transport-agnostic: it reads the
   * vault directly rather than asking the server for missed events.
   *
   * Runs asynchronously; call-site fire-and-forgets. Errors are logged
   * per file and don't abort the whole scan.
   */
  async scanCatchUp(lookbackMs = 60_000): Promise<number> {
    if (!this.plugin.settings.dispatcher.enabled) return 0;

    const watchedPrefix = "00-neuro-link/";
    const taskDir = this.plugin.settings.dispatcher.taskOutputDir.replace(/\/$/, "");
    const cutoff = Date.now() - lookbackMs;

    // Collect source paths already registered in existing task specs so we
    // don't re-dispatch something the previous session already processed.
    const processedSources = new Set<string>();
    for (const file of this.plugin.app.vault.getMarkdownFiles()) {
      const filePath = file.path.replace(/\\/g, "/");
      if (!filePath.startsWith(`${taskDir}/`)) continue;
      try {
        const body = await this.plugin.app.vault.read(file);
        const match = body.match(/^source:\s*"([^"]+)"/m);
        if (match) processedSources.add(match[1]);
      } catch {
        /* skip unreadable task file */
      }
    }

    // Walk top-level .md files under the watched folder; queue any
    // recent + unprocessed file.
    let queued = 0;
    for (const file of this.plugin.app.vault.getMarkdownFiles()) {
      const filePath = file.path.replace(/\\/g, "/");
      if (!this.isWatchedPath(filePath)) continue;
      const mtime = file.stat?.mtime ?? 0;
      if (mtime < cutoff) continue;
      if (processedSources.has(filePath)) continue;

      queued++;
      this.handle({
        kind: "FileCreated",
        path: filePath,
        timestamp: mtime,
      });
    }

    if (queued > 0) {
      console.log(`NLR dispatcher: cold-start catch-up queued ${queued} file(s) from ${watchedPrefix}`);
    }
    return queued;
  }

  /**
   * Entry point called from the subscription. Also safe to call directly
   * from the Obsidian `vault.on("create", ...)` event as a backup path
   * (not wired by default — the MCP subscription is authoritative).
   */
  handle(event: VaultEvent): void {
    if (!this.plugin.settings.dispatcher.enabled) return;
    if (event.kind !== "FileCreated") return;
    if (!this.isWatchedPath(event.path)) return;

    const existing = this.debounces.get(event.path);
    if (existing) clearTimeout(existing);

    const timer = setTimeout(() => {
      this.debounces.delete(event.path);
      void this.process(event.path);
    }, this.plugin.settings.dispatcher.debounceMs);

    this.debounces.set(event.path, timer);

    // Cancel debounce on plugin unload.
    const onAbort = (): void => {
      clearTimeout(timer);
      this.debounces.delete(event.path);
    };
    if (this.plugin.lifetimeSignal.aborted) onAbort();
    else this.plugin.lifetimeSignal.addEventListener("abort", onAbort, { once: true });
  }

  /** True for `00-neuro-link/<file>.md` but not `00-neuro-link/tasks/<...>`. */
  private isWatchedPath(vaultPath: string): boolean {
    const normalised = vaultPath.replace(/\\/g, "/");
    if (!normalised.endsWith(".md")) return false;
    if (!normalised.startsWith("00-neuro-link/")) return false;
    const rest = normalised.substring("00-neuro-link/".length);
    // Reject anything under a subfolder — top-level only.
    return !rest.includes("/");
  }

  private async process(vaultPath: string, retryCount = 0): Promise<void> {
    if (this.inflight.has(vaultPath)) return;
    this.inflight.add(vaultPath);

    try {
      const content = await this.readWithTrailingNewlineCheck(vaultPath);
      if (content === null) {
        console.warn(`NLR dispatcher: skipped ${vaultPath} — file not settled within debounce window`);
        return;
      }

      // Hash the content we're about to feed to the LLM. The round-trip
      // can take 8-20 s; if the user edits the file mid-flight, the generated
      // spec will describe stale content. After the LLM returns we re-read
      // and compare — on mismatch we discard the output and re-queue once.
      // See PR #26 adversarial review, blocker #3.
      const contentHash = hashContent(content);

      const prompt = this.renderPrompt(vaultPath, content);
      const spec = await this.callLLM(prompt);
      if (!spec) return;

      // Re-read after the LLM round-trip and compare hashes. If the file
      // changed, the spec we hold is stale — throw it away.
      const currentContent = await this.readCurrent(vaultPath);
      if (currentContent === null) {
        console.warn(`NLR dispatcher: ${vaultPath} disappeared during LLM call — discarding spec`);
        return;
      }
      if (hashContent(currentContent) !== contentHash) {
        if (retryCount < MAX_STALE_RETRIES) {
          console.warn(`NLR dispatcher: ${vaultPath} edited mid-flight — re-queueing (attempt ${retryCount + 1})`);
          // Release the inflight lock via `finally` below, then re-dispatch.
          // We do the recursion *after* the finally so the second call can
          // acquire the lock fresh; queueMicrotask avoids deep recursion.
          queueMicrotask(() => {
            void this.process(vaultPath, retryCount + 1);
          });
          return;
        }
        console.warn(
          `NLR dispatcher: ${vaultPath} still edited after ${MAX_STALE_RETRIES} retry — giving up`
        );
        return;
      }

      const outputPath = await this.writeTaskSpec(vaultPath, spec);
      if (outputPath) {
        new Notice(`Task spec generated: ${outputPath}`);
      }
    } catch (e) {
      const err = e as Error;
      console.warn(`NLR dispatcher: ${vaultPath} failed — ${err.message}`);
      new Notice(`Task-spec generation failed for ${path.basename(vaultPath)} — check console`);
    } finally {
      this.inflight.delete(vaultPath);
    }
  }

  /**
   * Read the file's current content via the same path the pre-LLM read used,
   * but without the newline-stability retry — we only need a snapshot for
   * comparing hashes. Returns null if the file is gone.
   */
  private async readCurrent(vaultPath: string): Promise<string | null> {
    try {
      const file = this.plugin.app.vault.getAbstractFileByPath(vaultPath);
      if (file instanceof TFile) {
        return await this.plugin.app.vault.read(file);
      }
      const vaultBase = this.plugin.settings.vaultPath;
      if (!vaultBase) return null;
      const fsPath = path.join(vaultBase, vaultPath);
      if (!fs.existsSync(fsPath)) return null;
      return fs.readFileSync(fsPath, "utf-8");
    } catch {
      return null;
    }
  }

  /**
   * Read the file, retrying once if the final character isn't a newline
   * (cheap heuristic for "write probably still in flight"). Returns null
   * if still not newline-terminated after the retry window.
   */
  private async readWithTrailingNewlineCheck(vaultPath: string): Promise<string | null> {
    const tryRead = async (): Promise<string> => {
      const file = this.plugin.app.vault.getAbstractFileByPath(vaultPath);
      if (file instanceof TFile) {
        return await this.plugin.app.vault.read(file);
      }
      // File might not be registered with Obsidian yet — fall back to raw fs.
      const vaultBase = this.plugin.settings.vaultPath;
      if (!vaultBase) throw new Error("vault path not set");
      const fsPath = path.join(vaultBase, vaultPath);
      return fs.readFileSync(fsPath, "utf-8");
    };

    let content = await tryRead();
    if (content.endsWith("\n") || content.length === 0) return content;

    // One retry after another debounce window — same as the original debounce.
    await new Promise((r) => setTimeout(r, this.plugin.settings.dispatcher.debounceMs));
    content = await tryRead();
    return content.endsWith("\n") || content.length === 0 ? content : null;
  }

  private renderPrompt(vaultPath: string, content: string): string {
    const tmpl = this.loadPromptTemplate();
    return tmpl
      .replace(/\{\{\s*file_path\s*\}\}/g, vaultPath)
      .replace(/\{\{\s*content\s*\}\}/g, content);
  }

  private loadPromptTemplate(): string {
    const nlrRoot = this.plugin.settings.nlrRoot;
    if (nlrRoot) {
      const p = path.join(nlrRoot, ".claude", "skills", "neuro-link", "prompts", "new-spec-to-task.md");
      try {
        if (fs.existsSync(p)) return fs.readFileSync(p, "utf-8");
      } catch {
        /* fall through */
      }
    }
    return FALLBACK_PROMPT;
  }

  private async callLLM(prompt: string): Promise<TaskSpec | null> {
    const model = this.plugin.settings.dispatcher.model || this.plugin.llm.defaultModel();
    if (!model) {
      console.warn("NLR dispatcher: no model configured");
      return null;
    }

    const tools = [
      {
        name: TASK_EMIT_TOOL,
        description: "Emit the generated task spec for the dropped file",
        parameters: {
          type: "object",
          properties: {
            slug: {
              type: "string",
              description: "kebab-case filename stem for the task spec (no .md)",
            },
            title: { type: "string" },
            type: {
              type: "string",
              enum: ["ingest", "curate", "scan", "repair", "report", "ontology", "other"],
            },
            priority: { type: "integer", minimum: 1, maximum: 5 },
            description: { type: "string" },
            dependencies: { type: "array", items: { type: "string" } },
          },
          required: ["slug", "title", "type", "description"],
        },
      },
    ];

    try {
      const result = await this.plugin.llm.tool_use({
        model,
        messages: [{ role: "user", content: prompt }],
        maxTokens: 1024,
        tools,
        timeoutMs: 30_000,
        signal: this.plugin.lifetimeSignal,
      });
      return this.parseSpec(result.tool_calls, result.content);
    } catch (e) {
      if (e instanceof LLMProviderError && e.kind === "aborted") return null;
      throw e;
    }
  }

  private parseSpec(
    toolCalls: Array<{ name: string; arguments: string }> | undefined,
    content: string
  ): TaskSpec | null {
    // Preferred path: the model emitted the tool call.
    if (toolCalls) {
      const call = toolCalls.find((c) => c.name === TASK_EMIT_TOOL);
      if (call) {
        try {
          return validateSpec(JSON.parse(call.arguments) as Record<string, unknown>);
        } catch (e) {
          console.warn("NLR dispatcher: tool_call arguments not valid JSON", e);
        }
      }
    }
    // Fallback: try to find a ```json block in the content.
    const match = content.match(/```json\s*\n([\s\S]*?)\n```/);
    if (match) {
      try {
        return validateSpec(JSON.parse(match[1]) as Record<string, unknown>);
      } catch (e) {
        console.warn("NLR dispatcher: fenced JSON block not valid", e);
      }
    }
    console.warn("NLR dispatcher: no usable task spec in LLM output");
    return null;
  }

  private async writeTaskSpec(sourcePath: string, spec: TaskSpec): Promise<string | null> {
    // Slug-selection and vault.create are held under `writeChain` so two
    // concurrent dispatches that slugify the same way can't both resolve
    // the same free path — see runWithWriteLock above.
    return this.runWithWriteLock(async () => {
      const outDir = this.plugin.settings.dispatcher.taskOutputDir.replace(/\/$/, "");
      const baseSlug = sanitiseSlug(spec.slug);
      const body = renderTaskMarkdown(sourcePath, spec);
      const MAX_SUFFIX = 32;

      // Ensure output folder exists once before the loop.
      if (!this.plugin.app.vault.getAbstractFileByPath(outDir)) {
        try {
          await this.plugin.app.vault.createFolder(outDir);
        } catch {
          /* already exists */
        }
      }

      // Loop: pick a candidate slug, try to create. If the vault reports
      // the file already exists (a race we didn't expect — the lock
      // *should* prevent this, but stale Obsidian cache or filesystem
      // churn can still surprise us), bump the suffix and try again.
      // Once we hold the lock, only another process (not another
      // dispatch) can introduce this race, so at most a handful of
      // retries are ever needed.
      for (let attempt = 0; attempt <= MAX_SUFFIX; attempt++) {
        const targetPath =
          attempt === 0 ? `${outDir}/${baseSlug}.md` : `${outDir}/${baseSlug}-${attempt}.md`;
        if (this.plugin.app.vault.getAbstractFileByPath(targetPath)) continue;
        try {
          await this.plugin.app.vault.create(targetPath, body);
          return targetPath;
        } catch (e) {
          // Obsidian throws "File already exists" for the races we care
          // about. Other errors (ENOSPC, permission denied, etc.) should
          // surface to the caller — re-throw.
          const msg = (e as Error).message || "";
          if (!/already exists|exists/i.test(msg)) throw e;
          // else: fall through to the next suffix.
        }
      }
      console.warn("NLR dispatcher: exhausted slug suffixes, aborting");
      return null;
    });
  }
}

