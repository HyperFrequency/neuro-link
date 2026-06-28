import { Modal, App, Setting, Notice } from "obsidian";
import type NLRPlugin from "./main";
import * as fs from "fs";
import * as path from "path";

export class McpSetupModal extends Modal {
  private plugin: NLRPlugin;

  constructor(app: App, plugin: NLRPlugin) {
    super(app);
    this.plugin = plugin;
  }

  onOpen(): void {
    const { contentEl } = this;
    contentEl.addClass("nlr-mcp-setup-modal");
    contentEl.createEl("h3", { text: "MCP Server Setup" });

    this.renderStep1(contentEl);
    this.renderStep2(contentEl);
    this.renderStep3(contentEl);
    this.renderStep4(contentEl);
    this.renderStep5(contentEl);
  }

  private renderStep1(contentEl: HTMLElement): void {
    const section = contentEl.createDiv({ cls: "nlr-setup-step" });
    section.createEl("h4", { text: "Step 1: Install NLR Binary" });

    const nlrBin = this.plugin.settings.nlrBinaryPath || "neuro-link";
    const statusEl = section.createDiv({ cls: "nlr-setup-status" });

    new Setting(section)
      .setName("Check Installation")
      .setDesc(`Current binary path: ${nlrBin}`)
      .addButton((btn) =>
        btn
          .setButtonText("Verify")
          .setCta()
          .onClick(async () => {
            try {
              await this.plugin.runNlrCommand(["--version"]);
              statusEl.empty();
              statusEl.createEl("span", { text: "\u2713 neuro-link binary found", cls: "nlr-stats-success" });
            } catch {
              statusEl.empty();
              statusEl.createEl("span", { text: "\u2717 neuro-link binary not found", cls: "nlr-stats-failure" });
            }
          })
      );

    const installInstructions = section.createDiv({ cls: "nlr-setup-instructions" });
    installInstructions.createEl("p", { text: "Install via Cargo:" });
    const codeBlock = installInstructions.createEl("pre", { cls: "nlr-result-pre" });
    codeBlock.createEl("code", {
      text: "cargo install neuro-link-mcp\n\n# Or build from source:\ncd server && cargo build --release\ncp target/release/neuro-link ~/.cargo/bin/neuro-link",
    });
  }

  private renderStep2(contentEl: HTMLElement): void {
    const section = contentEl.createDiv({ cls: "nlr-setup-step" });
    section.createEl("h4", { text: "Step 2: Configure Claude Code MCP Server" });

    const nlrRoot = this.plugin.settings.nlrRoot || "/path/to/neuro-link-recursive";
    const nlrBin = this.plugin.settings.nlrBinaryPath || "neuro-link";

    const mcpConfig = JSON.stringify(
      {
        mcpServers: {
          "neuro-link-recursive": {
            type: "stdio",
            command: nlrBin,
            args: ["mcp"],
            env: { NLR_ROOT: nlrRoot },
          },
        },
      },
      null,
      2
    );

    section.createEl("p", { text: "Add this to ~/.claude.json:" });
    const codeBlock = section.createEl("pre", { cls: "nlr-result-pre" });
    codeBlock.createEl("code", { text: mcpConfig });

    new Setting(section).addButton((btn) =>
      btn
        .setButtonText("Copy to Clipboard")
        .setCta()
        .onClick(async () => {
          await navigator.clipboard.writeText(mcpConfig);
          new Notice("MCP config copied to clipboard");
        })
    );

    new Setting(section).addButton((btn) =>
      btn
        .setButtonText("Auto-add to ~/.claude.json")
        .setWarning()
        .onClick(async () => {
          await this.addToClaudeJson(nlrBin, nlrRoot);
        })
    );
  }

