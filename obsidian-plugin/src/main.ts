import {
  Plugin,
  Notice,
  WorkspaceLeaf,
  addIcon,
} from "obsidian";
import { NLRSettingTab, NLRSettings, DEFAULT_SETTINGS, migrateSettings } from "./settings";
import { registerCommands } from "./commands";
import { ChatbotView, VIEW_TYPE_CHATBOT } from "./chatbot";
import { StatsView, VIEW_TYPE_STATS } from "./stats";
import { NeuroChatView, VIEW_TYPE_NEURO_CHAT } from "./views/chat-view";
import { LLMManager } from "./providers";
import { VaultEventsClient } from "./mcp-vault-events";
import { NewSpecDispatcher } from "./dispatcher/new-spec";
import { execFile, ChildProcess, spawn } from "child_process";
import { promisify } from "util";
import * as path from "path";
import * as fs from "fs";

const execFileAsync = promisify(execFile);

const BRAIN_ICON = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 2a7 7 0 0 0-7 7c0 2.38 1.19 4.47 3 5.74V17a2 2 0 0 0 2 2h4a2 2 0 0 0 2-2v-2.26c1.81-1.27 3-3.36 3-5.74a7 7 0 0 0-7-7z"/><path d="M9 21h6"/><path d="M10 17v4"/><path d="M14 17v4"/><path d="M8 14c-1.5-1-2.5-2.7-2.5-5"/><path d="M16 14c1.5-1 2.5-2.7 2.5-5"/></svg>`;

