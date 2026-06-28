// SPDX-License-Identifier: AGPL-3.0-only
//
// Portions adapted from logancyang/obsidian-copilot
//   Copyright (c) 2023 Logan Yang
// See THIRD_PARTY/obsidian-copilot/LICENSE for the full license text.
//
// Conceptual source:
//   src/components/Chat/ChatSingleMessage.tsx (React component)
//
// Adaptation notes:
//   - Upstream is React + Tailwind with ToolCall/AgentReasoning/Citation
//     sub-components we don't need. This file keeps only the core
//     markdown-render path: role-label, timestamp, markdown body rendered
//     via Obsidian's `MarkdownRenderer.render()`.
//   - Tool-call chips are our own, minimal, collapsible `<details>` blocks.
//     Not adapted from upstream.
//   - Incremental append uses a `Map<msgId, HTMLElement>` so streaming
//     rerenders only touch the last bubble. Upstream re-mounts the whole
//     list on every chunk — wasteful at fast stream rates.
//   - Copy-to-clipboard is a single button, no menu. (Upstream's full
//     copyMessage menu wasn't worth the bundle cost.)

import { MarkdownRenderer, Component } from "obsidian";
import type { App } from "obsidian";

export type MessageRole = "user" | "assistant" | "system" | "tool";

export interface ChatMessage {
  id: string;
  role: MessageRole;
  content: string;
  timestamp: number;
  /** Tool calls the assistant made in this turn. Rendered as chips. */
  toolCalls?: Array<{
    id: string;
    name: string;
    arguments: string;
    result?: string;
    outcome?: "ok" | "refused" | "error";
  }>;
  /** If set, mode badge (e.g. "agent") shown on the message. */
  modeBadge?: string;
}

export interface MessageListOptions {
  /** App reference for MarkdownRenderer.render(). */
  app: App;
  /** Parent Component lifecycle owner so we can propagate unload. */
  parent: Component;
  /** When true (default), auto-scroll on append. */
  autoScroll?: boolean;
}

export class MessageList {
  private app: App;
  private parent: Component;
  private container: HTMLElement;
  private messages = new Map<string, HTMLElement>();
  private order: string[] = [];
  private autoScrollEnabled: boolean;

  constructor(parent: HTMLElement, opts: MessageListOptions) {
    this.app = opts.app;
    this.parent = opts.parent;
    this.autoScrollEnabled = opts.autoScroll ?? true;
    this.container = parent.createDiv({ cls: "nlr-chat-messages" });
  }

  /** Clear all messages. */
  clear(): void {
    this.messages.clear();
    this.order = [];
    this.container.empty();
  }

  setAutoScroll(enabled: boolean): void {
    this.autoScrollEnabled = enabled;
  }

  /** Append a brand-new message bubble. Idempotent on duplicate id. */
  append(msg: ChatMessage): HTMLElement {
    if (this.messages.has(msg.id)) {
      return this.messages.get(msg.id)!;
    }
    const bubble = this.container.createDiv({
      cls: `nlr-chat-message nlr-chat-message-${msg.role}`,
    });
    bubble.dataset.messageId = msg.id;

    const header = bubble.createDiv({ cls: "nlr-chat-message-header" });
    header.createSpan({
      text: roleLabel(msg.role),
      cls: "nlr-chat-role-label",
    });
    if (msg.modeBadge) {
      header.createSpan({
        text: msg.modeBadge,
        cls: "nlr-chat-mode-badge",
      });
    }
    header.createSpan({
      text: new Date(msg.timestamp).toLocaleTimeString(),
      cls: "nlr-chat-timestamp",
    });

    const body = bubble.createDiv({ cls: "nlr-chat-message-body" });
    this.renderBody(body, msg.content, msg.role);

    if (msg.toolCalls && msg.toolCalls.length > 0) {
      this.renderToolCalls(bubble, msg.toolCalls);
    }

    // Copy button — single action, not a menu.
    const actions = bubble.createDiv({ cls: "nlr-chat-message-actions" });
    const copyBtn = actions.createEl("button", {
      text: "Copy",
      cls: "nlr-chat-action-btn",
    });
    copyBtn.addEventListener("click", () => {
      void navigator.clipboard.writeText(msg.content);
    });

    this.messages.set(msg.id, bubble);
    this.order.push(msg.id);
    if (this.autoScrollEnabled) this.scrollToBottom();
    return bubble;
  }