  private renderStep3(contentEl: HTMLElement): void {
    const section = contentEl.createDiv({ cls: "nlr-setup-step" });
    section.createEl("h4", { text: "Step 3: mcp2cli-rs Profile" });

    section.createEl("p", {
      text: "mcp2cli-rs converts MCP tool calls to CLI commands. Generate a profile for NLR:",
    });

    const profilePath = this.plugin.settings.mcp2cliProfilePath
      || path.join(this.plugin.settings.nlrRoot || "", "mcp2cli-profile.json");

    new Setting(section)
      .setName("Profile Path")
      .addText((text) =>
        text.setValue(profilePath).setDisabled(true)
      );

    const statusEl = section.createDiv({ cls: "nlr-setup-status" });

    new Setting(section).addButton((btn) =>
      btn
        .setButtonText("Generate Profile")
        .setCta()
        .onClick(async () => {
          await this.generateMcp2cliProfile(profilePath);
          statusEl.empty();
          statusEl.createEl("span", { text: "\u2713 Profile generated", cls: "nlr-stats-success" });
        })
    );

    new Setting(section).addButton((btn) =>
      btn
        .setButtonText("View Current Profile")
        .onClick(async () => {
          if (fs.existsSync(profilePath)) {
            const content = fs.readFileSync(profilePath, "utf-8");
            const pre = section.createEl("pre", { cls: "nlr-result-pre" });
            pre.createEl("code", { text: content });
          } else {
            new Notice("Profile not found at " + profilePath);
          }
        })
    );
  }

  private renderStep4(contentEl: HTMLElement): void {
    const section = contentEl.createDiv({ cls: "nlr-setup-step" });
    section.createEl("h4", { text: "Step 4: Connect External MCP Clients" });

    section.createEl("p", {
      text: "The server auto-starts when the plugin loads. External MCP clients connect via HTTP.",
    });

    const port = this.plugin.settings.apiRouterPort || 8080;

    // Read token from secrets/.env
    let token = "(not set)";
    const nlrRoot = this.plugin.settings.nlrRoot;
    if (nlrRoot) {
      const envPath = path.join(nlrRoot, "secrets", ".env");
      if (fs.existsSync(envPath)) {
        const content = fs.readFileSync(envPath, "utf-8");
        const match = content.match(/NLR_API_TOKEN=(.+)/);
        if (match) token = match[1].trim();
      }
    }

    section.createEl("p", { text: `Server: http://localhost:${port}` });
    section.createEl("p", { text: `Token: ${token.substring(0, 8)}...` });

    const mcpClientConfig = JSON.stringify({
      mcpServers: {
        "neuro-link-recursive": {
          type: "http",
          url: `http://localhost:${port}/mcp`,
          headers: {
            Authorization: `Bearer ${token}`,
          },
        },
      },
    }, null, 2);

    section.createEl("p", { text: "For HTTP MCP clients (add to their config):" });
    const codeBlock = section.createEl("pre", { cls: "nlr-result-pre" });
    codeBlock.createEl("code", { text: mcpClientConfig });

    new Setting(section).addButton((btn) =>
      btn
        .setButtonText("Copy MCP Config")
        .setCta()
        .onClick(async () => {
          await navigator.clipboard.writeText(mcpClientConfig);
          new Notice("MCP client config copied to clipboard");
        })
    );

    new Setting(section)
      .setName("Port")
      .addText((text) =>
        text
          .setValue(String(port))
          .onChange(async (v) => {
            const p = parseInt(v, 10);
            if (!isNaN(p) && p > 0 && p < 65536) {
              this.plugin.settings.apiRouterPort = p;
              await this.plugin.saveSettings();
            }
          })
      );
  }

  private renderStep5(contentEl: HTMLElement): void {
    const section = contentEl.createDiv({ cls: "nlr-setup-step" });
    section.createEl("h4", { text: "Step 5: Ngrok Tunnel (Optional)" });

    section.createEl("p", {
      text: "Expose the API router over HTTPS for remote harness communication.",
    });

    new Setting(section)
      .setName("Ngrok Domain")
      .addText((text) =>
        text
          .setPlaceholder("your-domain.ngrok-free.app")
          .setValue(this.plugin.settings.ngrokDomain)
          .onChange(async (v) => {
            this.plugin.settings.ngrokDomain = v;
            await this.plugin.saveSettings();
          })
      );

    const ngrokCmd = this.plugin.settings.ngrokDomain
      ? `ngrok http ${this.plugin.settings.apiRouterPort} --domain=${this.plugin.settings.ngrokDomain}`
      : `ngrok http ${this.plugin.settings.apiRouterPort}`;

    const pre = section.createEl("pre", { cls: "nlr-result-pre" });
    pre.createEl("code", { text: ngrokCmd });

    new Setting(section).addButton((btn) =>
      btn
        .setButtonText("Copy Command")
        .onClick(async () => {
          await navigator.clipboard.writeText(ngrokCmd);
          new Notice("Ngrok command copied");
        })
    );

    new Setting(section).addButton((btn) =>
      btn
        .setButtonText("Start via NLR")
        .setCta()
        .onClick(async () => {
          try {
            const result = await this.plugin.runNlrCommand(["ngrok"]);
            new Notice("Ngrok started");
            section.createEl("pre", { cls: "nlr-result-pre" }).createEl("code", { text: result });
          } catch (e: unknown) {
            const err = e as Error;
            new Notice(`Ngrok failed: ${err.message}`);
          }
        })
    );
  }

