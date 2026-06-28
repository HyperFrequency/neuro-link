import {
  ItemView,
  WorkspaceLeaf,
  Notice,
  MarkdownView,
  setIcon,
} from "obsidian";
import type NLRPlugin from "./main";

export const VIEW_TYPE_CHATBOT = "nlr-chatbot-view";

interface ChatMessage {
  role: "user" | "assistant" | "system";
  content: string;
  contextPages?: string[];
  timestamp: number;
}

export class ChatbotView extends ItemView {
  plugin: NLRPlugin;
  private messages: ChatMessage[] = [];
  private messagesEl!: HTMLElement;
  private inputEl!: HTMLTextAreaElement;
  private isStreaming: boolean = false;

  constructor(leaf: WorkspaceLeaf, plugin: NLRPlugin) {
    super(leaf);
    this.plugin = plugin;
  }

  getViewType(): string {
    return VIEW_TYPE_CHATBOT;
  }

  getDisplayText(): string {
    return "NLR Chatbot";
  }

  getIcon(): string {
    return "nlr-brain";
  }

  async onOpen(): Promise<void> {
    const container = this.containerEl.children[1] as HTMLElement;
    container.empty();
    container.addClass("nlr-chatbot-container");

    const header = container.createDiv({ cls: "nlr-chatbot-header" });
    header.createEl("h4", { text: "NLR Chatbot" });

    const headerActions = header.createDiv({ cls: "nlr-chatbot-header-actions" });

    const clearBtn = headerActions.createEl("button", {
      cls: "nlr-chatbot-btn nlr-chatbot-btn-small",
      attr: { "aria-label": "Clear chat" },
    });
    setIcon(clearBtn, "trash-2");
    clearBtn.addEventListener("click", () => {
      this.messages = [];
      this.renderMessages();
    });

    const modelInfo = header.createDiv({ cls: "nlr-chatbot-model-info" });
    modelInfo.createEl("span", {
      text: this.plugin.settings.chatbotModel.split("/").pop() || "unknown",
      cls: "nlr-chatbot-model-badge",
    });

    this.messagesEl = container.createDiv({ cls: "nlr-chatbot-messages" });

    const inputArea = container.createDiv({ cls: "nlr-chatbot-input-area" });

    this.inputEl = inputArea.createEl("textarea", {
      cls: "nlr-chatbot-input",
      attr: { placeholder: "Ask about your knowledge base...", rows: "3" },
    });

    this.inputEl.addEventListener("keydown", (e: KeyboardEvent) => {
      if (e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        this.sendMessage();
      }
    });

    const btnRow = inputArea.createDiv({ cls: "nlr-chatbot-btn-row" });

    const sendBtn = btnRow.createEl("button", {
      text: "Send",
      cls: "nlr-chatbot-btn nlr-chatbot-btn-primary",
    });
    sendBtn.addEventListener("click", () => this.sendMessage());

    const wikiBtn = btnRow.createEl("button", {
      text: "Save to Wiki",
      cls: "nlr-chatbot-btn",
    });
    wikiBtn.addEventListener("click", () => this.saveToWiki());

    const kdenseBtn = btnRow.createEl("button", {
      text: "Send to K-Dense",
      cls: "nlr-chatbot-btn",
    });
    kdenseBtn.addEventListener("click", () => this.dispatchToHarness("k-dense-byok"));

    const forgeBtn = btnRow.createEl("button", {
      text: "Send to ForgeCode",
      cls: "nlr-chatbot-btn",
    });
    forgeBtn.addEventListener("click", () => this.dispatchToHarness("forgecode"));

    this.renderMessages();
  }

  async onClose(): Promise<void> {
    // nothing to clean up
  }

  private renderMessages(): void {
    this.messagesEl.empty();

    if (this.messages.length === 0) {
      this.messagesEl.createDiv({ cls: "nlr-chatbot-empty" }).createEl("p", {
        text: "Ask questions about your neuro-link knowledge base. Wiki context is automatically injected via RAG.",
      });
      return;
    }

    for (const msg of this.messages) {
      const msgEl = this.messagesEl.createDiv({
        cls: `nlr-chatbot-message nlr-chatbot-message-${msg.role}`,
      });

      const roleEl = msgEl.createDiv({ cls: "nlr-chatbot-message-role" });
      roleEl.createEl("span", {
        text: msg.role === "user" ? "You" : "Assistant",
        cls: "nlr-chatbot-role-label",
      });
      roleEl.createEl("span", {
        text: new Date(msg.timestamp).toLocaleTimeString(),
        cls: "nlr-chatbot-timestamp",
      });

      msgEl.createDiv({ cls: "nlr-chatbot-message-content", text: msg.content });

      if (msg.contextPages && msg.contextPages.length > 0) {
        const ctxEl = msgEl.createDiv({ cls: "nlr-chatbot-context" });
        ctxEl.createEl("span", { text: "Context: ", cls: "nlr-chatbot-context-label" });
        for (const page of msg.contextPages) {
          const link = ctxEl.createEl("a", {
            text: page,
            cls: "nlr-chatbot-context-link",
            href: "#",
          });
          link.addEventListener("click", (e) => {
            e.preventDefault();
            this.app.workspace.openLinkText(page, "", false);
          });
        }
      }
    }

    this.messagesEl.scrollTop = this.messagesEl.scrollHeight;
  }

