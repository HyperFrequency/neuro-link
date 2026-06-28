import {
  ItemView,
  WorkspaceLeaf,
  Notice,
  setIcon,
} from "obsidian";
import type NLRPlugin from "./main";
import * as fs from "fs";
import * as path from "path";

export const VIEW_TYPE_STATS = "nlr-stats-view";

interface HeartbeatData {
  status: string;
  last_check: string;
  errors: string[];
}

interface SessionLogEntry {
  tool: string;
  timestamp: string;
  duration_ms?: number;
  success?: boolean;
}

interface ScoreEntry {
  session_id?: string;
  score: number;
  timestamp: string;
  dimensions?: Record<string, number>;
}

export class StatsView extends ItemView {
  plugin: NLRPlugin;

  constructor(leaf: WorkspaceLeaf, plugin: NLRPlugin) {
    super(leaf);
    this.plugin = plugin;
  }

  getViewType(): string {
    return VIEW_TYPE_STATS;
  }

  getDisplayText(): string {
    return "NLR Stats";
  }

  getIcon(): string {
    return "nlr-chart";
  }

  async onOpen(): Promise<void> {
    await this.render();
  }

  async onClose(): Promise<void> {
    // nothing to clean up
  }

  private async render(): Promise<void> {
    const container = this.containerEl.children[1] as HTMLElement;
    container.empty();
    container.addClass("nlr-stats-container");

    const header = container.createDiv({ cls: "nlr-stats-header" });
    header.createEl("h4", { text: "NLR Dashboard" });

    const refreshBtn = header.createEl("button", {
      cls: "nlr-chatbot-btn nlr-chatbot-btn-small",
      attr: { "aria-label": "Refresh" },
    });
    setIcon(refreshBtn, "refresh-cw");
    refreshBtn.addEventListener("click", () => this.render());

    const nlrRoot = this.plugin.settings.nlrRoot;
    if (!nlrRoot) {
      container.createEl("p", {
        text: "NLR Root path not configured. Set it in Settings.",
        cls: "nlr-error",
      });
      return;
    }

    const heartbeat = this.readHeartbeat(nlrRoot);
    const sessionLog = this.readJsonl<SessionLogEntry>(path.join(nlrRoot, "state", "session_log.jsonl"));
    const scoreHistory = this.readJsonl<ScoreEntry>(path.join(nlrRoot, "state", "score_history.jsonl"));
    const wikiPages = this.countFiles(path.join(nlrRoot, "02-KB-main"), ".md");
    const pendingTasks = this.countPendingTasks(nlrRoot);
    const gapCount = this.countFiles(path.join(nlrRoot, "05-insights-gaps"), ".md");

    this.renderHealthCard(container, heartbeat);
    this.renderSummaryCards(container, wikiPages, pendingTasks, gapCount, sessionLog, scoreHistory);
    this.renderToolUsageChart(container, sessionLog);
    this.renderScoreTrend(container, scoreHistory);
    this.renderRecentActivity(container, sessionLog);
  }

  private renderHealthCard(container: HTMLElement, heartbeat: HeartbeatData | null): void {
    const card = container.createDiv({ cls: "nlr-stats-card" });
    card.createEl("h5", { text: "System Health" });

    if (!heartbeat) {
      card.createEl("p", { text: "No heartbeat data", cls: "nlr-stats-muted" });
      return;
    }

    const statusEl = card.createDiv({ cls: "nlr-stats-health-row" });
    const statusDot = statusEl.createEl("span", {
      cls: `nlr-stats-dot nlr-stats-dot-${heartbeat.status === "initialized" || heartbeat.status === "healthy" ? "green" : "red"}`,
    });
    statusDot.textContent = "\u25CF";
    statusEl.createEl("span", { text: ` Status: ${heartbeat.status}` });

    card.createEl("p", {
      text: `Last check: ${new Date(heartbeat.last_check).toLocaleString()}`,
      cls: "nlr-stats-muted",
    });

    if (heartbeat.errors.length > 0) {
      const errList = card.createEl("ul", { cls: "nlr-stats-error-list" });
      for (const err of heartbeat.errors) {
        errList.createEl("li", { text: err, cls: "nlr-error" });
      }
    }
  }

