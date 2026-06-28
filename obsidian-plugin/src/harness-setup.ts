import { Modal, App, Setting, Notice, DropdownComponent } from "obsidian";
import type NLRPlugin from "./main";
import type { HarnessConfig } from "./settings";
import * as fs from "fs";
import * as path from "path";

export class HarnessSetupModal extends Modal {
  private plugin: NLRPlugin;
  private harness: HarnessConfig;
  private isNew: boolean;

  constructor(app: App, plugin: NLRPlugin, harness?: HarnessConfig) {
    super(app);
    this.plugin = plugin;
    this.isNew = !harness;
    this.harness = harness
      ? { ...harness }
      : {
          name: "",
          type: "api",
          status: "disabled",
          url: "",
          apiKeyEnv: "",
          role: "",
          capabilities: [],
        };
  }

  onOpen(): void {
    const { contentEl } = this;
    contentEl.createEl("h3", { text: this.isNew ? "Add Harness" : `Edit: ${this.harness.name}` });

    new Setting(contentEl)
      .setName("Name")
      .setDesc("Unique identifier for this harness")
      .addText((text) =>
        text
          .setPlaceholder("my-harness")
          .setValue(this.harness.name)
          .onChange((v) => { this.harness.name = v; })
      );

    new Setting(contentEl)
      .setName("Type")
      .addDropdown((drop: DropdownComponent) =>
        drop
          .addOption("local", "Local CLI")
          .addOption("api", "API (HTTP)")
          .addOption("mcp", "MCP Server")
          .setValue(this.harness.type)
          .onChange((v) => { this.harness.type = v; })
      );

    new Setting(contentEl)
      .setName("Status")
      .addDropdown((drop: DropdownComponent) =>
        drop
          .addOption("active", "Active")
          .addOption("disabled", "Disabled")
          .addOption("error", "Error")
          .setValue(this.harness.status)
          .onChange((v) => { this.harness.status = v; })
      );

    new Setting(contentEl)
      .setName("URL")
      .setDesc("API endpoint or MCP server URL (leave empty for local CLI)")
      .addText((text) =>
        text
          .setPlaceholder("http://localhost:8000")
          .setValue(this.harness.url)
          .onChange((v) => { this.harness.url = v; })
      );

    new Setting(contentEl)
      .setName("API Key Env Variable")
      .setDesc("Environment variable name for the API key")
      .addText((text) =>
        text
          .setPlaceholder("MY_HARNESS_API_KEY")
          .setValue(this.harness.apiKeyEnv)
          .onChange((v) => { this.harness.apiKeyEnv = v; })
      );

    new Setting(contentEl)
      .setName("Role")
      .addDropdown((drop: DropdownComponent) =>
        drop
          .addOption("primary", "Primary")
          .addOption("research", "Research")
          .addOption("implementation", "Implementation")
          .addOption("review", "Review")
          .addOption("monitoring", "Monitoring")
          .setValue(this.harness.role || "research")
          .onChange((v) => { this.harness.role = v; })
      );

    new Setting(contentEl)
      .setName("Capabilities")
      .setDesc("Comma-separated list of capabilities")
      .addText((text) =>
        text
          .setPlaceholder("code_generation, testing, review")
          .setValue(this.harness.capabilities.join(", "))
          .onChange((v) => {
            this.harness.capabilities = v
              .split(",")
              .map((s) => s.trim())
              .filter(Boolean);
          })
      );

    const btnRow = contentEl.createDiv({ cls: "nlr-modal-btn-row" });

    if (this.harness.url) {
      new Setting(btnRow).addButton((btn) =>
        btn
          .setButtonText("Test Connection")
          .setCta()
          .onClick(async () => {
            await this.testConnection();
          })
      );
    }

    new Setting(btnRow).addButton((btn) =>
      btn
        .setButtonText("Save")
        .setCta()
        .onClick(async () => {
          await this.save();
        })
    );

    new Setting(btnRow).addButton((btn) =>
      btn
        .setButtonText("Save to Config")
        .setWarning()
        .onClick(async () => {
          await this.save();
          await this.writeToConfig();
        })
    );

    this.renderRoutingRules(contentEl);
  }

