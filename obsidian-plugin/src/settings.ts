import {
  App,
  PluginSettingTab,
  Setting,
  Notice,
  TextComponent,
  ButtonComponent,
} from "obsidian";
import type NLRPlugin from "./main";
import * as fs from "fs";
import * as path from "path";
import { execFile } from "child_process";
import { promisify } from "util";
import {
  DEFAULT_LLM_SETTINGS,
  LLMManagerSettings,
  LLMProviderSettings,
} from "./providers";
import { ProviderId } from "./providers/base";

const execFileAsync = promisify(execFile);

export const SETTINGS_SCHEMA_VERSION = 3;

export interface DispatcherSettings {
  /** Master switch for the file-drop → task-spec pipeline. */
  enabled: boolean;
  /** Watch glob passed to tv_subscribe_vault_events. */
  watchGlob: string;
  /** Directory (relative to vault root) where generated task specs land. */
  taskOutputDir: string;
  /** Debounce before reading a newly-created file to avoid partial writes. */
  debounceMs: number;
  /** Model override (defaults to the primary provider's default model). */
  model: string;
}

export const DEFAULT_DISPATCHER_SETTINGS: DispatcherSettings = {
  enabled: true,
  watchGlob: "00-neuro-link/*.md",
  taskOutputDir: "00-neuro-link/tasks",
  debounceMs: 500,
  model: "",
};

export interface SubscriptionSettings {
  /** Master switch for the vault-events subscription client. */
  enabled: boolean;
  /**
   * HTTP base URL of the MCP endpoint the vault-events client talks to.
   * Empty = auto-derive from `apiRouterPort` (→ `http://localhost:<port>/mcp`).
   *
   * Legacy field: prior schemas called this `wsUrl` and stored
   * `ws://.../mcp/ws`. `migrateSettings` rewrites `ws(s)://` to `http(s)://`
   * and drops the `/ws` suffix one-shot when bumping schema v2 → v3.
   */
  endpointUrl: string;
}

export const DEFAULT_SUBSCRIPTION_SETTINGS: SubscriptionSettings = {
  enabled: true,
  endpointUrl: "",
};

/** Settings for the right-side chat panel + @neuro agent (Phase 6/7). */
export interface ChatPanelSettings {
  /** Model id used for chat + agent turns. Blank = primary provider default. */
  defaultModel: string;
  /**
   * Cap on in-memory transcript turns. Older turns get detached from the
   * view (and the outgoing LLM context) once this is exceeded. Prevents
   * long sessions from drifting into high-token prompts.
   */
  maxTranscriptTurns: number;
  /** When true, auto-scroll the message list to bottom on new content. */
  autoScroll: boolean;
}

export const DEFAULT_CHAT_PANEL_SETTINGS: ChatPanelSettings = {
  defaultModel: "",
  maxTranscriptTurns: 50,
  autoScroll: true,
};

export interface HarnessConfig {
  name: string;
  type: string;
  status: string;
  url: string;
  apiKeyEnv: string;
  role: string;
  capabilities: string[];
}

export interface NLRSettings {
  schemaVersion: number;
  nlrRoot: string;
  nlrBinaryPath: string;
  vaultPath: string;
  apiKeys: Record<string, string>;
  harnesses: HarnessConfig[];
  mcpServerMode: string;
  mcp2cliProfilePath: string;
  apiRouterPort: number;
  ngrokDomain: string;
  sessionLogging: boolean;
  scoreHistory: boolean;
  autoGrade: boolean;
  chatbotModel: string;
  chatbotSystemPrompt: string;
  apiRoutes: Array<{ keyName: string; provider: string; endpoint: string }>;
  llm: LLMManagerSettings;
  dispatcher: DispatcherSettings;
  subscription: SubscriptionSettings;
  chatPanel: ChatPanelSettings;
}

const API_KEY_DEFS: Array<{ key: string; label: string; desc: string; defaultVal?: string; test: string }> = [
  // LLM Providers (for your agents to call through neuro-link's /llm/v1 proxy)
  { key: "OPENROUTER_API_KEY", label: "OpenRouter API Key", desc: "LLM routing for chatbot and LLM passthrough proxy", test: "openrouter" },
  { key: "ANTHROPIC_API_KEY", label: "Anthropic API Key", desc: "Direct Anthropic access for /llm/v1/messages passthrough (optional if using OpenRouter)", test: "key-saved" },
  // Knowledge & Research
  { key: "PARALLEL_API_KEY", label: "Parallel Web API Key", desc: "Web scraping, search, and deep research for crawl-ingest pipeline", test: "key-saved" },
  { key: "INFRANODUS_API_KEY", label: "InfraNodus API Key", desc: "Knowledge graphs, gap analysis, ontology queries (MCP via mcporter)", test: "key-saved" },
  // Local Infrastructure
  { key: "EMBEDDING_API_URL", label: "Embedding Server URL", desc: "Octen-Embedding-8B — start with: ./scripts/embedding-server.sh", defaultVal: "http://localhost:8400/v1/embeddings", test: "local-url" },
  { key: "QDRANT_URL", label: "Qdrant URL", desc: "Vector database for semantic search", defaultVal: "http://localhost:6333", test: "local-url" },
  { key: "NEO4J_URI", label: "Neo4j Bolt URI", desc: "Graph database for temporal knowledge (Graphiti)", defaultVal: "bolt://localhost:7687", test: "format:bolt://" },
  { key: "NEO4J_HTTP_URL", label: "Neo4j HTTP URL", desc: "Neo4j HTTP API for Cypher queries", defaultVal: "http://localhost:7474", test: "local-url" },
  { key: "NEO4J_PASSWORD", label: "Neo4j Password", desc: "Neo4j auth password (user: neo4j, min 8 chars)", defaultVal: "neurolink1234", test: "key-saved" },
  // Tunneling
  { key: "NGROK_AUTH_TOKEN", label: "Ngrok Auth Token", desc: "Tunnel for remote MCP/API access (get from ngrok.com/dashboard)", test: "ngrok" },
];

