// SPDX-License-Identifier: MIT
//
// Clean-room. Not adapted from obsidian-copilot.
//
// Right-side chat panel. Hosts a Composer, a MessageList, and a header with
// mode toggle + model-info + refresh-tools button. Two modes:
//
//   - chat: plain streaming via plugin.llm.chatStream(). No tools loaded,
//     no agent loop. Cheap and fast.
//   - agent: @neuro-prefixed messages route through NeuroAgent. Tool
//     manifest loads lazily on first agent turn; subsequent turns reuse
//     the cached manifest.
//
// The view owns the transcript. Transcript turnover is bounded by
// settings.chatPanel.maxTranscriptTurns so long sessions don't OOM.

import { ItemView, Notice, WorkspaceLeaf } from "obsidian";
import type NLRPlugin from "../main";
import { Composer, type ComposerSuggestion } from "./composer";
import { MessageList, type ChatMessage } from "./message-list";
import { StreamingIndicator } from "./streaming-indicator";
import {
  NeuroAgent,
  detectNeuroMode,
  DEFAULT_MAX_TURNS,
  DEFAULT_TOKEN_BUDGET,
  type AgentLLM,
} from "../agent/neuro-agent";
import { ToolManifestLoader, skillToolName } from "../agent/tool-manifest";
import { SystemPromptLoader } from "../agent/system-prompt";
import { VaultTraceLogger } from "../agent/trace-logger";
import type { LLMToolCall, LLMMessage } from "../providers/base";
import * as path from "path";

export const VIEW_TYPE_NEURO_CHAT = "nlr-neuro-chat-view";

export class NeuroChatView extends ItemView {
  private plugin: NLRPlugin;
  private composer!: Composer;
  private messages!: MessageList;
  private indicator!: StreamingIndicator;
  private streaming = false;
  private abortController: AbortController | null = null;
  private transcript: ChatMessage[] = [];
  private manifestLoader!: ToolManifestLoader;
  private promptLoader!: SystemPromptLoader;
  private trace!: VaultTraceLogger;
  private headerSubtitleEl: HTMLElement | null = null;

  constructor(leaf: WorkspaceLeaf, plugin: NLRPlugin) {
    super(leaf);
    this.plugin = plugin;
  }

  getViewType(): string {
    return VIEW_TYPE_NEURO_CHAT;
  }

  getDisplayText(): string {
    return "Neuro Chat";
  }

  getIcon(): string {
    return "nlr-brain";
  }

  async onOpen(): Promise<void> {
    const root = this.containerEl.children[1] as HTMLElement;
    root.empty();
    root.addClass("nlr-chat-view");

    this.buildHeader(root);
    this.messages = new MessageList(root, {
      app: this.app,
      parent: this,
      autoScroll: this.plugin.settings.chatPanel.autoScroll,
    });
    // Indicator is the last child so it appears just above the composer.
    const indicatorHost = root.createDiv({ cls: "nlr-chat-indicator-host" });
    this.indicator = new StreamingIndicator(indicatorHost);
    this.composer = new Composer(root, {
      app: this.app,
      skills: () => this.skillSuggestions(),
      agents: () => [
        {
          kind: "agent",
          value: "neuro",
          label: "@neuro",
          description: "Orchestrator agent with vault tools",
        },
      ],
      onSubmit: (content) => {
        void this.handleSubmit(content);
      },
      onStop: () => this.abortInFlight(),
    });

    this.manifestLoader = new ToolManifestLoader({
      skills: {
        skillsDir: this.resolveSkillsDir(),
      },
    });

    this.promptLoader = new SystemPromptLoader({
      vaultPath: this.plugin.settings.vaultPath,
      nlrRoot: this.plugin.settings.nlrRoot,
    });

    this.trace = new VaultTraceLogger(this.app);
  }

  async onClose(): Promise<void> {
    this.abortInFlight();
    this.indicator?.destroy();
    this.composer?.destroy();
  }

  // ── header ────────────────────────────────────────────────────────────

  private buildHeader(root: HTMLElement): void {
    const header = root.createDiv({ cls: "nlr-chat-header" });
    const titleWrap = header.createDiv({ cls: "nlr-chat-header-title" });
    titleWrap.createEl("h4", { text: "Neuro Chat" });
    this.headerSubtitleEl = titleWrap.createSpan({
      cls: "nlr-chat-header-subtitle",
    });
    this.updateSubtitle();

    const actions = header.createDiv({ cls: "nlr-chat-header-actions" });

    const clearBtn = actions.createEl("button", {
      text: "Clear",
      cls: "nlr-chat-action-btn",
    });
    clearBtn.addEventListener("click", () => {
      this.transcript = [];
      this.messages.clear();
      this.updateSubtitle();
    });

    const refreshBtn = actions.createEl("button", {
      text: "Refresh tools",
      cls: "nlr-chat-action-btn",
    });
    refreshBtn.addEventListener("click", async () => {
      try {
        await this.manifestLoader.refresh();
        const counts = this.manifestLoader.lastCounts();
        new Notice(
          `Neuro tools refreshed: ${counts.mcp} MCP + ${counts.skills} skills`
        );
      } catch (e) {
        new Notice(`Refresh failed: ${(e as Error).message}`);
      }
    });
  }