const CHART_ICON = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="12" width="4" height="9"/><rect x="10" y="7" width="4" height="14"/><rect x="17" y="3" width="4" height="18"/><line x1="3" y1="21" x2="21" y2="21"/></svg>`;

export default class NLRPlugin extends Plugin {
  settings: NLRSettings = DEFAULT_SETTINGS;
  /** Single LLM manager the whole plugin shares. */
  llm!: LLMManager;
  /** Global AbortController — aborted on unload to terminate in-flight
   *  fetches, long-poll loops, and debounced file handlers. */
  private lifetime = new AbortController();
  private serverProcess: ChildProcess | null = null;
  private subscription: VaultEventsClient | null = null;
  private dispatcher: NewSpecDispatcher | null = null;

  async onload(): Promise<void> {
    await this.loadSettings();
    this.llm = new LLMManager(this.settings.llm);

    addIcon("nlr-brain", BRAIN_ICON);
    addIcon("nlr-chart", CHART_ICON);

    this.registerView(VIEW_TYPE_CHATBOT, (leaf) => new ChatbotView(leaf, this));
    this.registerView(VIEW_TYPE_STATS, (leaf) => new StatsView(leaf, this));
    this.registerView(VIEW_TYPE_NEURO_CHAT, (leaf) => new NeuroChatView(leaf, this));

    this.addSettingTab(new NLRSettingTab(this.app, this));

    this.addRibbonIcon("nlr-brain", "Neuro-Link Chatbot", () => {
      this.activateView(VIEW_TYPE_CHATBOT);
    });

    this.addRibbonIcon("nlr-chart", "Neuro-Link Stats", () => {
      this.activateView(VIEW_TYPE_STATS);
    });

    this.addRibbonIcon("nlr-brain", "Neuro Chat (@neuro)", () => {
      this.activateView(VIEW_TYPE_NEURO_CHAT);
    });

    // Chat panel toggle — Cmd/Ctrl+Shift+K.
    this.addCommand({
      id: "nlr-toggle-neuro-chat",
      name: "Toggle Neuro Chat panel",
      hotkeys: [{ modifiers: ["Mod", "Shift"], key: "k" }],
      callback: () => {
        void this.toggleNeuroChat();
      },
    });

    registerCommands(this);

    await this.scaffoldVaultStructure();
    await this.checkNlrBinary();
    await this.startServer();
    this.startVaultSubscription();
  }

  onunload(): void {
    // Aborting first lets any pending fetch / debounce timer short-circuit
    // before we tear down their owners.
    this.lifetime.abort();
    if (this.subscription) {
      // Fire-and-forget: disconnect awaits the in-flight long-poll and
      // unsubscribe. We don't block unload on it — the lifetime abort
      // already tripped, which cancels any pending fetch within the client.
      void this.subscription.disconnect();
      this.subscription = null;
    }
    this.dispatcher = null;
    this.stopServer();
    this.app.workspace.detachLeavesOfType(VIEW_TYPE_CHATBOT);
    this.app.workspace.detachLeavesOfType(VIEW_TYPE_STATS);
    this.app.workspace.detachLeavesOfType(VIEW_TYPE_NEURO_CHAT);
  }

  /** Called from settings UI when LLM config changes to rebind the manager. */
  refreshLLM(): void {
    this.llm.updateSettings(this.settings.llm);
  }

  /** AbortSignal that fires at plugin unload — wire in long-running work. */
  get lifetimeSignal(): AbortSignal {
    return this.lifetime.signal;
  }

  private startVaultSubscription(): void {
    if (!this.settings.subscription.enabled) return;

    // Construct dispatcher first; the subscription forwards events to it.
    this.dispatcher = new NewSpecDispatcher(this);

    this.subscription = new VaultEventsClient(this, async (event) => {
      if (!this.dispatcher) return;
      // Drop the synthetic Overflow kind the HTTP client uses for
      // bookkeeping — the dispatcher only knows the four real vault
      // event kinds. Log the overflow so it's visible in the devtools
      // without pulling in a new UI path.
      if (event.kind === "Overflow") {
        console.warn(
          `NLR vault-events: ${event.path} (dropped=${event.droppedCount ?? 0})`
        );
        return;
      }
      this.dispatcher.handle(event);
    });
    this.subscription.connect().catch((e: unknown) => {
      const err = e as Error;
      console.warn("NLR vault-events failed to connect:", err.message);
    });

    // Cold-start catch-up: dispatch any recent files that landed in the
    // window between `onload` starting and the subscription connecting.
    // Transport-agnostic (reads the vault directly), so it still matters
    // under the HTTP long-poll transport — arguably more so, since the
    // first `subscribe_vault_events` call is a full HTTP round-trip. See
    // PR #26 review, should-fix #9.
    this.dispatcher.scanCatchUp().catch((e: unknown) => {
      const err = e as Error;
      console.warn("NLR dispatcher: cold-start scan failed:", err.message);
    });
  }

  private async scaffoldVaultStructure(): Promise<void> {
    const vault = this.app.vault;
    const DIRS = [
      "00-raw",
      "01-sorted", "01-sorted/books", "01-sorted/medium", "01-sorted/arxiv",
      "01-sorted/huggingface", "01-sorted/github", "01-sorted/docs",
      "02-KB-main",
      "03-ontology-main", "03-ontology-main/workflow", "03-ontology-main/agents",
      "03-ontology-main/agents/by-agent", "03-ontology-main/agents/by-workflow-state",
      "03-ontology-main/agents/by-auto-HITL",
      "04-KB-agents-workflows",
      "05-insights-gaps", "05-insights-gaps/knowledge", "05-insights-gaps/ontology", "05-insights-gaps/goals",
      "05-self-improvement-HITL", "05-self-improvement-HITL/models",
      "05-self-improvement-HITL/hyperparameters", "05-self-improvement-HITL/prompts",
      "05-self-improvement-HITL/features", "05-self-improvement-HITL/code-changes",
      "05-self-improvement-HITL/services-integrations",
      "06-self-improvement-recursive", "06-self-improvement-recursive/harness-to-harness-comms",
      "06-self-improvement-recursive/harness-cli", "06-self-improvement-recursive/brain",
      "06-progress-reports",
      "07-neuro-link-task",
      "08-code-docs", "08-code-docs/my-repos", "08-code-docs/common-tools", "08-code-docs/my-forks",
      "09-business-docs",
      "config", "state",
    ];

    // Check if already scaffolded (02-KB-main exists = not first run)
    const marker = vault.getAbstractFileByPath("02-KB-main");
    if (marker) return; // already scaffolded

    for (const dir of DIRS) {
      try {
        await vault.createFolder(dir);
      } catch {
        // folder already exists
      }
    }

    // Create seed files
    const seeds: Array<{ path: string; content: string }> = [
      {
        path: "02-KB-main/schema.md",
        content: "---\ntitle: Wiki Schema\n---\n# Wiki Page Conventions\n\nEvery page has YAML frontmatter: `title`, `domain`, `sources[]`, `confidence`, `last_updated`, `open_questions[]`\n\nSections: Overview > Conceptual Model > Details > Contradictions > Open Questions > Sources\n",
      },
      {
        path: "02-KB-main/index.md",
        content: "# Wiki Index\n\n*Auto-generated. Do not edit manually.*\n",
      },
      {
        path: "02-KB-main/log.md",
        content: "# Mutation Log\n\n*Append-only record of wiki changes.*\n",
      },
      {
        path: "03-ontology-main/workflow/state-definitions.md",
        content: "---\ntitle: Workflow State Definitions\n---\n# States\n\nsignal → impression → insight → framework → lens → synthesis → index\n",
      },
      {
        path: "03-ontology-main/workflow/phase-gating.md",
        content: "---\ntitle: Phase Gating\n---\n# Phase Gate Requirements\n\nDefine what must be true before transitioning between states.\n",
      },
      {
        path: "03-ontology-main/workflow/goal-hierarchical.md",
        content: "---\ntitle: Goal Hierarchy\n---\n# Goals\n\nDefine your domain goals from broad to specific.\n",
      },
      {
        path: "06-progress-reports/daily.md",
        content: "# Daily Report\n\n*Auto-generated by progress-report skill.*\n",
      },
      {
        path: "config/neuro-link.md",
        content: "---\nversion: 1\nauto_rag: true\nauto_curate: true\ndefault_llm: claude-sonnet-4-6\nwiki_llm: claude-sonnet-4-6\nontology_llm: claude-opus-4-6\nembedding_model: Octen/Octen-Embedding-8B\nembedding_dims: 4096\nvector_db: qdrant\nallowed_paths: all\n---\n# Neuro-Link Master Config\n\nEdit the YAML frontmatter above to configure the system.\n",
      },
      {
        path: "state/heartbeat.json",
        content: '{"status":"initialized","last_check":"' + new Date().toISOString() + '","errors":[]}',
      },
    ];

    for (const seed of seeds) {
      const exists = vault.getAbstractFileByPath(seed.path);
      if (!exists) {
        try {
          await vault.create(seed.path, seed.content);
        } catch {
          // file exists
        }
      }
    }

    new Notice("Neuro-Link: vault structure created with default folders and config");
  }

  private async startServer(): Promise<void> {
    const binPath = this.resolveBinaryPath();
    const port = this.settings.apiRouterPort || 8080;

    const env: Record<string, string> = { ...process.env as Record<string, string> };
    if (this.settings.nlrRoot) {
      env["NLR_ROOT"] = this.settings.nlrRoot;
    }

    // Load token from secrets/.env if available
    const secretsPath = this.settings.nlrRoot
      ? path.join(this.settings.nlrRoot, "secrets", ".env")
      : "";
    let token = "";
    if (secretsPath && fs.existsSync(secretsPath)) {
      const content = fs.readFileSync(secretsPath, "utf-8");
      const match = content.match(/NLR_API_TOKEN=(.+)/);
      if (match) token = match[1].trim();
    }

    const args = ["serve", "--port", String(port)];
    if (token) {
      args.push("--token", token);
    } else {
      args.push("--token", "auto");
    }

    try {
      this.serverProcess = spawn(binPath, args, {
        env,
        cwd: this.settings.nlrRoot || undefined,
        stdio: ["ignore", "pipe", "pipe"],
      });

      this.serverProcess.on("error", () => {
        // Binary not found or spawn failed — silently ignore
        this.serverProcess = null;
      });

      this.serverProcess.on("exit", () => {
        this.serverProcess = null;
      });

      // Wait briefly then health check
      await new Promise((r) => setTimeout(r, 1500));
      if (this.serverProcess) {
        try {
          const resp = await fetch(`http://localhost:${port}/health`);
          if (resp.ok) {
            new Notice(`Neuro-Link server running on port ${port}`);
          }
        } catch {
          // Server may still be starting
        }
      }
    } catch {
      // spawn failed
    }
  }

  private stopServer(): void {
    if (this.serverProcess) {
      this.serverProcess.kill();
      this.serverProcess = null;
    }
  }

  async loadSettings(): Promise<void> {
    const data = (await this.loadData()) as Partial<NLRSettings> | null;
    this.settings = migrateSettings(data || {});
    if (!this.settings.nlrRoot) {
      this.settings.nlrRoot = this.detectNlrRoot();
    }
    if (!this.settings.vaultPath) {
      this.settings.vaultPath = this.detectVaultPath();
    }
  }

  async saveSettings(): Promise<void> {
    await this.saveData(this.settings);
  }

  detectNlrRoot(): string {
    const vaultPath = this.detectVaultPath();
    if (vaultPath) {
      const candidate = path.resolve(vaultPath, "..");
      if (fs.existsSync(path.join(candidate, "config", "neuro-link.md"))) {
        return candidate;
      }
      if (fs.existsSync(path.join(vaultPath, "config", "neuro-link.md"))) {
        return vaultPath;
      }
    }
    const home = process.env.HOME || process.env.USERPROFILE || "";
    const defaultPath = path.join(home, "Desktop", "HyperFrequency", "neuro-link-recursive");
    if (fs.existsSync(defaultPath)) {
      return defaultPath;
    }
    return "";
  }

  detectVaultPath(): string {
    const adapter = this.app.vault.adapter;
    if ("getBasePath" in adapter && typeof adapter.getBasePath === "function") {
      return (adapter as { getBasePath(): string }).getBasePath();
    }
    return "";
  }

  resolveBinaryPath(): string {
    // User-configured path takes priority
    const configured = this.settings.nlrBinaryPath;
    if (configured && configured !== "neuro-link" && fs.existsSync(configured)) {
      return configured;
    }
    // Check common locations (Electron doesn't inherit shell PATH)
    const candidates = [
      "/usr/local/bin/neuro-link",
      path.join(process.env.HOME || "", ".cargo/bin/neuro-link"),
      this.settings.nlrRoot ? path.join(this.settings.nlrRoot, "server/target/release/neuro-link") : "",
      "/opt/homebrew/bin/neuro-link",
    ].filter(Boolean);
    for (const c of candidates) {
      if (fs.existsSync(c)) return c;
    }
    return configured || "neuro-link";
  }

  async checkNlrBinary(): Promise<void> {
    const binPath = this.resolveBinaryPath();
    try {
      await execFileAsync(binPath, ["--version"]);
    } catch {
      new Notice(
        `neuro-link binary not found at ${binPath}. Set the full path in Settings > Neuro-Link Recursive > NLR Binary Path.`,
        10000
      );
    }
  }

  async runNlrCommand(args: string[]): Promise<string> {
    const binPath = this.resolveBinaryPath();
    const env: Record<string, string> = { ...process.env as Record<string, string> };
    if (this.settings.nlrRoot) {
      env["NLR_ROOT"] = this.settings.nlrRoot;
    }
    try {
      const { stdout, stderr } = await execFileAsync(binPath, args, {
        cwd: this.settings.nlrRoot || undefined,
        env,
        timeout: 30000,
      });
      if (stderr && !stdout) return stderr;
      return stdout;
    } catch (e: unknown) {
      const err = e as { stderr?: string; message?: string };
      throw new Error(err.stderr || err.message || "Unknown error");
    }
  }

  /**
   * Toggle the Neuro Chat panel. If already visible as the active leaf,
   * detach it; otherwise reveal (creating the leaf if needed). Matches
   * Obsidian's convention for side-panel toggle commands.
   */
  async toggleNeuroChat(): Promise<void> {
    const leaves = this.app.workspace.getLeavesOfType(VIEW_TYPE_NEURO_CHAT);
    if (leaves.length > 0) {
      const activeLeaf = this.app.workspace.activeLeaf;
      // Detach only if the active leaf IS our chat view; otherwise focus it.
      if (activeLeaf && leaves.includes(activeLeaf)) {
        this.app.workspace.detachLeavesOfType(VIEW_TYPE_NEURO_CHAT);
        return;
      }
      this.app.workspace.revealLeaf(leaves[0]);
      return;
    }
    await this.activateView(VIEW_TYPE_NEURO_CHAT);
  }

  async activateView(viewType: string): Promise<void> {
    const existing = this.app.workspace.getLeavesOfType(viewType);
    if (existing.length > 0) {
      this.app.workspace.revealLeaf(existing[0]);
      return;
    }
    const leaf: WorkspaceLeaf = this.app.workspace.getRightLeaf(false)!;
    await leaf.setViewState({ type: viewType, active: true });
    this.app.workspace.revealLeaf(leaf);
  }
}