  private async addToClaudeJson(nlrBin: string, nlrRoot: string): Promise<void> {
    const home = process.env.HOME || process.env.USERPROFILE || "";
    const claudeJsonPath = path.join(home, ".claude.json");

    let existing: Record<string, unknown> = {};
    if (fs.existsSync(claudeJsonPath)) {
      try {
        existing = JSON.parse(fs.readFileSync(claudeJsonPath, "utf-8")) as Record<string, unknown>;
      } catch {
        new Notice("Failed to parse existing ~/.claude.json");
        return;
      }
    }

    const mcpServers = (existing["mcpServers"] || {}) as Record<string, unknown>;
    mcpServers["neuro-link-recursive"] = {
      type: "stdio",
      command: nlrBin,
      args: ["mcp"],
      env: { NLR_ROOT: nlrRoot },
    };
    existing["mcpServers"] = mcpServers;

    fs.writeFileSync(claudeJsonPath, JSON.stringify(existing, null, 2) + "\n", "utf-8");
    new Notice("Added neuro-link-recursive to ~/.claude.json");
  }

  private async generateMcp2cliProfile(profilePath: string): Promise<void> {
    const profile = {
      profile: "neuro-link-recursive",
      version: 1,
      transport: {
        type: "stdio",
        command: this.plugin.settings.nlrBinaryPath || "nlr",
        args: ["mcp"],
      },
      tools: [
        { mcp_name: "nlr_wiki_create", cli_name: "wiki-create" },
        { mcp_name: "nlr_wiki_read", cli_name: "wiki-read" },
        { mcp_name: "nlr_wiki_update", cli_name: "wiki-update" },
        { mcp_name: "nlr_wiki_list", cli_name: "wiki-list" },
        { mcp_name: "nlr_wiki_search", cli_name: "wiki-search" },
        { mcp_name: "nlr_rag_query", cli_name: "rag-query" },
        { mcp_name: "nlr_rag_rebuild_index", cli_name: "rag-rebuild" },
        { mcp_name: "nlr_ontology_generate", cli_name: "ontology-generate" },
        { mcp_name: "nlr_ontology_query", cli_name: "ontology-query" },
        { mcp_name: "nlr_ontology_gaps", cli_name: "ontology-gaps" },
        { mcp_name: "nlr_ingest", cli_name: "ingest" },
        { mcp_name: "nlr_ingest_classify", cli_name: "ingest-classify" },
        { mcp_name: "nlr_ingest_dedup", cli_name: "ingest-dedup" },
        { mcp_name: "nlr_task_list", cli_name: "task-list" },
        { mcp_name: "nlr_task_create", cli_name: "task-create" },
        { mcp_name: "nlr_task_update", cli_name: "task-update" },
        { mcp_name: "nlr_harness_dispatch", cli_name: "harness-dispatch" },
        { mcp_name: "nlr_harness_list", cli_name: "harness-list" },
        { mcp_name: "nlr_scan_health", cli_name: "scan-health" },
        { mcp_name: "nlr_scan_staleness", cli_name: "scan-staleness" },
        { mcp_name: "nlr_state_heartbeat", cli_name: "state-heartbeat" },
        { mcp_name: "nlr_state_log", cli_name: "state-log" },
        { mcp_name: "nlr_config_read", cli_name: "config-read" },
      ],
    };

    const dir = path.dirname(profilePath);
    if (!fs.existsSync(dir)) {
      fs.mkdirSync(dir, { recursive: true });
    }

    fs.writeFileSync(profilePath, JSON.stringify(profile, null, 2) + "\n", "utf-8");
    new Notice(`mcp2cli profile written to ${profilePath}`);
  }

  onClose(): void {
    this.contentEl.empty();
  }
}