  private updateSubtitle(): void {
    if (!this.headerSubtitleEl) return;
    const model = this.plugin.settings.chatPanel.defaultModel
      || this.plugin.llm.defaultModel()
      || "(no model)";
    this.headerSubtitleEl.setText(
      `${model} · ${this.transcript.length} turns`
    );
  }

  // ── submit / dispatch ─────────────────────────────────────────────────

  private async handleSubmit(content: string): Promise<void> {
    if (this.streaming) return;

    const isAgentMode = detectNeuroMode(content);
    const userMsg = this.appendUser(content, isAgentMode);

    this.streaming = true;
    this.composer.setStreaming(true);
    this.indicator.show();
    this.abortController = new AbortController();

    try {
      if (isAgentMode) {
        await this.runAgent(content);
      } else {
        await this.runChat(content);
      }
    } catch (e) {
      this.appendAssistant(`Error: ${(e as Error).message}`);
    } finally {
      this.streaming = false;
      this.composer.setStreaming(false);
      this.indicator.hide();
      this.abortController = null;
      this.trimTranscript();
      this.updateSubtitle();
      void userMsg; // silence unused-local warning
    }
  }

  private async runChat(content: string): Promise<void> {
    const model =
      this.plugin.settings.chatPanel.defaultModel ||
      this.plugin.llm.defaultModel();
    if (!model) {
      this.appendAssistant(
        "No LLM model configured. Set one under Settings → Neuro-Link Recursive → LLM Providers."
      );
      return;
    }
    const history = this.toLLMHistory();
    const placeholder = this.appendAssistant("", false);
    let accum = "";
    try {
      for await (const chunk of this.plugin.llm.chatStream({
        model,
        messages: [
          ...history,
          { role: "user", content },
        ],
        signal: this.abortController?.signal,
      })) {
        if (chunk.contentDelta) {
          accum += chunk.contentDelta;
          this.messages.update(placeholder.id, accum);
        }
        if (chunk.done) break;
      }
    } catch (e) {
      if ((e as Error).name === "AbortError") {
        accum += "\n\n*[stopped]*";
      } else {
        accum += `\n\nError: ${(e as Error).message}`;
      }
    }
    placeholder.content = accum || "(no response)";
    this.messages.update(placeholder.id, placeholder.content);
  }

  private async runAgent(content: string): Promise<void> {
    const tools = await this.manifestLoader.get();
    const systemPrompt = this.promptLoader.load();
    const llm: AgentLLM = {
      tool_use: (opts) => this.plugin.llm.tool_use(opts),
      defaultModel: () =>
        this.plugin.settings.chatPanel.defaultModel ||
        this.plugin.llm.defaultModel(),
    };

    const assistantMsg = this.appendAssistant("", true);
    const toolCalls: Array<{
      id: string;
      name: string;
      arguments: string;
      outcome?: "ok" | "refused" | "error";
      result?: string;
    }> = [];

    const agent = new NeuroAgent(
      {
        llm,
        tools,
        systemPrompt,
        trace: this.trace,
        executor: (call) => this.executeToolCall(call),
        signal: this.abortController?.signal,
      },
      {
        maxTurns: DEFAULT_MAX_TURNS,
        tokenBudget: DEFAULT_TOKEN_BUDGET,
        conversationId: assistantMsg.id,
      }
    );

    let result;
    try {
      result = await agent.run(content, this.toLLMHistory());
    } catch (e) {
      assistantMsg.content = `Agent error: ${(e as Error).message}`;
      this.messages.update(assistantMsg.id, assistantMsg.content);
      return;
    }

    for (const tc of result.toolCalls) {
      toolCalls.push({
        id: tc.call.id,
        name: tc.call.name,
        arguments: tc.call.arguments,
        outcome: tc.outcome,
        result: tc.result,
      });
    }

    const content_out =
      result.finalContent ||
      (result.stopReason === "max_turns"
        ? "_Agent stopped after reaching max turns. Try narrowing the task._"
        : result.stopReason === "token_budget"
        ? "_Agent stopped: token budget exceeded._"
        : result.stopReason === "aborted"
        ? "_Agent stopped by user._"
        : "_Agent stopped without producing a response._");

    assistantMsg.content = content_out;
    assistantMsg.toolCalls = toolCalls;
    this.messages.update(assistantMsg.id, assistantMsg.content, toolCalls);
  }