  private renderSummaryCards(
    container: HTMLElement,
    wikiPages: number,
    pendingTasks: number,
    gapCount: number,
    sessionLog: SessionLogEntry[],
    scoreHistory: ScoreEntry[]
  ): void {
    const grid = container.createDiv({ cls: "nlr-stats-grid" });

    this.createMetricCard(grid, "Wiki Pages", String(wikiPages), "file-text");
    this.createMetricCard(grid, "Pending Tasks", String(pendingTasks), "list-todo");
    this.createMetricCard(grid, "Knowledge Gaps", String(gapCount), "alert-triangle");

    const successCount = sessionLog.filter((e) => e.success === true).length;
    const totalWithStatus = sessionLog.filter((e) => e.success !== undefined).length;
    const rate = totalWithStatus > 0 ? Math.round((successCount / totalWithStatus) * 100) : 0;
    this.createMetricCard(grid, "Success Rate", `${rate}%`, "check-circle");

    const avgScore = scoreHistory.length > 0
      ? (scoreHistory.reduce((sum, e) => sum + e.score, 0) / scoreHistory.length).toFixed(1)
      : "N/A";
    this.createMetricCard(grid, "Avg Score", avgScore, "star");
    this.createMetricCard(grid, "Sessions", String(scoreHistory.length), "activity");
  }

  private createMetricCard(parent: HTMLElement, label: string, value: string, icon: string): void {
    const card = parent.createDiv({ cls: "nlr-stats-metric" });
    const iconEl = card.createDiv({ cls: "nlr-stats-metric-icon" });
    setIcon(iconEl, icon);
    card.createEl("div", { text: value, cls: "nlr-stats-metric-value" });
    card.createEl("div", { text: label, cls: "nlr-stats-metric-label" });
  }

  private renderToolUsageChart(container: HTMLElement, sessionLog: SessionLogEntry[]): void {
    const card = container.createDiv({ cls: "nlr-stats-card" });
    card.createEl("h5", { text: "Tool Usage" });

    if (sessionLog.length === 0) {
      card.createEl("p", { text: "No session data", cls: "nlr-stats-muted" });
      return;
    }

    const toolCounts: Record<string, number> = {};
    for (const entry of sessionLog) {
      if (entry.tool) {
        toolCounts[entry.tool] = (toolCounts[entry.tool] || 0) + 1;
      }
    }

    const sorted = Object.entries(toolCounts).sort((a, b) => b[1] - a[1]).slice(0, 15);
    if (sorted.length === 0) {
      card.createEl("p", { text: "No tool usage data", cls: "nlr-stats-muted" });
      return;
    }

    const maxCount = sorted[0][1];
    const chartEl = card.createDiv({ cls: "nlr-stats-bar-chart" });

    for (const [tool, count] of sorted) {
      const row = chartEl.createDiv({ cls: "nlr-stats-bar-row" });
      row.createDiv({ cls: "nlr-stats-bar-label", text: tool });

      const barContainer = row.createDiv({ cls: "nlr-stats-bar-container" });
      const bar = barContainer.createDiv({ cls: "nlr-stats-bar" });
      const pct = maxCount > 0 ? (count / maxCount) * 100 : 0;
      bar.style.width = `${pct}%`;

      row.createDiv({ cls: "nlr-stats-bar-value", text: String(count) });
    }
  }

