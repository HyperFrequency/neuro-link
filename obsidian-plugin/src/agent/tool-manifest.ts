// SPDX-License-Identifier: MIT
//
// Clean-room. Not adapted from obsidian-copilot.
//
// Dynamic tool-manifest loader for the @neuro agent.
//
// Sources:
//   1. `tv_*` tools — fetched via `tools/list` on the live MCP connection.
//      The subscription client exposes a `listTools()` helper; this module
//      consumes whatever shape it returns and filters to `name.startsWith
//      ("tv_")`.
//   2. Skill shims — every `.claude/skills/*/SKILL.md` becomes a tool
//      whose single input is `{args: string}`. Invocation is delegated to
//      the caller (the agent loop), which shells via `nlr_task_dispatch`
//      (or the plugin's existing `runNlrCommand`) — this module just builds
//      the ToolSpec metadata.
//
// Cache:
//   - 60s TTL keyed on the source-set signature. `refresh()` forces a rebuild.

import type { LLMToolDefinition } from "../providers/base";
import * as fs from "fs";
import * as path from "path";

/** Shape of whatever provides live `tv_*` tools. Abstracted for tests. */
export interface McpToolSource {
  listTools(): Promise<Array<{ name: string; description?: string; inputSchema?: Record<string, unknown> }>>;
}

export interface SkillSource {
  /** Absolute path to the directory containing `<skill>/SKILL.md` subdirs. */
  skillsDir: string;
}

export interface ToolManifestOptions {
  mcp?: McpToolSource;
  skills?: SkillSource;
  ttlMs?: number;
  /** Monotonic clock injection for deterministic tests. */
  now?: () => number;
}

interface CacheEntry {
  expiresAt: number;
  tools: LLMToolDefinition[];
  skillCount: number;
  mcpToolCount: number;
}

const DEFAULT_TTL_MS = 60_000;

export class ToolManifestLoader {
  private opts: Required<ToolManifestOptions>;
  private cache: CacheEntry | null = null;

  constructor(opts: ToolManifestOptions = {}) {
    this.opts = {
      mcp: opts.mcp ?? { listTools: () => Promise.resolve([]) },
      skills: opts.skills ?? { skillsDir: "" },
      ttlMs: opts.ttlMs ?? DEFAULT_TTL_MS,
      now: opts.now ?? Date.now,
    };
  }

  /** Return cached manifest if still valid, otherwise rebuild. */
  async get(): Promise<LLMToolDefinition[]> {
    const now = this.opts.now();
    if (this.cache && this.cache.expiresAt > now) {
      return this.cache.tools;
    }
    return this.refresh();
  }

  async refresh(): Promise<LLMToolDefinition[]> {
    const now = this.opts.now();
    const [mcpTools, skillTools] = await Promise.all([
      this.loadMcpTools(),
      Promise.resolve(this.loadSkillTools()),
    ]);
    const tools = [...mcpTools, ...skillTools];
    this.cache = {
      expiresAt: now + this.opts.ttlMs,
      tools,
      mcpToolCount: mcpTools.length,
      skillCount: skillTools.length,
    };
    return tools;
  }

  /** Diagnostic counts exposed to UI (e.g. "13 MCP + 8 skills"). */
  lastCounts(): { mcp: number; skills: number } {
    return {
      mcp: this.cache?.mcpToolCount ?? 0,
      skills: this.cache?.skillCount ?? 0,
    };
  }

  private async loadMcpTools(): Promise<LLMToolDefinition[]> {
    try {
      const raw = await this.opts.mcp.listTools();
      return raw
        .filter((t) => typeof t.name === "string" && t.name.startsWith("tv_"))
        .map<LLMToolDefinition>((t) => ({
          name: t.name,
          description: t.description ?? `TurboVault tool ${t.name}`,
          parameters: t.inputSchema ?? { type: "object", properties: {} },
        }));
    } catch {
      // MCP may not be connected yet — degrade gracefully rather than
      // blocking the agent from answering with local tools.
      return [];
    }
  }

  private loadSkillTools(): LLMToolDefinition[] {
    const dir = this.opts.skills.skillsDir;
    if (!dir || !fs.existsSync(dir)) return [];
    const out: LLMToolDefinition[] = [];
    let names: string[] = [];
    try {
      names = fs.readdirSync(dir);
    } catch {
      return [];
    }
    for (const entry of names) {
      const skillMd = path.join(dir, entry, "SKILL.md");
      if (!fs.existsSync(skillMd)) continue;
      try {
        const contents = fs.readFileSync(skillMd, "utf-8");
        const spec = parseSkillFrontmatter(contents);
        if (!spec) continue;
        out.push(skillToolDef(spec));
      } catch {
        /* unreadable skill — skip */
      }
    }
    return out;
  }
}

export interface SkillFrontmatter {
  name: string;
  description: string;
}

/**
 * Minimal YAML-ish frontmatter parser — just enough to pick out `name:` and
 * `description:` from a block bounded by `---` lines. We intentionally do
 * NOT pull in a full YAML dep; skills declare these two fields with known
 * shape. Multiline descriptions (folded with `>` or continuations) collapse
 * onto one line.
 */
export function parseSkillFrontmatter(text: string): SkillFrontmatter | null {
  const match = text.match(/^---\s*\n([\s\S]*?)\n---\s*\n/);
  if (!match) return null;
  const block = match[1];

  const name = extractField(block, "name");
  const description = extractField(block, "description");
  if (!name) return null;

  return {
    name,
    description: description ?? `Skill ${name}`,
  };
}

function extractField(block: string, key: string): string | null {
  const re = new RegExp(`^${key}:\\s*(.*)$`, "m");
  const m = block.match(re);
  if (!m) return null;
  let value = m[1].trim();
  // Strip surrounding quotes if present.
  if ((value.startsWith('"') && value.endsWith('"')) || (value.startsWith("'") && value.endsWith("'"))) {
    value = value.slice(1, -1);
  }
  // If empty on the same line, may be a folded block (not handled — return
  // null to signal "description absent" rather than misinterpret).
  if (!value) return null;
  return value;
}

/**
 * Build a ToolDefinition the model can invoke. The single argument is
 * `{args: string}` — opaque free-form invocation arguments — because skills
 * don't expose structured parameter schemas. The caller interprets `args`
 * when dispatching the skill via `nlr_task_dispatch`.
 */
export function skillToolDef(spec: SkillFrontmatter): LLMToolDefinition {
  return {
    name: skillToolName(spec.name),
    description: spec.description,
    parameters: {
      type: "object",
      properties: {
        args: {
          type: "string",
          description:
            "Free-form invocation arguments passed to the skill. Example: '<topic>' or '--model sonnet'.",
        },
      },
      required: ["args"],
    },
  };
}

export function skillToolName(skillName: string): string {
  const safe = skillName.replace(/[^a-zA-Z0-9_-]/g, "_");
  return `run_skill_${safe}`;
}