  /**
   * Execute a single tool call. Routes:
   *   - `run_skill_<skill>` → plugin.runNlrCommand(["skill", "run", ...])
   *   - anything else → throw (the caller wraps into an "error" tool-result)
   *
   * Real MCP tool execution is intentionally *not* wired here yet — the
   * MCP subscription client only provides subscribe/unsubscribe. Adding a
   * generic tools/call over the same WebSocket is a follow-up; for now,
   * tv_* calls surface as errors the agent can react to.
   */
  private async executeToolCall(call: LLMToolCall): Promise<unknown> {
    if (call.name.startsWith("run_skill_")) {
      const skillName = call.name.slice("run_skill_".length).replace(/_/g, "-");
      let args: { args?: string } = {};
      try {
        args = JSON.parse(call.arguments || "{}") as { args?: string };
      } catch {
        /* ignore */
      }
      const argStr = args.args ?? "";
      const cliArgs = ["skill", "run", skillName];
      if (argStr.trim()) cliArgs.push("--args", argStr);
      return await this.plugin.runNlrCommand(cliArgs);
    }
    throw new Error(
      `Tool '${call.name}' has no local executor. Connect the MCP transport or extend executeToolCall().`
    );
  }

  // ── helpers ───────────────────────────────────────────────────────────

  private abortInFlight(): void {
    if (this.abortController) {
      this.abortController.abort();
    }
  }

  private appendUser(content: string, isAgent: boolean): ChatMessage {
    const msg: ChatMessage = {
      id: makeMessageId(),
      role: "user",
      content,
      timestamp: Date.now(),
      modeBadge: isAgent ? "agent" : undefined,
    };
    this.transcript.push(msg);
    this.messages.append(msg);
    return msg;
  }

  private appendAssistant(content: string, isAgent = false): ChatMessage {
    const msg: ChatMessage = {
      id: makeMessageId(),
      role: "assistant",
      content,
      timestamp: Date.now(),
      modeBadge: isAgent ? "agent" : undefined,
    };
    this.transcript.push(msg);
    this.messages.append(msg);
    return msg;
  }

  /** Convert the running transcript to the shape llm.* expects. */
  private toLLMHistory(): LLMMessage[] {
    // Exclude the most recent user turn (handleSubmit adds it *after*
    // calling runChat/runAgent for rendering, so `toLLMHistory` is called
    // after the user msg is in the transcript — we need to drop it).
    const history: LLMMessage[] = [];
    for (let i = 0; i < this.transcript.length - 1; i++) {
      const m = this.transcript[i];
      if (m.role === "user" || m.role === "assistant") {
        history.push({ role: m.role, content: m.content });
      }
    }
    return history;
  }

  private trimTranscript(): void {
    const cap = this.plugin.settings.chatPanel.maxTranscriptTurns;
    if (!cap || this.transcript.length <= cap) return;
    const drop = this.transcript.length - cap;
    for (let i = 0; i < drop; i++) {
      this.messages.remove(this.transcript[i].id);
    }
    this.transcript = this.transcript.slice(drop);
  }

  private skillSuggestions(): ComposerSuggestion[] {
    // Cheap pass — read the skill names out of the manifest loader's last
    // cache. If the cache is empty, return an empty array; user can hit
    // "Refresh tools" to populate.
    const counts = this.manifestLoader?.lastCounts();
    if (!counts || counts.skills === 0) return [];
    // We don't expose the full manifest off the loader; synthesize a
    // minimal list from the skills directory directly. Pure-JS: read the
    // dir, no fs watch needed.
    try {
      const fs = require("fs") as typeof import("fs");
      const dir = this.resolveSkillsDir();
      if (!dir || !fs.existsSync(dir)) return [];
      const entries = fs.readdirSync(dir);
      return entries
        .filter((e: string) => fs.existsSync(`${dir}/${e}/SKILL.md`))
        .map<ComposerSuggestion>((e: string) => ({
          kind: "skill",
          value: skillToolName(e),
          label: e,
          description: `Skill ${e}`,
        }));
    } catch {
      return [];
    }
  }

  private resolveSkillsDir(): string {
    const nlrRoot = this.plugin.settings.nlrRoot;
    if (!nlrRoot) return "";
    return path.join(nlrRoot, ".claude", "skills");
  }
}

function makeMessageId(): string {
  // Lightweight unique id — good enough for Map keys inside a single view.
  return `m_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 8)}`;
}
