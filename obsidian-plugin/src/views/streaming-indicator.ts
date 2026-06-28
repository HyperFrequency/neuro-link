// SPDX-License-Identifier: AGPL-3.0-only
//
// Portions adapted from logancyang/obsidian-copilot
//   Copyright (c) 2023 Logan Yang
// See THIRD_PARTY/obsidian-copilot/LICENSE for the full license text.
//
// Conceptual source:
//   src/components/Chat/ChatStreamingIndicator.tsx (React component)
//
// Adaptation notes:
//   - Upstream uses React hooks + Tailwind classes. Rewritten to plain DOM
//     (createDiv, classList). The visual shape — three pulsing dots in a
//     row — is preserved.
//   - No state management; callers control visibility via `show()`/`hide()`.
//   - Animation cleanup on unmount prevents the animation-frame leak the
//     upstream has when rapidly mounting/unmounting between turns.

export class StreamingIndicator {
  private el: HTMLElement;
  private timer: ReturnType<typeof setInterval> | null = null;
  private dotIndex = 0;
  private dots: HTMLElement[] = [];

  constructor(parent: HTMLElement) {
    this.el = parent.createDiv({ cls: "nlr-streaming-indicator" });
    this.el.setAttribute("aria-label", "Assistant is responding");
    this.el.setAttribute("role", "status");
    for (let i = 0; i < 3; i++) {
      this.dots.push(this.el.createSpan({ cls: "nlr-streaming-dot" }));
    }
    this.hide();
  }

  show(): void {
    if (this.timer) return;
    this.el.classList.add("is-visible");
    this.timer = setInterval(() => this.tick(), 250);
  }

  hide(): void {
    if (this.timer) {
      clearInterval(this.timer);
      this.timer = null;
    }
    this.el.classList.remove("is-visible");
    for (const d of this.dots) d.classList.remove("is-active");
  }

  /** Remove from DOM. Always stops the timer first to avoid leaks. */
  destroy(): void {
    this.hide();
    this.el.remove();
  }

  private tick(): void {
    for (let i = 0; i < this.dots.length; i++) {
      this.dots[i].classList.toggle("is-active", i === this.dotIndex);
    }
    this.dotIndex = (this.dotIndex + 1) % this.dots.length;
  }
}