export const DEFAULT_SETTINGS: NLRSettings = {
  schemaVersion: SETTINGS_SCHEMA_VERSION,
  nlrRoot: "",
  nlrBinaryPath: "neuro-link",
  vaultPath: "",
  apiKeys: {},
  harnesses: [],
  mcpServerMode: "stdio",
  mcp2cliProfilePath: "",
  apiRouterPort: 8080,
  ngrokDomain: "",
  sessionLogging: true,
  scoreHistory: true,
  autoGrade: false,
  chatbotModel: "anthropic/claude-sonnet-4-20250514",
  chatbotSystemPrompt: "You are an assistant with access to the neuro-link-recursive knowledge base. Use the provided wiki context to answer questions accurately.",
  apiRoutes: [],
  llm: DEFAULT_LLM_SETTINGS,
  dispatcher: DEFAULT_DISPATCHER_SETTINGS,
  subscription: DEFAULT_SUBSCRIPTION_SETTINGS,
  chatPanel: DEFAULT_CHAT_PANEL_SETTINGS,
};

/**
 * Canonical mapping from legacy env-style key names (what lives in
 * `settings.apiKeys` and `secrets/.env`) to the provider id in the new
 * `llm.providers.*` schema. Used by both `mergeLLMSettings` (migration
 * path) and `syncLegacyApiKeys` (runtime propagation after loadSecretsEnv).
 */
const LEGACY_KEY_TO_PROVIDER: Array<{ env: string; provider: ProviderId }> = [
  { env: "OPENROUTER_API_KEY", provider: "openrouter" },
  { env: "ANTHROPIC_API_KEY", provider: "anthropic" },
  { env: "OPENAI_API_KEY", provider: "openai" },
];

/**
 * Migrates settings loaded from disk to the current schema. Preserves old
 * top-level keys (OPENROUTER_API_KEY, ANTHROPIC_API_KEY) as fallbacks for
 * the new `llm.providers` config when a user upgrades without reconfiguring.
 *
 * Called from main.ts::loadSettings after Object.assign merges defaults.
 */
export function migrateSettings(raw: Partial<NLRSettings>): NLRSettings {
  // Recover from a corrupted `llm` field (non-object) — we'd otherwise
  // crash on the destructuring in mergeLLMSettings. Falling back to
  // undefined lets the defaults take over.
  const safeLlm = isPlainObject(raw.llm) ? (raw.llm as Partial<LLMManagerSettings>) : undefined;
  const merged: NLRSettings = {
    ...DEFAULT_SETTINGS,
    ...raw,
    apiKeys: { ...(raw.apiKeys || {}) },
    llm: mergeLLMSettings(safeLlm, raw.apiKeys || {}),
    dispatcher: { ...DEFAULT_DISPATCHER_SETTINGS, ...(raw.dispatcher || {}) },
    subscription: migrateSubscriptionSettings(raw.subscription),
    chatPanel: { ...DEFAULT_CHAT_PANEL_SETTINGS, ...(raw.chatPanel || {}) },
    schemaVersion: SETTINGS_SCHEMA_VERSION,
  };
  return merged;
}

/**
 * Handle the v2 → v3 subscription rename. Old installs stored
 * `subscription.wsUrl: "ws://localhost:8080/mcp/ws"`; the new transport
 * reads `subscription.endpointUrl: "http://localhost:8080/mcp"`. Preserve
 * user intent by rewriting the scheme and trimming the `/ws` suffix so
 * the upgrade is transparent.
 */
function migrateSubscriptionSettings(
  raw: Partial<SubscriptionSettings> | undefined
): SubscriptionSettings {
  const base: SubscriptionSettings = { ...DEFAULT_SUBSCRIPTION_SETTINGS };
  if (!raw) return base;
  if (typeof raw.enabled === "boolean") base.enabled = raw.enabled;

  // New field wins if the user already set it.
  if (typeof raw.endpointUrl === "string" && raw.endpointUrl.length > 0) {
    base.endpointUrl = raw.endpointUrl;
    return base;
  }

  // Compat path: lift the legacy `wsUrl` into `endpointUrl`, rewriting
  // `ws(s)://` → `http(s)://` and stripping the `/ws` suffix the WS
  // transport needed.
  const legacy = (raw as { wsUrl?: unknown }).wsUrl;
  if (typeof legacy === "string" && legacy.length > 0) {
    base.endpointUrl = coerceLegacyWsUrl(legacy);
  }
  return base;
}

/** Exported for unit tests — see tests/settings-migration.test.ts. */
export function coerceLegacyWsUrl(raw: string): string {
  let url = raw.trim();
  if (url.startsWith("ws://")) url = "http://" + url.substring(5);
  else if (url.startsWith("wss://")) url = "https://" + url.substring(6);
  // Trim `/mcp/ws` → `/mcp` because HTTP has no separate websocket path.
  url = url.replace(/\/mcp\/ws(\/?)$/, "/mcp$1");
  return url;
}