  private renderScoreTrend(container: HTMLElement, scoreHistory: ScoreEntry[]): void {
    const card = container.createDiv({ cls: "nlr-stats-card" });
    card.createEl("h5", { text: "Score Trend" });

    if (scoreHistory.length < 2) {
      card.createEl("p", { text: "Need at least 2 sessions for trend", cls: "nlr-stats-muted" });
      return;
    }

    const recent = scoreHistory.slice(-20);
    const canvas = card.createEl("canvas", {
      cls: "nlr-stats-canvas",
      attr: { width: "400", height: "150" },
    });

    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const scores = recent.map((e) => e.score);
    const minScore = Math.min(...scores);
    const maxScore = Math.max(...scores);
    const range = maxScore - minScore || 1;

    const w = canvas.width;
    const h = canvas.height;
    const padding = 20;
    const plotW = w - padding * 2;
    const plotH = h - padding * 2;

    ctx.strokeStyle = "var(--text-muted, #888)";
    ctx.lineWidth = 0.5;
    ctx.beginPath();
    ctx.moveTo(padding, padding);
    ctx.lineTo(padding, h - padding);
    ctx.lineTo(w - padding, h - padding);
    ctx.stroke();

    ctx.strokeStyle = "var(--interactive-accent, #7b68ee)";
    ctx.lineWidth = 2;
    ctx.beginPath();

    for (let i = 0; i < scores.length; i++) {
      const x = padding + (i / (scores.length - 1)) * plotW;
      const y = h - padding - ((scores[i] - minScore) / range) * plotH;
      if (i === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    }
    ctx.stroke();

    for (let i = 0; i < scores.length; i++) {
      const x = padding + (i / (scores.length - 1)) * plotW;
      const y = h - padding - ((scores[i] - minScore) / range) * plotH;
      ctx.fillStyle = "var(--interactive-accent, #7b68ee)";
      ctx.beginPath();
      ctx.arc(x, y, 3, 0, Math.PI * 2);
      ctx.fill();
    }

    ctx.fillStyle = "var(--text-muted, #888)";
    ctx.font = "10px sans-serif";
    ctx.fillText(maxScore.toFixed(1), 2, padding + 4);
    ctx.fillText(minScore.toFixed(1), 2, h - padding + 4);
  }

  private renderRecentActivity(container: HTMLElement, sessionLog: SessionLogEntry[]): void {
    const card = container.createDiv({ cls: "nlr-stats-card" });
    card.createEl("h5", { text: "Recent Activity" });

    const recent = sessionLog.slice(-10).reverse();
    if (recent.length === 0) {
      card.createEl("p", { text: "No recent activity", cls: "nlr-stats-muted" });
      return;
    }

    const table = card.createEl("table", { cls: "nlr-stats-table" });
    const thead = table.createEl("thead");
    const headerRow = thead.createEl("tr");
    headerRow.createEl("th", { text: "Tool" });
    headerRow.createEl("th", { text: "Time" });
    headerRow.createEl("th", { text: "Duration" });
    headerRow.createEl("th", { text: "Status" });

    const tbody = table.createEl("tbody");
    for (const entry of recent) {
      const row = tbody.createEl("tr");
      row.createEl("td", { text: entry.tool || "unknown" });
      row.createEl("td", {
        text: entry.timestamp ? new Date(entry.timestamp).toLocaleTimeString() : "-",
      });
      row.createEl("td", {
        text: entry.duration_ms !== undefined ? `${entry.duration_ms}ms` : "-",
      });
      const statusCell = row.createEl("td");
      if (entry.success === true) {
        statusCell.createEl("span", { text: "\u2713", cls: "nlr-stats-success" });
      } else if (entry.success === false) {
        statusCell.createEl("span", { text: "\u2717", cls: "nlr-stats-failure" });
      } else {
        statusCell.createEl("span", { text: "-" });
      }
    }
  }

  private readHeartbeat(nlrRoot: string): HeartbeatData | null {
    const filePath = path.join(nlrRoot, "state", "heartbeat.json");
    try {
      const content = fs.readFileSync(filePath, "utf-8");
      return JSON.parse(content) as HeartbeatData;
    } catch {
      return null;
    }
  }

  private readJsonl<T>(filePath: string): T[] {
    try {
      const content = fs.readFileSync(filePath, "utf-8");
      return content
        .split("\n")
        .filter((line) => line.trim())
        .map((line) => JSON.parse(line) as T);
    } catch {
      return [];
    }
  }

  private countFiles(dirPath: string, extension: string): number {
    try {
      if (!fs.existsSync(dirPath)) return 0;
      return fs.readdirSync(dirPath).filter((f) => f.endsWith(extension)).length;
    } catch {
      return 0;
    }
  }

  private countPendingTasks(nlrRoot: string): number {
    const taskDir = path.join(nlrRoot, "07-neuro-link-task");
    try {
      if (!fs.existsSync(taskDir)) return 0;
      const files = fs.readdirSync(taskDir).filter((f) => f.endsWith(".md"));
      let pending = 0;
      for (const file of files) {
        try {
          const content = fs.readFileSync(path.join(taskDir, file), "utf-8");
          if (content.includes("status: pending")) {
            pending++;
          }
        } catch {
          // skip unreadable files
        }
      }
      return pending;
    } catch {
      return 0;
    }
  }
}
