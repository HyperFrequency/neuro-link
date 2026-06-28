import { Modal, App, Setting, Notice, DropdownComponent } from "obsidian";
import type NLRPlugin from "./main";
import * as fs from "fs";
import * as path from "path";

interface ApiRoute {
  keyName: string;
  provider: string;
  endpoint: string;
}

const PROVIDERS = [
  { value: "openrouter", label: "OpenRouter", endpoint: "https://openrouter.ai/api/v1" },
  { value: "anthropic", label: "Anthropic", endpoint: "https://api.anthropic.com/v1" },
  { value: "openai", label: "OpenAI", endpoint: "https://api.openai.com/v1" },
  { value: "kdense", label: "K-Dense", endpoint: "http://localhost:8000" },
  { value: "modal", label: "Modal", endpoint: "https://api.modal.com" },
  { value: "custom", label: "Custom", endpoint: "" },
];

export class ApiRouterModal extends Modal {
  private plugin: NLRPlugin;
  private routes: ApiRoute[];

  constructor(app: App, plugin: NLRPlugin) {
    super(app);
    this.plugin = plugin;
    this.routes = [...(this.plugin.settings.apiRoutes || [])];
  }

  onOpen(): void {
    const { contentEl } = this;
    contentEl.addClass("nlr-api-router-modal");
    contentEl.createEl("h3", { text: "API Key Routing" });
    contentEl.createEl("p", {
      text: "Map API keys to provider endpoints. Routes determine where requests are forwarded.",
      cls: "nlr-stats-muted",
    });

    this.renderRoutes(contentEl);
    this.renderAddRoute(contentEl);
    this.renderActions(contentEl);
  }

  private renderRoutes(contentEl: HTMLElement): void {
    const routesContainer = contentEl.createDiv({ cls: "nlr-api-routes" });

    if (this.routes.length === 0) {
      routesContainer.createEl("p", { text: "No routes configured", cls: "nlr-stats-muted" });
      return;
    }

    const table = routesContainer.createEl("table", { cls: "nlr-stats-table" });
    const thead = table.createEl("thead");
    const headerRow = thead.createEl("tr");
    headerRow.createEl("th", { text: "Key" });
    headerRow.createEl("th", { text: "Provider" });
    headerRow.createEl("th", { text: "Endpoint" });
    headerRow.createEl("th", { text: "Status" });
    headerRow.createEl("th", { text: "" });

    const tbody = table.createEl("tbody");
    for (let i = 0; i < this.routes.length; i++) {
      const route = this.routes[i];
      const row = tbody.createEl("tr");
      row.createEl("td", { text: route.keyName });
      row.createEl("td", { text: route.provider });
      row.createEl("td", { text: truncateUrl(route.endpoint) });

      const statusCell = row.createEl("td");
      const hasKey = !!this.plugin.settings.apiKeys[route.keyName];
      statusCell.createEl("span", {
        text: hasKey ? "\u2713 Key set" : "\u2717 No key",
        cls: hasKey ? "nlr-stats-success" : "nlr-stats-failure",
      });

      const actionCell = row.createEl("td");
      const testBtn = actionCell.createEl("button", {
        text: "Test",
        cls: "nlr-chatbot-btn nlr-chatbot-btn-small",
      });
      testBtn.addEventListener("click", () => this.testRoute(route));

      const removeBtn = actionCell.createEl("button", {
        text: "\u2717",
        cls: "nlr-chatbot-btn nlr-chatbot-btn-small",
      });
      removeBtn.addEventListener("click", () => {
        this.routes.splice(i, 1);
        this.refreshDisplay();
      });
    }
  }

  private renderAddRoute(contentEl: HTMLElement): void {
    contentEl.createEl("h4", { text: "Add Route" });

    const newRoute: ApiRoute = { keyName: "", provider: "", endpoint: "" };

    new Setting(contentEl)
      .setName("API Key Variable")
      .addText((text) =>
        text
          .setPlaceholder("OPENROUTER_API_KEY")
          .onChange((v) => { newRoute.keyName = v; })
      );

    new Setting(contentEl)
      .setName("Provider")
      .addDropdown((drop: DropdownComponent) => {
        for (const p of PROVIDERS) {
          drop.addOption(p.value, p.label);
        }
        drop.onChange((v) => {
          newRoute.provider = v;
          const match = PROVIDERS.find((p) => p.value === v);
          if (match && match.endpoint) {
            newRoute.endpoint = match.endpoint;
          }
        });
      });

    new Setting(contentEl)
      .setName("Endpoint")
      .addText((text) =>
        text
          .setPlaceholder("https://api.example.com/v1")
          .onChange((v) => { newRoute.endpoint = v; })
      );

    new Setting(contentEl).addButton((btn) =>
      btn
        .setButtonText("Add Route")
        .setCta()
        .onClick(() => {
          if (!newRoute.keyName || !newRoute.provider) {
            new Notice("Key name and provider are required");
            return;
          }
          if (!newRoute.endpoint) {
            const match = PROVIDERS.find((p) => p.value === newRoute.provider);
            newRoute.endpoint = match?.endpoint || "";
          }
          this.routes.push({ ...newRoute });
          this.refreshDisplay();
        })
    );
  }