function isPlainObject(v: unknown): boolean {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

/**
 * Build llm settings by layering (in order of precedence):
 *   1. Explicit `raw.llm` block (new-schema users)
 *   2. Legacy API keys stored under apiKeys (pre-schemaVersion 2 users)
 *   3. DEFAULT_LLM_SETTINGS
 */
function mergeLLMSettings(
  rawLlm: Partial<LLMManagerSettings> | undefined,
  legacyApiKeys: Record<string, string>
): LLMManagerSettings {
  const base: LLMManagerSettings = {
    priority: [...DEFAULT_LLM_SETTINGS.priority],
    providers: JSON.parse(JSON.stringify(DEFAULT_LLM_SETTINGS.providers)) as LLMManagerSettings["providers"],
  };

  // Layer in legacy keys (back-compat for one release).
  for (const { env, provider } of LEGACY_KEY_TO_PROVIDER) {
    const legacyVal = legacyApiKeys[env];
    if (legacyVal) base.providers[provider].apiKey = legacyVal;
  }

  if (!rawLlm) return base;

  const priority = Array.isArray(rawLlm.priority) && rawLlm.priority.length > 0
    ? [...(rawLlm.priority as ProviderId[])]
    : base.priority;

  const providers = base.providers;
  if (rawLlm.providers && isPlainObject(rawLlm.providers)) {
    for (const id of Object.keys(rawLlm.providers) as ProviderId[]) {
      const incoming = rawLlm.providers[id] as Partial<LLMProviderSettings> | undefined;
      if (!incoming) continue;
      providers[id] = {
        ...providers[id],
        ...incoming,
      };
    }
  }

  return { priority, providers };
}

/**
 * Propagate legacy env-style keys (the shape read by `loadSecretsEnv` out
 * of `secrets/.env`) onto the new `llm.providers.<id>.apiKey` path that
 * the provider classes actually consume.
 *
 * Returns the list of providers whose UI-set key differed from the env
 * value — caller should surface those as warnings so we don't silently
 * clobber a user's direct override.
 *
 * Addresses PR #26 adversarial review, blocker #4: prior to this helper,
 * `loadSecretsEnv` only updated `apiKeys[...]`, so key rotation via .env
 * was invisible to the LLM manager until a full plugin reload.
 */
export function syncLegacyApiKeys(settings: NLRSettings): Array<{
  provider: ProviderId;
  envKey: string;
  uiValueLength: number;
  envValueLength: number;
}> {
  const overrides: Array<{
    provider: ProviderId;
    envKey: string;
    uiValueLength: number;
    envValueLength: number;
  }> = [];

  for (const { env, provider } of LEGACY_KEY_TO_PROVIDER) {
    const envVal = settings.apiKeys[env];
    if (!envVal) continue;
    const providerCfg = settings.llm.providers[provider];
    if (!providerCfg) continue;
    const uiVal = providerCfg.apiKey;
    // If a user manually set a provider key in the UI that differs from
    // the env value, flag it but still prefer env — `.env` is the
    // authoritative "current deployment" source.
    if (uiVal && uiVal !== envVal) {
      overrides.push({
        provider,
        envKey: env,
        uiValueLength: uiVal.length,
        envValueLength: envVal.length,
      });
    }
    providerCfg.apiKey = envVal;
  }

  return overrides;
}

function providerLabel(id: ProviderId): string {
  switch (id) {
    case "openrouter":
      return "OpenRouter";
    case "anthropic":
      return "Anthropic";
    case "openai":
      return "OpenAI";
    case "local-llama":
      return "Local llama-server";
  }
}

export class NLRSettingTab extends PluginSettingTab {
  plugin: NLRPlugin;

  constructor(app: App, plugin: NLRPlugin) {
    super(app, plugin);
    this.plugin = plugin;
  }

  display(): void {
    const { containerEl } = this;
    containerEl.empty();

    this.renderPathsSection(containerEl);
    this.renderFolderAccessSection(containerEl);
    this.renderApiKeysSection(containerEl);
    this.renderLLMProvidersSection(containerEl);
    this.renderDispatcherSection(containerEl);
    this.renderHarnessSection(containerEl);
    this.renderMcpSection(containerEl);
    this.renderLoggingSection(containerEl);
    this.renderChatbotSection(containerEl);
    this.renderChatPanelSection(containerEl);
  }

  private renderLLMProvidersSection(containerEl: HTMLElement): void {
    containerEl.createEl("h2", { text: "LLM Providers" });
    containerEl.createEl("p", {
      text: "Configure which LLM backends the plugin can call. Priority order determines fallback on rate-limits or transient failures.",
      cls: "setting-item-description",
    });

    const llm = this.plugin.settings.llm;
    const providerIds: ProviderId[] = ["openrouter", "anthropic", "openai", "local-llama"];

    // Priority order — render as ordered list with up/down buttons.
    containerEl.createEl("h3", { text: "Priority / Fallback Order" });
    const orderEl = containerEl.createDiv({ cls: "nlr-provider-order" });
    this.renderPriorityList(orderEl, llm, providerIds);

    // Per-provider config
    for (const id of providerIds) {
      this.renderProviderBlock(containerEl, id);
    }
  }

  private renderPriorityList(
    orderEl: HTMLElement,
    llm: LLMManagerSettings,
    providerIds: ProviderId[]
  ): void {
    orderEl.empty();

    // Ensure the priority list includes every known provider (missing ones
    // append to the end in discovery order — this makes newly-added providers
    // visible without forcing the user to re-order).
    const seen = new Set(llm.priority);
    for (const id of providerIds) {
      if (!seen.has(id)) llm.priority.push(id);
    }

    for (let i = 0; i < llm.priority.length; i++) {
      const id = llm.priority[i];
      const setting = new Setting(orderEl).setName(`${i + 1}. ${providerLabel(id)}`);

      if (i > 0) {
        setting.addButton((btn: ButtonComponent) =>
          btn.setButtonText("Up").onClick(async () => {
            [llm.priority[i - 1], llm.priority[i]] = [llm.priority[i], llm.priority[i - 1]];
            await this.plugin.saveSettings();
            this.renderPriorityList(orderEl, llm, providerIds);
          })
        );
      }
      if (i < llm.priority.length - 1) {
        setting.addButton((btn: ButtonComponent) =>
          btn.setButtonText("Down").onClick(async () => {
            [llm.priority[i], llm.priority[i + 1]] = [llm.priority[i + 1], llm.priority[i]];
            await this.plugin.saveSettings();
            this.renderPriorityList(orderEl, llm, providerIds);
          })
        );
      }
    }
  }

  private renderProviderBlock(containerEl: HTMLElement, id: ProviderId): void {
    containerEl.createEl("h3", { text: providerLabel(id) });
    const cfg = this.plugin.settings.llm.providers[id];

    new Setting(containerEl)
      .setName("API Key")
      .setDesc(id === "local-llama" ? "Usually blank for local servers" : "Bearer token / API key")
      .addText((text) => {
        text.inputEl.type = "password";
        text.setValue(cfg.apiKey).onChange(async (v) => {
          cfg.apiKey = v;
          await this.plugin.saveSettings();
          this.plugin.refreshLLM();
        });
      });

    new Setting(containerEl)
      .setName("Base URL")
      .addText((text) =>
        text.setValue(cfg.baseUrl).onChange(async (v) => {
          cfg.baseUrl = v.trim();
          await this.plugin.saveSettings();
          this.plugin.refreshLLM();
        })
      );

    new Setting(containerEl)
      .setName("Default Model")
      .addText((text) =>
        text.setValue(cfg.defaultModel).onChange(async (v) => {
          cfg.defaultModel = v.trim();
          await this.plugin.saveSettings();
          this.plugin.refreshLLM();
        })
      );
  }

  private renderDispatcherSection(containerEl: HTMLElement): void {
    containerEl.createEl("h2", { text: "File-drop Task Dispatcher" });
    containerEl.createEl("p", {
      text: "On FileCreated under 00-neuro-link/ (top-level only), read frontmatter+body, call the primary LLM, and write a task spec under 00-neuro-link/tasks/.",
      cls: "setting-item-description",
    });

    const d = this.plugin.settings.dispatcher;

    new Setting(containerEl)
      .setName("Enabled")
      .addToggle((toggle) =>
        toggle.setValue(d.enabled).onChange(async (v) => {
          d.enabled = v;
          await this.plugin.saveSettings();
        })
      );

    new Setting(containerEl)
      .setName("Watch Glob")
      .setDesc("Glob pattern passed to tv_subscribe_vault_events")
      .addText((text) =>
        text.setValue(d.watchGlob).onChange(async (v) => {
          d.watchGlob = v.trim();
          await this.plugin.saveSettings();
        })
      );

    new Setting(containerEl)
      .setName("Task Output Dir")
      .setDesc("Vault-relative directory for generated task specs")
      .addText((text) =>
        text.setValue(d.taskOutputDir).onChange(async (v) => {
          d.taskOutputDir = v.trim();
          await this.plugin.saveSettings();
        })
      );

    new Setting(containerEl)
      .setName("Debounce (ms)")
      .setDesc("Wait this long after FileCreated before reading (avoid partial-write races)")
      .addText((text) =>
        text.setValue(String(d.debounceMs)).onChange(async (v) => {
          const n = parseInt(v, 10);
          if (!isNaN(n) && n >= 0) {
            d.debounceMs = n;
            await this.plugin.saveSettings();
          }
        })
      );

    new Setting(containerEl)
      .setName("Model Override")
      .setDesc("Leave blank to use the primary provider's default model")
      .addText((text) =>
        text.setPlaceholder("(use default)").setValue(d.model).onChange(async (v) => {
          d.model = v.trim();
          await this.plugin.saveSettings();
        })
      );

    containerEl.createEl("h3", { text: "Vault-events subscription" });
    const s = this.plugin.settings.subscription;

    new Setting(containerEl)
      .setName("Enabled")
      .setDesc("Long-poll the MCP endpoint at plugin load")
      .addToggle((toggle) =>
        toggle.setValue(s.enabled).onChange(async (v) => {
          s.enabled = v;
          await this.plugin.saveSettings();
        })
      );

    new Setting(containerEl)
      .setName("MCP endpoint URL")
      .setDesc("Leave blank to auto-derive from API Router Port")
      .addText((text) =>
        text
          .setPlaceholder(`http://localhost:${this.plugin.settings.apiRouterPort}/mcp`)
          .setValue(s.endpointUrl)
          .onChange(async (v) => {
            s.endpointUrl = v.trim();
            await this.plugin.saveSettings();
          })
      );
  }

  private renderPathsSection(containerEl: HTMLElement): void {
    containerEl.createEl("h2", { text: "Paths" });

    new Setting(containerEl)
      .setName("NLR Root")
      .setDesc("Path to neuro-link-recursive project root")
      .addText((text) =>
        text
          .setPlaceholder("/path/to/neuro-link-recursive")
          .setValue(this.plugin.settings.nlrRoot)
          .onChange(async (value) => {
            this.plugin.settings.nlrRoot = value;
            await this.plugin.saveSettings();
          })
      )
      .addButton((btn) =>
        btn.setButtonText("Auto-detect").onClick(async () => {
          const detected = this.plugin.detectNlrRoot();
          if (detected) {
            this.plugin.settings.nlrRoot = detected;
            await this.plugin.saveSettings();
            this.display();
            new Notice(`NLR root detected: ${detected}`);
          } else {
            new Notice("Could not auto-detect NLR root");
          }
        })
      );

    new Setting(containerEl)
      .setName("Neuro-Link Binary Path")
      .setDesc("Full path to the neuro-link CLI binary (auto-resolved if left default)")
      .addText((text) =>
        text
          .setPlaceholder("/usr/local/bin/neuro-link")
          .setValue(this.plugin.settings.nlrBinaryPath)
          .onChange(async (value) => {
            this.plugin.settings.nlrBinaryPath = value;
            await this.plugin.saveSettings();
          })
      );

    new Setting(containerEl)
      .setName("Obsidian Vault Path")
      .setDesc("Auto-detected from current vault")
      .addText((text) =>
        text
          .setValue(this.plugin.settings.vaultPath)
          .setDisabled(true)
      );
  }

  private renderFolderAccessSection(containerEl: HTMLElement): void {
    containerEl.createEl("h2", { text: "Folder Access" });
    containerEl.createEl("p", {
      text: "Select which folders the MCP server exposes to external clients. Default: all knowledge base folders.",
      cls: "setting-item-description",
    });

    const nlrRoot = this.plugin.settings.nlrRoot;
    if (!nlrRoot) {
      new Setting(containerEl)
        .setName("Set NLR Root first")
        .setDesc("Configure the NLR Root path above to manage folder access");
      return;
    }

    const ALL_FOLDERS = [
      { name: "00-raw", desc: "Raw ingested sources" },
      { name: "01-sorted", desc: "Classified raw material by domain" },
      { name: "02-KB-main", desc: "Wiki pages (sources of truth)" },
      { name: "03-ontology-main", desc: "Reasoning ontologies" },
      { name: "04-KB-agents-workflows", desc: "Per-agent/workflow knowledge" },
      { name: "05-insights-gaps", desc: "Knowledge gap reports" },
      { name: "05-self-improvement-HITL", desc: "Human-in-loop improvement" },
      { name: "06-self-improvement-recursive", desc: "Automated improvement" },
      { name: "06-progress-reports", desc: "Daily/weekly/monthly reports" },
      { name: "07-neuro-link-task", desc: "Task queue" },
      { name: "08-code-docs", desc: "Code documentation" },
      { name: "09-business-docs", desc: "Business documents" },
      { name: "config", desc: "Configuration files" },
    ];

    // Read current allowed_paths from config
    const configPath = path.join(nlrRoot, "config", "neuro-link.md");
    let currentAllowed: string[] = ALL_FOLDERS.map((f) => f.name); // default: all
    if (fs.existsSync(configPath)) {
      const content = fs.readFileSync(configPath, "utf-8");
      const match = content.match(/allowed_paths:\s*(.+)/);
      if (match && match[1].trim() !== "all") {
        currentAllowed = match[1].split(",").map((s) => s.trim()).filter(Boolean);
      }
    }

    for (const folder of ALL_FOLDERS) {
      const isEnabled = currentAllowed.includes(folder.name);
      new Setting(containerEl)
        .setName(folder.name)
        .setDesc(folder.desc)
        .addToggle((toggle) =>
          toggle.setValue(isEnabled).onChange(async (value) => {
            if (value && !currentAllowed.includes(folder.name)) {
              currentAllowed.push(folder.name);
            } else if (!value) {
              const idx = currentAllowed.indexOf(folder.name);
              if (idx >= 0) currentAllowed.splice(idx, 1);
            }
          })
        );
    }

    new Setting(containerEl)
      .addButton((btn) =>
        btn
          .setButtonText("Save Folder Access")
          .setCta()
          .onClick(async () => {
            await this.saveFolderAccess(currentAllowed);
          })
      );
  }

  private async saveFolderAccess(allowed: string[]): Promise<void> {
    const nlrRoot = this.plugin.settings.nlrRoot;
    if (!nlrRoot) return;

    const configPath = path.join(nlrRoot, "config", "neuro-link.md");
    if (!fs.existsSync(configPath)) {
      new Notice("neuro-link.md not found");
      return;
    }

    let content = fs.readFileSync(configPath, "utf-8");
    const allowedStr = allowed.join(", ");

    if (content.includes("allowed_paths:")) {
      content = content.replace(/allowed_paths:\s*.+/, `allowed_paths: ${allowedStr}`);
    } else {
      // Insert before closing ---
      content = content.replace(/\n---/, `\nallowed_paths: ${allowedStr}\n---`);
    }

    fs.writeFileSync(configPath, content, "utf-8");
    new Notice(`Folder access updated: ${allowed.length} folders enabled`);
  }

  private renderApiKeysSection(containerEl: HTMLElement): void {
    containerEl.createEl("h2", { text: "API Keys & Services" });

    // Auto-populate defaults on first load
    for (const def of API_KEY_DEFS) {
      if (def.defaultVal && !this.plugin.settings.apiKeys[def.key]) {
        this.plugin.settings.apiKeys[def.key] = def.defaultVal;
      }
    }

    let lastSection = "";
    for (const def of API_KEY_DEFS) {
      // Section headers
      const section =
        def.key.includes("OPENROUTER") || def.key.includes("ANTHROPIC") ? "LLM Providers" :
        def.key.includes("INFRANODUS") || def.key.includes("PARALLEL") ? "Knowledge & Research" :
        def.key.includes("EMBEDDING") || def.key.includes("QDRANT") || def.key.includes("NEO4J") ? "Local Infrastructure" :
        "Tunneling";
      if (section !== lastSection) {
        containerEl.createEl("h3", { text: section });
        lastSection = section;
      }

      const isPassword = !def.key.includes("URL") && !def.key.includes("URI");
      const setting = new Setting(containerEl)
        .setName(def.label)
        .setDesc(def.desc);

      setting.addText((text) => {
        const placeholder = def.defaultVal || def.key;
        text
          .setPlaceholder(placeholder)
          .setValue(this.plugin.settings.apiKeys[def.key] || "");
        if (isPassword) {
          text.inputEl.type = "password";
        }
        text.onChange(async (value) => {
          this.plugin.settings.apiKeys[def.key] = value;
          await this.plugin.saveSettings();
        });
      });

      setting.addButton((btn: ButtonComponent) =>
        btn
          .setButtonText("Test")
          .setCta()
          .onClick(async () => {
            await this.testApiKey(def.key);
          })
      );
    }

    new Setting(containerEl)
      .setName("Save to secrets/.env")
      .setDesc("Write all configured API keys to NLR_ROOT/secrets/.env")
      .addButton((btn) =>
        btn
          .setButtonText("Save")
          .setWarning()
          .onClick(async () => {
            await this.saveSecretsEnv();
          })
      );

    new Setting(containerEl)
      .setName("Load from secrets/.env")
      .setDesc("Read existing keys from NLR_ROOT/secrets/.env")
      .addButton((btn) =>
        btn
          .setButtonText("Load")
          .onClick(async () => {
            await this.loadSecretsEnv();
            this.display();
          })
      );
  }

  private renderHarnessSection(containerEl: HTMLElement): void {
    containerEl.createEl("h2", { text: "Harness Connections" });

    const harnesses = this.plugin.settings.harnesses;

    if (harnesses.length === 0) {
      new Setting(containerEl)
        .setName("No harnesses configured")
        .setDesc("Load from config or add manually")
        .addButton((btn) =>
          btn.setButtonText("Load from config").onClick(async () => {
            await this.loadHarnessesFromConfig();
            this.display();
          })
        );
    }

    for (let i = 0; i < harnesses.length; i++) {
      const h = harnesses[i];
      const setting = new Setting(containerEl)
        .setName(h.name)
        .setDesc(`${h.type} | ${h.role} | ${h.status}`);

      if (h.url) {
        setting.addButton((btn) =>
          btn.setButtonText("Test").setCta().onClick(async () => {
            await this.testHarnessConnection(h);
          })
        );
      }

      setting.addButton((btn) =>
        btn
          .setButtonText("Remove")
          .setWarning()
          .onClick(async () => {
            harnesses.splice(i, 1);
            await this.plugin.saveSettings();
            this.display();
          })
      );
    }

    new Setting(containerEl).addButton((btn) =>
      btn.setButtonText("Add Harness").onClick(() => {
        harnesses.push({
          name: "",
          type: "api",
          status: "disabled",
          url: "",
          apiKeyEnv: "",
          role: "",
          capabilities: [],
        });
        this.plugin.saveSettings();
        this.display();
      })
    );
  }

  private renderMcpSection(containerEl: HTMLElement): void {
    containerEl.createEl("h2", { text: "MCP Setup" });

    // Auto-populate mcp2cli profile path
    if (!this.plugin.settings.mcp2cliProfilePath && this.plugin.settings.nlrRoot) {
      this.plugin.settings.mcp2cliProfilePath = path.join(this.plugin.settings.nlrRoot, "mcp2cli-profile.json");
    }

    new Setting(containerEl)
      .setName("MCP Server Mode")
      .setDesc("Transport mode for MCP server")
      .addDropdown((drop) =>
        drop
          .addOption("stdio", "stdio")
          .addOption("http", "HTTP/SSE")
          .setValue(this.plugin.settings.mcpServerMode)
          .onChange(async (value) => {
            this.plugin.settings.mcpServerMode = value;
            await this.plugin.saveSettings();
          })
      );

    new Setting(containerEl)
      .setName("mcp2cli-rs Profile Path")
      .setDesc("Path to mcp2cli-rs profile JSON (auto-generated by MCP Setup wizard)")
      .addText((text) =>
        text
          .setPlaceholder(path.join(this.plugin.settings.nlrRoot || "/path/to/neuro-link", "mcp2cli-profile.json"))
          .setValue(this.plugin.settings.mcp2cliProfilePath)
          .onChange(async (value) => {
            this.plugin.settings.mcp2cliProfilePath = value;
            await this.plugin.saveSettings();
          })
      );

    new Setting(containerEl)
      .setName("API Router Port")
      .setDesc("Port for the NLR API router")
      .addText((text) =>
        text
          .setValue(String(this.plugin.settings.apiRouterPort))
          .onChange(async (value) => {
            const parsed = parseInt(value, 10);
            if (!isNaN(parsed) && parsed > 0 && parsed < 65536) {
              this.plugin.settings.apiRouterPort = parsed;
              await this.plugin.saveSettings();
            }
          })
      );

    new Setting(containerEl)
      .setName("Ngrok Domain")
      .setDesc("Custom Ngrok domain for stable remote URL (requires paid plan)")
      .addText((text) =>
        text
          .setPlaceholder("your-domain.ngrok-free.app")
          .setValue(this.plugin.settings.ngrokDomain)
          .onChange(async (value) => {
            this.plugin.settings.ngrokDomain = value;
            await this.plugin.saveSettings();
          })
      );

    // ── MCP Connection Info ──
    containerEl.createEl("h3", { text: "Connect External Services" });
    containerEl.createEl("p", {
      text: "Copy the config below into your AI tool's MCP settings to connect to this neuro-link instance.",
      cls: "setting-item-description",
    });

    const port = this.plugin.settings.apiRouterPort || 8080;
    const nlrRoot = this.plugin.settings.nlrRoot;
    let token = "";
    if (nlrRoot) {
      const envPath = path.join(nlrRoot, "secrets", ".env");
      if (fs.existsSync(envPath)) {
        const content = fs.readFileSync(envPath, "utf-8");
        const match = content.match(/NLR_API_TOKEN=(.+)/);
        if (match) token = match[1].trim();
      }
    }

    const binPath = this.plugin.resolveBinaryPath();

    // stdio config (Claude Code, Cursor, etc.)
    const stdioConfig = JSON.stringify({
      mcpServers: {
        "neuro-link": {
          type: "stdio",
          command: binPath,
          args: ["mcp"],
          env: { NLR_ROOT: nlrRoot || "/path/to/neuro-link" },
        },
      },
    }, null, 2);

    // HTTP config (remote clients, K-Dense, web-based tools)
    const baseUrl = this.plugin.settings.ngrokDomain
      ? `https://${this.plugin.settings.ngrokDomain}`
      : `http://localhost:${port}`;
    const httpConfig = JSON.stringify({
      mcpServers: {
        "neuro-link": {
          type: "http",
          url: `${baseUrl}/mcp`,
          headers: { Authorization: `Bearer ${token || "YOUR_TOKEN_HERE"}` },
        },
      },
    }, null, 2);

    const stdioPre = containerEl.createEl("div", { cls: "nlr-setup-step" });
    stdioPre.createEl("h4", { text: "For CLI tools (Claude Code, Cursor, Cline)" });
    stdioPre.createEl("pre", { cls: "nlr-result-pre" }).createEl("code", { text: stdioConfig });
    new Setting(stdioPre).addButton((btn) =>
      btn.setButtonText("Copy stdio config").setCta().onClick(async () => {
        await navigator.clipboard.writeText(stdioConfig);
        new Notice("stdio MCP config copied");
      })
    );

    const httpPre = containerEl.createEl("div", { cls: "nlr-setup-step" });
    httpPre.createEl("h4", { text: "For web/remote tools (K-Dense, ChatGPT Actions, remote CLI)" });
    httpPre.createEl("pre", { cls: "nlr-result-pre" }).createEl("code", { text: httpConfig });
    new Setting(httpPre).addButton((btn) =>
      btn.setButtonText("Copy HTTP config").setCta().onClick(async () => {
        await navigator.clipboard.writeText(httpConfig);
        new Notice("HTTP MCP config copied");
      })
    );

    // REST API info
    const restPre = containerEl.createEl("div", { cls: "nlr-setup-step" });
    restPre.createEl("h4", { text: "REST API (OpenAPI-compatible)" });
    restPre.createEl("pre", { cls: "nlr-result-pre" }).createEl("code", {
      text: `Base URL: ${baseUrl}/api/v1\nAuth: Bearer ${token ? token.substring(0, 8) + "..." : "YOUR_TOKEN"}\nHealth: ${baseUrl}/health (no auth)\nDocs: ${baseUrl}/api/v1/openapi.json`,
    });
  }

  private renderLoggingSection(containerEl: HTMLElement): void {
    containerEl.createEl("h2", { text: "Logging" });

    new Setting(containerEl)
      .setName("Session Logging")
      .setDesc("Log tool calls and responses to state/session_log.jsonl")
      .addToggle((toggle) =>
        toggle.setValue(this.plugin.settings.sessionLogging).onChange(async (value) => {
          this.plugin.settings.sessionLogging = value;
          await this.plugin.saveSettings();
        })
      );

    new Setting(containerEl)
      .setName("Score History")
      .setDesc("Record session grading scores to state/score_history.jsonl")
      .addToggle((toggle) =>
        toggle.setValue(this.plugin.settings.scoreHistory).onChange(async (value) => {
          this.plugin.settings.scoreHistory = value;
          await this.plugin.saveSettings();
        })
      );

    new Setting(containerEl)
      .setName("Auto Grade")
      .setDesc("Automatically grade sessions on completion")
      .addToggle((toggle) =>
        toggle.setValue(this.plugin.settings.autoGrade).onChange(async (value) => {
          this.plugin.settings.autoGrade = value;
          await this.plugin.saveSettings();
        })
      );
  }

  private renderChatbotSection(containerEl: HTMLElement): void {
    containerEl.createEl("h2", { text: "Chatbot" });

    new Setting(containerEl)
      .setName("Model")
      .setDesc("OpenRouter model identifier for chatbot")
      .addText((text) =>
        text
          .setPlaceholder("anthropic/claude-sonnet-4-20250514")
          .setValue(this.plugin.settings.chatbotModel)
          .onChange(async (value) => {
            this.plugin.settings.chatbotModel = value;
            await this.plugin.saveSettings();
          })
      );

    new Setting(containerEl)
      .setName("System Prompt")
      .setDesc("System prompt prepended to chatbot conversations")
      .addTextArea((text) =>
        text
          .setPlaceholder("You are an assistant...")
          .setValue(this.plugin.settings.chatbotSystemPrompt)
          .onChange(async (value) => {
            this.plugin.settings.chatbotSystemPrompt = value;
            await this.plugin.saveSettings();
          })
      );
  }

  private renderChatPanelSection(containerEl: HTMLElement): void {
    containerEl.createEl("h2", { text: "Chat Panel (@neuro)" });
    containerEl.createEl("p", {
      text: "Right-side panel with streaming chat + agent mode. Toggle with the ribbon icon or Cmd/Ctrl+Shift+K.",
      cls: "setting-item-description",
    });

    const cp = this.plugin.settings.chatPanel;

    new Setting(containerEl)
      .setName("Default Model")
      .setDesc("Model id for chat + agent turns. Leave blank to use the primary LLM provider's default.")
      .addText((text) =>
        text
          .setPlaceholder("(use primary provider default)")
          .setValue(cp.defaultModel)
          .onChange(async (v) => {
            cp.defaultModel = v.trim();
            await this.plugin.saveSettings();
          })
      );

    new Setting(containerEl)
      .setName("Max Transcript Turns")
      .setDesc("Oldest turns beyond this cap are detached from the view and the outgoing LLM context.")
      .addText((text) =>
        text
          .setValue(String(cp.maxTranscriptTurns))
          .onChange(async (v) => {
            const n = parseInt(v, 10);
            if (!isNaN(n) && n >= 2 && n <= 500) {
              cp.maxTranscriptTurns = n;
              await this.plugin.saveSettings();
            }
          })
      );

    new Setting(containerEl)
      .setName("Auto-scroll")
      .setDesc("Automatically scroll the message list to the bottom on new content.")
      .addToggle((toggle) =>
        toggle.setValue(cp.autoScroll).onChange(async (v) => {
          cp.autoScroll = v;
          await this.plugin.saveSettings();
        })
      );
  }

  private async testApiKey(keyName: string): Promise<void> {
    const value = this.plugin.settings.apiKeys[keyName];
    const def = API_KEY_DEFS.find((d) => d.key === keyName);
    const label = def?.label || keyName;
    const test = def?.test || "key-saved";

    if (!value) {
      new Notice(`${label}: not set`);
      return;
    }

    try {
      // ── key-saved: no test possible, just confirm saved ──
      if (test === "key-saved") {
        new Notice(`${label}: saved \u2713`);
        return;
      }

      // ── key-format:prefix — validate key starts with expected prefix ──
      if (test.startsWith("key-format:")) {
        const prefix = test.substring(11);
        if (value.startsWith(prefix)) {
          new Notice(`${label}: format valid (${prefix}...) \u2713`);
        } else {
          new Notice(`${label}: saved (expected prefix: ${prefix})`);
        }
        return;
      }

      // ── format:prefix — validate URL/URI format ──
      if (test.startsWith("format:")) {
        const prefix = test.substring(7);
        new Notice(value.startsWith(prefix)
          ? `${label}: ${value} \u2713`
          : `${label}: expected ${prefix} prefix`);
        return;
      }

      // ── local-url: test local service connectivity ──
      if (test === "local-url") {
        let url = value;
        // Qdrant has /healthz
        if (keyName === "QDRANT_URL") url = value.replace(/\/$/, "") + "/healthz";
        // Embedding server: strip /v1/embeddings, try base
        else if (keyName === "EMBEDDING_API_URL") url = value.replace(/\/v1\/embeddings\/?$/, "");
        // Neo4j HTTP: test root
        // else use value as-is

        try {
          const resp = await fetch(url);
          new Notice(`${label}: connected (${resp.status}) \u2713`);
        } catch {
          const hint = keyName === "EMBEDDING_API_URL"
            ? " — start with: ./scripts/embedding-server.sh"
            : keyName === "QDRANT_URL"
            ? " — run: docker start qdrant-nlr"
            : keyName === "NEO4J_HTTP_URL"
            ? " — run: docker start neo4j-nlr"
            : "";
          new Notice(`${label}: not reachable${hint}`);
        }
        return;
      }

      // ── openrouter: known working test endpoint ──
      if (test === "openrouter") {
        const resp = await fetch("https://openrouter.ai/api/v1/models", {
          headers: { Authorization: `Bearer ${value}` },
        });
        if (resp.ok) {
          new Notice(`${label}: connected \u2713`);
        } else {
          new Notice(`${label}: HTTP ${resp.status} — check your key at openrouter.ai/settings/keys`);
        }
        return;
      }

      // ── firecrawl: POST-only API, test with a minimal scrape ──
      if (test === "firecrawl") {
        const resp = await fetch("https://api.firecrawl.dev/v1/scrape", {
          method: "POST",
          headers: {
            Authorization: `Bearer ${value}`,
            "Content-Type": "application/json",
          },
          body: JSON.stringify({ url: "https://example.com", formats: ["markdown"], onlyMainContent: true }),
        });
        if (resp.ok || resp.status === 200 || resp.status === 201) {
          new Notice(`${label}: connected \u2713`);
        } else if (resp.status === 401 || resp.status === 403) {
          new Notice(`${label}: invalid key (${resp.status})`);
        } else if (resp.status === 402) {
          new Notice(`${label}: key valid but out of credits (${resp.status})`);
        } else {
          new Notice(`${label}: HTTP ${resp.status}`);
        }
        return;
      }

      // ── ngrok: configure auth token via CLI ──
      if (test === "ngrok") {
        try {
          await execFileAsync("ngrok", ["config", "add-authtoken", value]);
          new Notice(`${label}: configured \u2713`);
        } catch {
          // ngrok not on Electron PATH, try common locations
          const ngrokPaths = ["/usr/local/bin/ngrok", "/opt/homebrew/bin/ngrok"];
          let configured = false;
          for (const p of ngrokPaths) {
            if (fs.existsSync(p)) {
              try {
                await execFileAsync(p, ["config", "add-authtoken", value]);
                new Notice(`${label}: configured \u2713`);
                configured = true;
                break;
              } catch { /* try next */ }
            }
          }
          if (!configured) {
            new Notice(`${label}: saved — run in terminal: ngrok config add-authtoken ${value.substring(0, 8)}...`);
          }
        }
        return;
      }

      new Notice(`${label}: saved \u2713`);
    } catch (e: unknown) {
      const err = e as Error;
      new Notice(`${label}: error — ${err.message}`);
    }
  }

  private async testHarnessConnection(harness: HarnessConfig): Promise<void> {
    if (!harness.url) {
      new Notice(`${harness.name}: no URL configured`);
      return;
    }
    try {
      const response = await fetch(harness.url);
      new Notice(`${harness.name}: ${response.ok ? "OK" : response.status}`);
    } catch (e: unknown) {
      const err = e as Error;
      new Notice(`${harness.name}: unreachable - ${err.message}`);
    }
  }

  private async saveSecretsEnv(): Promise<void> {
    const nlrRoot = this.plugin.settings.nlrRoot;
    if (!nlrRoot) {
      new Notice("NLR Root path not set");
      return;
    }

    const secretsDir = path.join(nlrRoot, "secrets");
    if (!fs.existsSync(secretsDir)) {
      fs.mkdirSync(secretsDir, { recursive: true });
    }

    const envPath = path.join(secretsDir, ".env");
    const lines: string[] = [
      "# neuro-link-recursive secrets",
      `# Generated by Obsidian plugin at ${new Date().toISOString()}`,
      "",
    ];

    for (const def of API_KEY_DEFS) {
      const value = this.plugin.settings.apiKeys[def.key] || "";
      if (value) {
        lines.push(`${def.key}=${value}`);
      }
    }

    fs.writeFileSync(envPath, lines.join("\n") + "\n", "utf-8");
    new Notice(`Saved ${lines.length - 3} keys to ${envPath}`);
  }

  private async loadSecretsEnv(): Promise<void> {
    const nlrRoot = this.plugin.settings.nlrRoot;
    if (!nlrRoot) {
      new Notice("NLR Root path not set");
      return;
    }

    const envPath = path.join(nlrRoot, "secrets", ".env");
    if (!fs.existsSync(envPath)) {
      new Notice("secrets/.env not found");
      return;
    }

    const content = fs.readFileSync(envPath, "utf-8");
    let loaded = 0;

    for (const line of content.split("\n")) {
      const trimmed = line.trim();
      if (!trimmed || trimmed.startsWith("#")) continue;
      const eqIdx = trimmed.indexOf("=");
      if (eqIdx === -1) continue;
      const key = trimmed.substring(0, eqIdx).trim();
      const value = trimmed.substring(eqIdx + 1).trim();
      if (API_KEY_DEFS.some((d) => d.key === key)) {
        this.plugin.settings.apiKeys[key] = value;
        loaded++;
      }
    }

    // Propagate env-style keys onto the new `llm.providers.*.apiKey` path so
    // the LLM manager actually sees rotated keys without a plugin reload.
    // Addresses PR #26 adversarial review, blocker #4.
    const overrides = syncLegacyApiKeys(this.plugin.settings);
    for (const o of overrides) {
      new Notice(
        `Neuro-Link: ${providerLabel(o.provider)} key from secrets/.env differs from the value in the UI — env value wins. Update the UI field if this was unintended.`,
        10000
      );
      console.warn(
        `NLR settings: ${o.envKey} in secrets/.env diverges from llm.providers.${o.provider}.apiKey (ui len=${o.uiValueLength}, env len=${o.envValueLength}) — env preferred`
      );
    }

    await this.plugin.saveSettings();
    // Refresh the manager so in-flight callers pick up the new keys.
    this.plugin.refreshLLM();
    new Notice(`Loaded ${loaded} keys from secrets/.env`);
  }

  private async loadHarnessesFromConfig(): Promise<void> {
    const nlrRoot = this.plugin.settings.nlrRoot;
    if (!nlrRoot) {
      new Notice("NLR Root path not set");
      return;
    }

    const configPath = path.join(nlrRoot, "config", "harness-harness-comms.md");
    if (!fs.existsSync(configPath)) {
      new Notice("harness-harness-comms.md not found");
      return;
    }

    const content = fs.readFileSync(configPath, "utf-8");
    const fmMatch = content.match(/^---\n([\s\S]*?)\n---/);
    if (!fmMatch) {
      new Notice("No frontmatter found in harness config");
      return;
    }

    const fm = fmMatch[1];
    const harnesses: HarnessConfig[] = [];
    const harnessBlock = fm.match(/harnesses:\n([\s\S]*?)(?=routing_rules:|$)/);

    if (harnessBlock) {
      const entries = harnessBlock[1].matchAll(
        /\s{2}(\S+):\n([\s\S]*?)(?=\n\s{2}\S+:|\n[a-z]|$)/g
      );
      for (const entry of entries) {
        const name = entry[1];
        const block = entry[2];
        const getVal = (key: string): string => {
          const m = block.match(new RegExp(`${key}:\\s*(.+)`));
          return m ? m[1].trim() : "";
        };
        const capsMatch = block.match(/capabilities:\n((?:\s+-\s+.+\n?)*)/);
        const capabilities = capsMatch
          ? capsMatch[1]
              .split("\n")
              .map((l) => l.replace(/^\s+-\s+/, "").trim())
              .filter(Boolean)
          : [];

        harnesses.push({
          name,
          type: getVal("type"),
          status: getVal("status"),
          url: getVal("url") || "",
          apiKeyEnv: getVal("api_key_env"),
          role: getVal("role"),
          capabilities,
        });
      }
    }

    this.plugin.settings.harnesses = harnesses;
    await this.plugin.saveSettings();
    new Notice(`Loaded ${harnesses.length} harnesses from config`);
  }
}
