import { Modal, App, Setting, Notice, TextComponent } from "obsidian";
import type NLRPlugin from "./main";
import { HarnessSetupModal } from "./harness-setup";
import { McpSetupModal } from "./mcp-setup";
import { ApiRouterModal } from "./api-router";
import { VIEW_TYPE_CHATBOT } from "./chatbot";
import { VIEW_TYPE_STATS } from "./stats";

function showResultModal(app: App, title: string, content: string): void {
  const modal = new ResultModal(app, title, content);
  modal.open();
}

class ResultModal extends Modal {
  private title: string;
  private content: string;

  constructor(app: App, title: string, content: string) {
    super(app);
    this.title = title;
    this.content = content;
  }

  onOpen(): void {
    const { contentEl } = this;
    contentEl.createEl("h3", { text: this.title });
    const pre = contentEl.createEl("pre", { cls: "nlr-result-pre" });
    pre.createEl("code", { text: this.content });
  }

  onClose(): void {
    this.contentEl.empty();
  }
}

class SearchModal extends Modal {
  private plugin: NLRPlugin;
  private query: string = "";

  constructor(app: App, plugin: NLRPlugin) {
    super(app);
    this.plugin = plugin;
  }

  onOpen(): void {
    const { contentEl } = this;
    contentEl.createEl("h3", { text: "NLR Wiki Search" });

    new Setting(contentEl).setName("Query").addText((text: TextComponent) => {
      text.setPlaceholder("Search the wiki...").onChange((value) => {
        this.query = value;
      });
      text.inputEl.addEventListener("keydown", (e: KeyboardEvent) => {
        if (e.key === "Enter") {
          this.doSearch();
        }
      });
      setTimeout(() => text.inputEl.focus(), 50);
    });

    new Setting(contentEl).addButton((btn) =>
      btn
        .setButtonText("Search")
        .setCta()
        .onClick(() => this.doSearch())
    );

    contentEl.createEl("div", { cls: "nlr-search-results", attr: { id: "nlr-search-results" } });
  }

  private async doSearch(): Promise<void> {
    if (!this.query.trim()) return;

    const resultsEl = this.contentEl.querySelector("#nlr-search-results");
    if (!resultsEl) return;
    resultsEl.empty();
    resultsEl.createEl("p", { text: "Searching..." });

    try {
      const result = await this.plugin.runNlrCommand(["search", this.query]);
      resultsEl.empty();
      const pre = resultsEl.createEl("pre", { cls: "nlr-result-pre" });
      pre.createEl("code", { text: result || "No results found" });
    } catch (e: unknown) {
      const err = e as Error;
      resultsEl.empty();
      resultsEl.createEl("p", { text: `Error: ${err.message}`, cls: "nlr-error" });
    }
  }

  onClose(): void {
    this.contentEl.empty();
  }
}

class CreateTaskModal extends Modal {
  private plugin: NLRPlugin;
  private taskType: string = "curate";
  private taskPriority: string = "3";
  private taskDescription: string = "";

  constructor(app: App, plugin: NLRPlugin) {
    super(app);
    this.plugin = plugin;
  }

  onOpen(): void {
    const { contentEl } = this;
    contentEl.createEl("h3", { text: "Create NLR Task" });

    new Setting(contentEl)
      .setName("Type")
      .addDropdown((drop) =>
        drop
          .addOption("ingest", "Ingest")
          .addOption("curate", "Curate")
          .addOption("scan", "Scan")
          .addOption("repair", "Repair")
          .addOption("report", "Report")
          .addOption("ontology", "Ontology")
          .setValue(this.taskType)
          .onChange((v) => { this.taskType = v; })
      );

    new Setting(contentEl)
      .setName("Priority")
      .setDesc("1 (highest) to 5 (lowest)")
      .addDropdown((drop) =>
        drop
          .addOption("1", "1 - Critical")
          .addOption("2", "2 - High")
          .addOption("3", "3 - Normal")
          .addOption("4", "4 - Low")
          .addOption("5", "5 - Background")
          .setValue(this.taskPriority)
          .onChange((v) => { this.taskPriority = v; })
      );

    new Setting(contentEl)
      .setName("Description")
      .addTextArea((text) =>
        text
          .setPlaceholder("Describe the task...")
          .onChange((v) => { this.taskDescription = v; })
      );

    new Setting(contentEl).addButton((btn) =>
      btn
        .setButtonText("Create")
        .setCta()
        .onClick(async () => {
          await this.createTask();
        })
    );
  }

