// SPDX-License-Identifier: MIT
//
// Clean-room. The `@`-mention typeahead pattern is conceptually inspired by
// obsidian-copilot's AtMentionTypeahead.tsx, but no code was copied — this
// is a plain-textarea + absolute-positioned overlay implementation, ~200
// lines, with no Lexical dependency.
//
// Design:
//   - Plain `<textarea>` for input (same UX as chatbot.ts so users have
//     muscle memory).
//   - On each keystroke we scan backwards from the caret for an `@token`.
//     If we find one, an overlay appears below the textarea with filtered
//     suggestions. Arrow keys move through them; Enter selects.
//   - Three suggestion sources, switched via tab keys: files (markdown
//     files from the vault), skills (loaded at construct time), agents
//     (currently just `@neuro`).
//   - Submit on Enter (no shift). Shift+Enter = newline.

import type { App } from "obsidian";

export interface ComposerSuggestion {
  /** Category label shown in the overlay. */
  kind: "file" | "skill" | "agent";
  /** Raw text inserted into the textarea (without the leading `@`). */
  value: string;
  /** Display label in the overlay. Usually === `value`. */
  label: string;
  /** Optional secondary description. */
  description?: string;
}

export interface ComposerOptions {
  app: App;
  /** Callable providing skill suggestions. Called on each overlay open. */
  skills: () => ComposerSuggestion[];
  /** Agents list (static). */
  agents: () => ComposerSuggestion[];
  /** Submit handler — called with the textarea content on Enter. */
  onSubmit: (content: string) => void;
  /** Optional abort handler — "Stop" button. Visible only when streaming. */
  onStop?: () => void;
  placeholder?: string;
}

export class Composer {
  private app: App;
  private opts: ComposerOptions;
  private root: HTMLElement;
  private textarea: HTMLTextAreaElement;
  private overlay: HTMLElement;
  private sendBtn!: HTMLButtonElement;
  private stopBtn!: HTMLButtonElement;
  private overlayItems: HTMLElement[] = [];
  private overlayIndex = 0;
  private activeSuggestions: ComposerSuggestion[] = [];
  private suppressNextClose = false;
  private streaming = false;

  constructor(parent: HTMLElement, opts: ComposerOptions) {
    this.app = opts.app;
    this.opts = opts;
    this.root = parent.createDiv({ cls: "nlr-chat-composer" });
    this.textarea = this.root.createEl("textarea", {
      cls: "nlr-chat-composer-input",
      attr: {
        rows: "3",
        placeholder: opts.placeholder ?? "Type a message — prefix with @neuro to use agent mode.",
        "aria-label": "Chat message",
      },
    });
    this.overlay = this.root.createDiv({ cls: "nlr-chat-composer-overlay" });
    this.overlay.style.display = "none";

    const btnRow = this.root.createDiv({ cls: "nlr-chat-composer-buttons" });
    this.sendBtn = btnRow.createEl("button", {
      text: "Send",
      cls: "nlr-chat-action-btn nlr-chat-action-btn-primary",
    });
    this.stopBtn = btnRow.createEl("button", {
      text: "Stop",
      cls: "nlr-chat-action-btn nlr-chat-action-btn-danger",
    });
    this.stopBtn.style.display = "none";

    this.wireEvents();
  }

  /** Toggle stream-mode (shows Stop instead of Send). */
  setStreaming(streaming: boolean): void {
    this.streaming = streaming;
    if (streaming) {
      this.sendBtn.style.display = "none";
      this.stopBtn.style.display = "";
    } else {
      this.sendBtn.style.display = "";
      this.stopBtn.style.display = "none";
    }
  }

  /** Programmatic value setter (e.g. restoring draft). */
  setValue(v: string): void {
    this.textarea.value = v;
  }

  /** Focus the input. */
  focus(): void {
    this.textarea.focus();
  }

  /** Current input value. */
  get value(): string {
    return this.textarea.value;
  }

  /** Release DOM. */
  destroy(): void {
    this.hideOverlay();
    this.root.remove();
  }