  private renderRoutingRules(contentEl: HTMLElement): void {
    contentEl.createEl("h4", { text: "Routing Rules" });

    const nlrRoot = this.plugin.settings.nlrRoot;
    if (!nlrRoot) {
      contentEl.createEl("p", { text: "Set NLR Root to view routing rules", cls: "nlr-stats-muted" });
      return;
    }

    const configPath = path.join(nlrRoot, "config", "harness-harness-comms.md");
    if (!fs.existsSync(configPath)) {
      contentEl.createEl("p", { text: "harness-harness-comms.md not found", cls: "nlr-stats-muted" });
      return;
    }

    const content = fs.readFileSync(configPath, "utf-8");
    const fmMatch = content.match(/^---\n([\s\S]*?)\n---/);
    if (!fmMatch) return;

    const rulesMatch = fmMatch[1].match(/routing_rules:\n([\s\S]*?)$/);
    if (!rulesMatch) return;

    const rules: Array<{ pattern: string; route_to: string }> = [];
    const ruleEntries = rulesMatch[1].matchAll(/- pattern:\s*"([^"]+)"\n\s+route_to:\s*(\S+)/g);
    for (const m of ruleEntries) {
      rules.push({ pattern: m[1], route_to: m[2] });
    }

    if (rules.length === 0) {
      contentEl.createEl("p", { text: "No routing rules defined", cls: "nlr-stats-muted" });
      return;
    }

    const table = contentEl.createEl("table", { cls: "nlr-stats-table" });
    const thead = table.createEl("thead");
    const headerRow = thead.createEl("tr");
    headerRow.createEl("th", { text: "Pattern" });
    headerRow.createEl("th", { text: "Route To" });

    const tbody = table.createEl("tbody");
    for (const rule of rules) {
      const row = tbody.createEl("tr");
      row.createEl("td", { text: rule.pattern });
      const routeCell = row.createEl("td", { text: rule.route_to });
      if (rule.route_to === this.harness.name) {
        routeCell.addClass("nlr-stats-highlight");
      }
    }
  }

  private async testConnection(): Promise<void> {
    if (!this.harness.url) {
      new Notice("No URL configured");
      return;
    }
    try {
      const response = await fetch(this.harness.url);
      new Notice(`${this.harness.name}: ${response.ok ? "Connected" : `HTTP ${response.status}`}`);
    } catch (e: unknown) {
      const err = e as Error;
      new Notice(`${this.harness.name}: unreachable - ${err.message}`);
    }
  }

  private async save(): Promise<void> {
    if (!this.harness.name) {
      new Notice("Harness name is required");
      return;
    }

    const harnesses = this.plugin.settings.harnesses;
    const existingIdx = harnesses.findIndex((h) => h.name === this.harness.name);
    if (existingIdx >= 0) {
      harnesses[existingIdx] = { ...this.harness };
    } else {
      harnesses.push({ ...this.harness });
    }

    await this.plugin.saveSettings();
    new Notice(`Harness "${this.harness.name}" saved`);
    this.close();
  }

  private async writeToConfig(): Promise<void> {
    const nlrRoot = this.plugin.settings.nlrRoot;
    if (!nlrRoot) {
      new Notice("NLR Root not set");
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
      new Notice("No frontmatter found in config");
      return;
    }

    const h = this.harness;
    const yamlBlock = [
      `  ${h.name}:`,
      `    type: ${h.type}`,
      `    status: ${h.status}`,
      `    role: ${h.role}`,
    ];
    if (h.url) yamlBlock.push(`    url: ${h.url}`);
    if (h.apiKeyEnv) yamlBlock.push(`    api_key_env: ${h.apiKeyEnv}`);
    if (h.capabilities.length > 0) {
      yamlBlock.push("    capabilities:");
      for (const cap of h.capabilities) {
        yamlBlock.push(`      - ${cap}`);
      }
    }
    const newBlock = yamlBlock.join("\n");

    let fm = fmMatch[1];
    const existingPattern = new RegExp(`  ${h.name}:\\n(?:    .+\\n)*`, "g");
    if (existingPattern.test(fm)) {
      fm = fm.replace(existingPattern, newBlock + "\n");
    } else {
      const routingIdx = fm.indexOf("routing_rules:");
      if (routingIdx >= 0) {
        fm = fm.substring(0, routingIdx) + newBlock + "\n" + fm.substring(routingIdx);
      } else {
        fm += "\n" + newBlock;
      }
    }

    const body = content.substring(fmMatch[0].length);
    fs.writeFileSync(configPath, `---\n${fm}\n---${body}`, "utf-8");
    new Notice(`Written ${h.name} to harness config`);
  }

  onClose(): void {
    this.contentEl.empty();
  }
}