  private async createTask(): Promise<void> {
    if (!this.taskDescription.trim()) {
      new Notice("Task description required");
      return;
    }
    try {
      const result = await this.plugin.runNlrCommand([
        "tasks",
        "create",
        "--type",
        this.taskType,
        "--priority",
        this.taskPriority,
        "--desc",
        this.taskDescription,
      ]);
      new Notice("Task created");
      showResultModal(this.app, "Task Created", result);
      this.close();
    } catch (e: unknown) {
      const err = e as Error;
      new Notice(`Failed: ${err.message}`);
    }
  }

  onClose(): void {
    this.contentEl.empty();
  }
}

export function registerCommands(plugin: NLRPlugin): void {
  plugin.addCommand({
    id: "nlr-check-status",
    name: "Neuro-Link: Check Status",
    callback: async () => {
      try {
        const result = await plugin.runNlrCommand(["status"]);
        showResultModal(plugin.app, "NLR Status", result);
      } catch (e: unknown) {
        const err = e as Error;
        new Notice(`NLR status failed: ${err.message}`);
      }
    },
  });

  plugin.addCommand({
    id: "nlr-run-brain-scan",
    name: "Neuro-Link: Run Brain Scan",
    callback: async () => {
      new Notice("Running brain scan...");
      try {
        const result = await plugin.runNlrCommand(["scan"]);
        showResultModal(plugin.app, "Brain Scan Results", result);
      } catch (e: unknown) {
        const err = e as Error;
        new Notice(`Brain scan failed: ${err.message}`);
      }
    },
  });

  plugin.addCommand({
    id: "nlr-ingest-current-note",
    name: "Neuro-Link: Ingest Current Note",
    callback: async () => {
      const activeFile = plugin.app.workspace.getActiveFile();
      if (!activeFile) {
        new Notice("No active file");
        return;
      }
      const filePath = activeFile.path;
      new Notice(`Ingesting ${filePath}...`);
      try {
        const vaultPath = plugin.settings.vaultPath;
        const fullPath = vaultPath ? `${vaultPath}/${filePath}` : filePath;
        const result = await plugin.runNlrCommand(["ingest", fullPath]);
        new Notice("Ingestion complete");
        showResultModal(plugin.app, "Ingest Result", result);
      } catch (e: unknown) {
        const err = e as Error;
        new Notice(`Ingest failed: ${err.message}`);
      }
    },
  });

  plugin.addCommand({
    id: "nlr-search-wiki",
    name: "Neuro-Link: Search Wiki",
    callback: () => {
      new SearchModal(plugin.app, plugin).open();
    },
  });

  plugin.addCommand({
    id: "nlr-list-tasks",
    name: "Neuro-Link: List Tasks",
    callback: async () => {
      try {
        const result = await plugin.runNlrCommand(["tasks"]);
        showResultModal(plugin.app, "NLR Tasks", result);
      } catch (e: unknown) {
        const err = e as Error;
        new Notice(`List tasks failed: ${err.message}`);
      }
    },
  });

  plugin.addCommand({
    id: "nlr-create-task",
    name: "Neuro-Link: Create Task",
    callback: () => {
      new CreateTaskModal(plugin.app, plugin).open();
    },
  });

  plugin.addCommand({
    id: "nlr-run-heartbeat",
    name: "Neuro-Link: Run Heartbeat",
    callback: async () => {
      try {
        const result = await plugin.runNlrCommand(["heartbeat"]);
        new Notice("Heartbeat sent");
        showResultModal(plugin.app, "Heartbeat", result);
      } catch (e: unknown) {
        const err = e as Error;
        new Notice(`Heartbeat failed: ${err.message}`);
      }
    },
  });

  plugin.addCommand({
    id: "nlr-start-server-tunnel",
    name: "Neuro-Link: Start Server with Tunnel",
    callback: async () => {
      new Notice("Starting server with tunnel...");
      try {
        const result = await plugin.runNlrCommand(["serve", "--tunnel", "--token", "auto"]);
        showResultModal(plugin.app, "Server + Tunnel", result);
      } catch (e: unknown) {
        const err = e as Error;
        new Notice(`Server start failed: ${err.message}`);
      }
    },
  });

  plugin.addCommand({
    id: "nlr-rebuild-rag-index",
    name: "Neuro-Link: Rebuild RAG Index",
    callback: async () => {
      new Notice("Rebuilding RAG index...");
      try {
        const result = await plugin.runNlrCommand(["rag-rebuild"]);
        new Notice("RAG index rebuilt");
        showResultModal(plugin.app, "RAG Rebuild", result);
      } catch (e: unknown) {
        const err = e as Error;
        new Notice(`RAG rebuild failed: ${err.message}`);
      }
    },
  });

  plugin.addCommand({
    id: "nlr-grade-session",
    name: "Neuro-Link: Grade Session",
    callback: async () => {
      new Notice("Grading session...");
      try {
        const result = await plugin.runNlrCommand(["grade", "--session"]);
        showResultModal(plugin.app, "Session Grade", result);
      } catch (e: unknown) {
        const err = e as Error;
        new Notice(`Grading failed: ${err.message}`);
      }
    },
  });

  plugin.addCommand({
    id: "nlr-open-harness-setup",
    name: "Neuro-Link: Open Harness Setup",
    callback: () => {
      new HarnessSetupModal(plugin.app, plugin).open();
    },
  });

  plugin.addCommand({
    id: "nlr-open-mcp-setup",
    name: "Neuro-Link: Open MCP Setup",
    callback: () => {
      new McpSetupModal(plugin.app, plugin).open();
    },
  });

  plugin.addCommand({
    id: "nlr-open-api-router",
    name: "Neuro-Link: Open API Router",
    callback: () => {
      new ApiRouterModal(plugin.app, plugin).open();
    },
  });

  plugin.addCommand({
    id: "nlr-open-chatbot",
    name: "Neuro-Link: Open Chatbot",
    callback: () => {
      plugin.activateView(VIEW_TYPE_CHATBOT);
    },
  });

  plugin.addCommand({
    id: "nlr-open-stats",
    name: "Neuro-Link: Open Stats",
    callback: () => {
      plugin.activateView(VIEW_TYPE_STATS);
    },
  });

  plugin.addCommand({
    id: "nlr-sessions-parse",
    name: "Neuro-Link: Parse Claude Code Sessions",
    callback: async () => {
      new Notice("Parsing Claude Code sessions...");
      try {
        const result = await plugin.runNlrCommand(["sessions", "parse"]);
        showResultModal(plugin.app, "Session Parse", result);
      } catch (e: unknown) {
        const err = e as Error;
        new Notice(`Parse failed: ${err.message}`);
      }
    },
  });

  plugin.addCommand({
    id: "nlr-sessions-scan",
    name: "Neuro-Link: Scan Session Quality",
    callback: async () => {
      new Notice("Scanning session quality...");
      try {
        const result = await plugin.runNlrCommand(["sessions", "scan", "--days", "7"]);
        showResultModal(plugin.app, "Session Quality Scan", result);
      } catch (e: unknown) {
        const err = e as Error;
        new Notice(`Scan failed: ${err.message}`);
      }
    },
  });
}