  private wireEvents(): void {
    this.textarea.addEventListener("input", () => this.refreshOverlay());
    this.textarea.addEventListener("keydown", (e: KeyboardEvent) => this.onKeydown(e));
    this.textarea.addEventListener("blur", () => {
      // Delay close so clicks on the overlay still fire.
      setTimeout(() => {
        if (this.suppressNextClose) {
          this.suppressNextClose = false;
          return;
        }
        this.hideOverlay();
      }, 150);
    });
    this.sendBtn.addEventListener("click", () => this.submit());
    this.stopBtn.addEventListener("click", () => {
      this.opts.onStop?.();
    });
  }

  private onKeydown(e: KeyboardEvent): void {
    // Overlay navigation takes priority when visible.
    if (this.overlay.style.display !== "none" && this.overlayItems.length > 0) {
      if (e.key === "ArrowDown") {
        e.preventDefault();
        this.moveOverlay(1);
        return;
      }
      if (e.key === "ArrowUp") {
        e.preventDefault();
        this.moveOverlay(-1);
        return;
      }
      if (e.key === "Enter") {
        e.preventDefault();
        this.pickOverlay(this.overlayIndex);
        return;
      }
      if (e.key === "Escape") {
        e.preventDefault();
        this.hideOverlay();
        return;
      }
      if (e.key === "Tab") {
        e.preventDefault();
        this.cycleOverlayKind(e.shiftKey ? -1 : 1);
        return;
      }
    }

    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      this.submit();
    }
  }

  private submit(): void {
    if (this.streaming) return;
    const val = this.textarea.value.trim();
    if (!val) return;
    this.textarea.value = "";
    this.hideOverlay();
    this.opts.onSubmit(val);
  }

  // ── typeahead ──────────────────────────────────────────────────────────

  private refreshOverlay(): void {
    const tok = this.currentAtToken();
    if (!tok) {
      this.hideOverlay();
      return;
    }
    const suggestions = this.collectSuggestions(tok.query, tok.kind);
    if (suggestions.length === 0) {
      this.hideOverlay();
      return;
    }
    this.activeSuggestions = suggestions;
    this.renderOverlay(tok.kind);
  }

  /**
   * Returns the `@` token currently under the caret (text between the last
   * `@` and the caret, with no whitespace), plus the deduced suggestion
   * kind. Null if no `@` precedes the caret.
   *
   * Exported as a static helper via `matchAtToken` for pure testing.
   */
  private currentAtToken(): { query: string; kind: ComposerSuggestion["kind"] } | null {
    return matchAtToken(this.textarea.value, this.textarea.selectionStart ?? 0);
  }

  private collectSuggestions(
    query: string,
    kind: ComposerSuggestion["kind"]
  ): ComposerSuggestion[] {
    const q = query.toLowerCase();
    let pool: ComposerSuggestion[];
    switch (kind) {
      case "file":
        pool = this.app.vault
          .getMarkdownFiles()
          .map<ComposerSuggestion>((f) => ({
            kind: "file",
            value: f.path,
            label: f.basename,
            description: f.path,
          }));
        break;
      case "skill":
        pool = this.opts.skills();
        break;
      case "agent":
        pool = this.opts.agents();
        break;
    }
    return pool
      .filter((s) => s.label.toLowerCase().includes(q) || s.value.toLowerCase().includes(q))
      .slice(0, 12);
  }

  private renderOverlay(kind: ComposerSuggestion["kind"]): void {
    this.overlay.empty();
    this.overlay.style.display = "";
    const tabs = this.overlay.createDiv({ cls: "nlr-chat-typeahead-tabs" });
    for (const k of ["file", "skill", "agent"] as const) {
      const tab = tabs.createSpan({
        text: k,
        cls: "nlr-chat-typeahead-tab" + (k === kind ? " is-active" : ""),
      });
      tab.addEventListener("mousedown", (e) => {
        // mousedown because blur fires before click; suppress the blur-hide.
        e.preventDefault();
        this.suppressNextClose = true;
        this.overrideKind(k);
      });
    }
    this.overlayItems = [];
    for (let i = 0; i < this.activeSuggestions.length; i++) {
      const s = this.activeSuggestions[i];
      const item = this.overlay.createDiv({
        cls: "nlr-chat-typeahead-item" + (i === this.overlayIndex ? " is-active" : ""),
      });
      item.createSpan({ text: s.label, cls: "nlr-chat-typeahead-label" });
      if (s.description && s.description !== s.label) {
        item.createSpan({
          text: s.description,
          cls: "nlr-chat-typeahead-desc",
        });
      }
      item.addEventListener("mousedown", (e) => {
        e.preventDefault();
        this.suppressNextClose = true;
        this.pickOverlay(i);
      });
      this.overlayItems.push(item);
    }
    if (this.overlayIndex >= this.overlayItems.length) this.overlayIndex = 0;
  }

  private moveOverlay(delta: number): void {
    if (this.overlayItems.length === 0) return;
    this.overlayIndex =
      (this.overlayIndex + delta + this.overlayItems.length) % this.overlayItems.length;
    for (let i = 0; i < this.overlayItems.length; i++) {
      this.overlayItems[i].classList.toggle("is-active", i === this.overlayIndex);
    }
  }

  private pickOverlay(i: number): void {
    const s = this.activeSuggestions[i];
    if (!s) return;
    const caret = this.textarea.selectionStart ?? this.textarea.value.length;
    const val = this.textarea.value;
    const before = val.slice(0, caret);
    const after = val.slice(caret);
    const atIdx = before.lastIndexOf("@");
    if (atIdx < 0) return;
    const newBefore = before.slice(0, atIdx + 1) + s.value + " ";
    this.textarea.value = newBefore + after;
    const nextCaret = newBefore.length;
    this.textarea.selectionStart = nextCaret;
    this.textarea.selectionEnd = nextCaret;
    this.hideOverlay();
    this.textarea.focus();
  }

  private cycleOverlayKind(direction: number): void {
    const kinds: ComposerSuggestion["kind"][] = ["file", "skill", "agent"];
    const tok = this.currentAtToken();
    if (!tok) return;
    const idx = kinds.indexOf(tok.kind);
    const next = kinds[(idx + direction + kinds.length) % kinds.length];
    this.overrideKind(next);
  }

  private overrideKind(kind: ComposerSuggestion["kind"]): void {
    const tok = this.currentAtToken();
    if (!tok) return;
    this.activeSuggestions = this.collectSuggestions(tok.query, kind);
    if (this.activeSuggestions.length === 0) {
      this.hideOverlay();
      return;
    }
    this.overlayIndex = 0;
    this.renderOverlay(kind);
  }

  private hideOverlay(): void {
    this.overlay.style.display = "none";
    this.overlay.empty();
    this.overlayItems = [];
    this.activeSuggestions = [];
    this.overlayIndex = 0;
  }
}