  /**
   * Replace the content of an existing message — used for streaming updates.
   * Only re-renders the `body` sub-element, not the whole bubble.
   */
  update(id: string, content: string, toolCalls?: ChatMessage["toolCalls"]): void {
    const bubble = this.messages.get(id);
    if (!bubble) return;
    const body = bubble.querySelector(".nlr-chat-message-body") as HTMLElement | null;
    if (body) {
      body.empty();
      const role = (bubble.classList.contains("nlr-chat-message-user")
        ? "user"
        : bubble.classList.contains("nlr-chat-message-tool")
        ? "tool"
        : "assistant") as MessageRole;
      this.renderBody(body, content, role);
    }
    // Remove previous tool-call block (if any) and re-render fresh.
    const existingCalls = bubble.querySelector(".nlr-chat-tool-calls");
    if (existingCalls) existingCalls.remove();
    if (toolCalls && toolCalls.length > 0) {
      this.renderToolCalls(bubble, toolCalls, bubble.querySelector(".nlr-chat-message-actions") as HTMLElement | null);
    }
    if (this.autoScrollEnabled) this.scrollToBottom();
  }

  /** Remove a message (e.g. to replace a placeholder with real content). */
  remove(id: string): void {
    const bubble = this.messages.get(id);
    if (!bubble) return;
    bubble.remove();
    this.messages.delete(id);
    this.order = this.order.filter((x) => x !== id);
  }

  private renderBody(target: HTMLElement, content: string, role: MessageRole): void {
    // For `user` and `tool` messages, render as pre-wrapped text (no
    // markdown evaluation on untrusted-ish content). For `assistant` and
    // `system`, use MarkdownRenderer.
    if (role === "user" || role === "tool") {
      target.createEl("pre", { text: content, cls: "nlr-chat-plain" });
      return;
    }
    // The rendered markdown may include task-list checkboxes, internal
    // links, embeds, etc.  Obsidian's renderer handles all of that.
    // `sourcePath` is empty-ish — we're not associated with a file.
    void MarkdownRenderer.render(this.app, content, target, "", this.parent);
  }

  private renderToolCalls(
    bubble: HTMLElement,
    calls: NonNullable<ChatMessage["toolCalls"]>,
    before: HTMLElement | null = null
  ): void {
    const wrap = bubble.createDiv({ cls: "nlr-chat-tool-calls" });
    if (before) bubble.insertBefore(wrap, before);
    for (const call of calls) {
      const details = wrap.createEl("details", { cls: "nlr-chat-tool-call" });
      details.addClass(`nlr-chat-tool-call-${call.outcome ?? "ok"}`);
      const summary = details.createEl("summary");
      summary.createSpan({
        text: call.name,
        cls: "nlr-chat-tool-call-name",
      });
      summary.createSpan({
        text: call.outcome ?? "pending",
        cls: "nlr-chat-tool-call-outcome",
      });

      const argsEl = details.createEl("pre", { cls: "nlr-chat-tool-call-args" });
      argsEl.createEl("code", { text: call.arguments });

      if (call.result) {
        const resultEl = details.createEl("pre", {
          cls: "nlr-chat-tool-call-result",
        });
        resultEl.createEl("code", { text: call.result });
      }
    }
  }

  private scrollToBottom(): void {
    this.container.scrollTop = this.container.scrollHeight;
  }
}

function roleLabel(role: MessageRole): string {
  switch (role) {
    case "user":
      return "You";
    case "assistant":
      return "Assistant";
    case "tool":
      return "Tool";
    case "system":
      return "System";
  }
}