  private async sendMessage(): Promise<void> {
    const content = this.inputEl.value.trim();
    if (!content || this.isStreaming) return;

    this.inputEl.value = "";
    this.isStreaming = true;

    this.messages.push({
      role: "user",
      content,
      timestamp: Date.now(),
    });
    this.renderMessages();

    let contextPages: string[] = [];
    let ragContext = "";

    try {
      const ragResult = await this.plugin.runNlrCommand(["rag-query", content]);
      if (ragResult) {
        ragContext = ragResult;
        const pageMatches = ragResult.matchAll(/\[\[([^\]]+)\]\]/g);
        for (const m of pageMatches) {
          contextPages.push(m[1]);
        }
        if (contextPages.length === 0) {
          const fileMatches = ragResult.matchAll(/(?:^|\n)(?:source|file|page):\s*(.+)/gi);
          for (const m of fileMatches) {
            contextPages.push(m[1].trim());
          }
        }
      }
    } catch {
      // RAG unavailable, proceed without context
    }

    const apiKey = this.plugin.settings.apiKeys["OPENROUTER_API_KEY"];
    if (!apiKey) {
      this.messages.push({
        role: "assistant",
        content: "OpenRouter API key not configured. Set it in Settings > Neuro-Link Recursive > API Keys.",
        timestamp: Date.now(),
      });
      this.isStreaming = false;
      this.renderMessages();
      return;
    }

    const systemMessage = this.plugin.settings.chatbotSystemPrompt;
    const contextBlock = ragContext
      ? `\n\n--- Wiki Context ---\n${ragContext}\n--- End Context ---`
      : "";

    const apiMessages = [
      { role: "system" as const, content: systemMessage + contextBlock },
      ...this.messages
        .filter((m) => m.role !== "system")
        .map((m) => ({ role: m.role, content: m.content })),
    ];

    try {
      const response = await fetch("https://openrouter.ai/api/v1/chat/completions", {
        method: "POST",
        headers: {
          Authorization: `Bearer ${apiKey}`,
          "Content-Type": "application/json",
          "HTTP-Referer": "https://github.com/HyperFrequency",
          "X-Title": "NLR Obsidian Plugin",
        },
        body: JSON.stringify({
          model: this.plugin.settings.chatbotModel,
          messages: apiMessages,
          max_tokens: 4096,
        }),
      });

      if (!response.ok) {
        const errText = await response.text();
        if (response.status === 401 || errText.includes("not found")) {
          throw new Error(`OpenRouter auth failed (${response.status}). Check your API key at openrouter.ai/settings/keys — current key starts with: ${apiKey.substring(0, 8)}...`);
        }
        throw new Error(`OpenRouter ${response.status}: ${errText}`);
      }

      const data = (await response.json()) as {
        choices: Array<{ message: { content: string } }>;
      };
      const assistantContent = data.choices?.[0]?.message?.content || "No response received";

      this.messages.push({
        role: "assistant",
        content: assistantContent,
        contextPages,
        timestamp: Date.now(),
      });
    } catch (e: unknown) {
      const err = e as Error;
      this.messages.push({
        role: "assistant",
        content: `Error: ${err.message}`,
        timestamp: Date.now(),
      });
    }

    this.isStreaming = false;
    this.renderMessages();
  }

  private async saveToWiki(): Promise<void> {
    if (this.messages.length === 0) {
      new Notice("No messages to save");
      return;
    }

    const lastAssistant = [...this.messages].reverse().find((m) => m.role === "assistant");
    if (!lastAssistant) {
      new Notice("No assistant response to save");
      return;
    }

    const lastUser = [...this.messages].reverse().find((m) => m.role === "user");
    const title = lastUser
      ? lastUser.content.substring(0, 60).replace(/[^a-zA-Z0-9\s-]/g, "").trim()
      : "chatbot-note";
    const slug = title.replace(/\s+/g, "-").toLowerCase();
    const now = new Date().toISOString().split("T")[0];

    const frontmatter = [
      "---",
      `title: "${title}"`,
      'domain: chatbot',
      `sources: [chatbot-${now}]`,
      "confidence: 0.6",
      `last_updated: "${now}"`,
      "open_questions: []",
      "---",
    ].join("\n");

    const content = `${frontmatter}\n\n# ${title}\n\n${lastAssistant.content}\n\n## Sources\n\n- Generated by NLR Chatbot on ${now}\n`;

    try {
      const file = await this.app.vault.create(`02-KB-main/${slug}.md`, content);
      new Notice(`Wiki page created: ${file.path}`);
      this.app.workspace.openLinkText(file.path, "", false);
    } catch (e: unknown) {
      const err = e as Error;
      if (err.message.includes("already exists")) {
        new Notice("A wiki page with this name already exists");
      } else {
        new Notice(`Failed to create wiki page: ${err.message}`);
      }
    }
  }

  private async dispatchToHarness(harnessName: string): Promise<void> {
    if (this.messages.length === 0) {
      new Notice("No messages to dispatch");
      return;
    }

    const lastUser = [...this.messages].reverse().find((m) => m.role === "user");
    if (!lastUser) {
      new Notice("No user message to dispatch");
      return;
    }

    try {
      const result = await this.plugin.runNlrCommand([
        "harness-dispatch",
        "--to",
        harnessName,
        "--task",
        lastUser.content,
      ]);
      new Notice(`Dispatched to ${harnessName}`);

      this.messages.push({
        role: "system",
        content: `Dispatched to ${harnessName}: ${result}`,
        timestamp: Date.now(),
      });
      this.renderMessages();
    } catch (e: unknown) {
      const err = e as Error;
      new Notice(`Dispatch failed: ${err.message}`);
    }
  }
}