/**
 * Pure helper — extracts the `@<query>` token ending at the caret, and
 * classifies it. `query` is the text after the `@` up to the caret.
 *
 * Kind deduction (heuristic):
 *   - Empty or explicit `@neuro*` → agent
 *   - Starts with a skill name prefix we know → skill
 *   - Otherwise → file
 *
 * We don't try to be clever — the user can switch kinds via Tab.
 */
export function matchAtToken(
  text: string,
  caret: number
): { query: string; kind: ComposerSuggestion["kind"] } | null {
  // Scan backwards from caret to find an `@`, bailing on whitespace or
  // another `@`.
  let i = caret - 1;
  while (i >= 0) {
    const ch = text[i];
    if (ch === "@") break;
    if (/\s/.test(ch)) return null;
    i--;
  }
  if (i < 0 || text[i] !== "@") return null;
  // Require that the `@` is either at the start or preceded by whitespace —
  // otherwise `email@example.com` would match.
  if (i > 0 && !/\s/.test(text[i - 1])) return null;
  const query = text.slice(i + 1, caret);
  // If query contains whitespace, the token has ended; no match.
  if (/\s/.test(query)) return null;
  const kind: ComposerSuggestion["kind"] = /^neuro/i.test(query)
    ? "agent"
    : "file";
  return { query, kind };
}