  private renderActions(contentEl: HTMLElement): void {
    const actions = contentEl.createDiv({ cls: "nlr-modal-btn-row" });

    new Setting(actions).addButton((btn) =>
      btn
        .setButtonText("Save Routes")
        .setCta()
        .onClick(async () => {
          this.plugin.settings.apiRoutes = [...this.routes];
          await this.plugin.saveSettings();
          new Notice(`Saved ${this.routes.length} routes`);
        })
    );

    new Setting(actions).addButton((btn) =>
      btn
        .setButtonText("Write to Config")
        .setWarning()
        .onClick(async () => {
          await this.writeToConfig();
        })
    );

    new Setting(actions).addButton((btn) =>
      btn
        .setButtonText("Load from Config")
        .onClick(async () => {
          await this.loadFromConfig();
          this.refreshDisplay();
        })
    );
  }

  private async testRoute(route: ApiRoute): Promise<void> {
    const key = this.plugin.settings.apiKeys[route.keyName];
    if (!key) {
      new Notice(`No key set for ${route.keyName}`);
      return;
    }

    try {
      const headers: Record<string, string> = {
        Authorization: `Bearer ${key}`,
      };

      if (route.provider === "anthropic") {
        headers["x-api-key"] = key;
        headers["anthropic-version"] = "2023-06-01";
        delete headers["Authorization"];
      }

      const testUrl = route.endpoint.replace(/\/+$/, "");
      let url = testUrl;
      if (route.provider === "openrouter") url = "https://openrouter.ai/api/v1/models";
      else if (route.provider === "anthropic") url = "https://api.anthropic.com/v1/models";
      else if (route.provider === "openai") url = "https://api.openai.com/v1/models";

      const response = await fetch(url, { headers });
      if (response.ok) {
        new Notice(`${route.provider}: Connected`);
      } else {
        new Notice(`${route.provider}: HTTP ${response.status}`);
      }
    } catch (e: unknown) {
      const err = e as Error;
      new Notice(`${route.provider}: ${err.message}`);
    }
  }

  private async writeToConfig(): Promise<void> {
    const nlrRoot = this.plugin.settings.nlrRoot;
    if (!nlrRoot) {
      new Notice("NLR Root not set");
      return;
    }

    const configPath = path.join(nlrRoot, "config", "neuro-link-config.md");
    if (!fs.existsSync(configPath)) {
      new Notice("neuro-link-config.md not found");
      return;
    }

    const content = fs.readFileSync(configPath, "utf-8");
    const fmMatch = content.match(/^---\n([\s\S]*?)\n---/);
    if (!fmMatch) {
      new Notice("No frontmatter in config");
      return;
    }

    let fm = fmMatch[1];

    const routeYaml = ["api_routes:"];
    for (const route of this.routes) {
      routeYaml.push(`  - key: ${route.keyName}`);
      routeYaml.push(`    provider: ${route.provider}`);
      routeYaml.push(`    endpoint: ${route.endpoint}`);
    }
    const routeBlock = routeYaml.join("\n");

    const existingRoutes = fm.match(/api_routes:[\s\S]*?(?=\n[a-z]|\n---$|$)/);
    if (existingRoutes) {
      fm = fm.replace(existingRoutes[0], routeBlock);
    } else {
      fm += "\n" + routeBlock;
    }

    const body = content.substring(fmMatch[0].length);
    fs.writeFileSync(configPath, `---\n${fm}\n---${body}`, "utf-8");

    this.plugin.settings.apiRoutes = [...this.routes];
    await this.plugin.saveSettings();
    new Notice(`Wrote ${this.routes.length} routes to config`);
  }

  private async loadFromConfig(): Promise<void> {
    const nlrRoot = this.plugin.settings.nlrRoot;
    if (!nlrRoot) {
      new Notice("NLR Root not set");
      return;
    }

    const configPath = path.join(nlrRoot, "config", "neuro-link-config.md");
    if (!fs.existsSync(configPath)) {
      new Notice("neuro-link-config.md not found");
      return;
    }

    const content = fs.readFileSync(configPath, "utf-8");
    const fmMatch = content.match(/^---\n([\s\S]*?)\n---/);
    if (!fmMatch) return;

    const fm = fmMatch[1];
    const routesBlock = fm.match(/api_routes:\n([\s\S]*?)(?=\n[a-z]|\n$|$)/);
    if (!routesBlock) {
      new Notice("No api_routes found in config");
      return;
    }

    const loaded: ApiRoute[] = [];
    const entries = routesBlock[1].matchAll(
      /- key:\s*(\S+)\n\s+provider:\s*(\S+)\n\s+endpoint:\s*(\S+)/g
    );
    for (const m of entries) {
      loaded.push({ keyName: m[1], provider: m[2], endpoint: m[3] });
    }

    this.routes = loaded;
    this.plugin.settings.apiRoutes = loaded;
    await this.plugin.saveSettings();
    new Notice(`Loaded ${loaded.length} routes from config`);
  }

  private refreshDisplay(): void {
    this.contentEl.empty();
    this.onOpen();
  }

  onClose(): void {
    this.contentEl.empty();
  }
}

function truncateUrl(url: string): string {
  if (url.length <= 40) return url;
  return url.substring(0, 37) + "...";
}
