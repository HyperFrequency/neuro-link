// SPDX-License-Identifier: MIT
//
// Clean-room. Not adapted from obsidian-copilot.
//
// Safety gates for the @neuro agent's tool-use loop. Every write-intent tool
// call MUST pass through `checkWriteSafety` before the tool is executed; any
// failure short-circuits with a structured refusal the model sees as a
// tool-result (so it can retry), not a raw exception.
//
// Design notes:
//   - We only match path arguments, not arbitrary tool payloads. If a new
//     write-capable tool appears whose path lives under a non-standard key,
//     extend WRITE_TOOL_PATH_KEYS below.
//   - Allowed-path matching is glob-ish, not regex. `**` == any depth,
//     `*` == single segment. Keep the globs narrow to reduce blast radius.
//   - The 02-KB-main/ schema-routing gate is second, because it's more
//     specific: a `tv_write_note` to `02-KB-main/foo.md` is *path-allowed*
//     (02-KB-main is allowed) but *tool-wrong* — it must go through
//     `nlr_wiki_*`. Order matters.

/**
 * Write-intent tools we recognize. The key names vary because the MCP tool
 * schema isn't standardized; we check any of these for a path string.
 */
const WRITE_INTENT_TOOLS = new Set<string>([
  "tv_write_note",
  "tv_edit_note",
  "tv_delete_note",
  "tv_rename_note",
  "tv_append_note",
  "tv_batch_execute",
  "nlr_wiki_create",
  "nlr_wiki_update",
  "nlr_task_create",
  "nlr_task_update",
  "nlr_state_log",
  "nlr_config_read", // read-only but namespace matches; safe to allow
]);

const WRITE_TOOL_PATH_KEYS = [
  "path",
  "note_path",
  "file_path",
  "target",
  "target_path",
  "source_path",
  "old_path",
  "new_path",
];

/**
 * Default allowed-path globs. A write-intent tool whose path does NOT match
 * any glob is refused. Read-only tools skip this check.
 *
 * Order matches the Phase 7 spec: raw sources, curated wiki (via schema
 * tools only), task queue, agent memory log (append-only), HITL inbox,
 * recursive improvement, self-improvement-HITL, code docs.
 */
export const DEFAULT_ALLOWED_PATHS: readonly string[] = Object.freeze([
  "01-raw/**",
  "02-KB-main/**",
  "00-neuro-link/tasks/**",
  "04-Agent-Memory/logs.md",
  "05-insights-HITL/**",
  "06-Recursive/**",
  "07-self-improvement-HITL/**",
  "08-code-docs/**",
]);

export interface SafetyContext {
  /** Which path globs are permitted. Defaults to DEFAULT_ALLOWED_PATHS. */
  allowedPaths?: readonly string[];
}

export interface SafetyRefusal {
  /** Tool name that was blocked. */
  tool: string;
  /** Path argument that caused the block (if resolvable). */
  path?: string;
  reason:
    | "path-not-allowed"
    | "use-nlr-wiki-for-02kb"
    | "unknown-tool-write-intent"
    | "argument-parse-error";
  /** Human-readable message the model sees in the tool-result envelope. */
  message: string;
}

/**
 * Returns null if the call is allowed; otherwise a refusal object the caller
 * should return to the agent as a tool-result. Never throws — a refusal is
 * just data the model must react to.
 */
export function checkWriteSafety(
  toolName: string,
  rawArguments: string,
  ctx: SafetyContext = {}
): SafetyRefusal | null {
  // Read-only tools (not in WRITE_INTENT_TOOLS) are unconditionally allowed.
  if (!isWriteIntent(toolName)) return null;

  const allowed = ctx.allowedPaths ?? DEFAULT_ALLOWED_PATHS;
  let parsed: Record<string, unknown>;
  try {
    parsed = JSON.parse(rawArguments || "{}") as Record<string, unknown>;
  } catch {
    return {
      tool: toolName,
      reason: "argument-parse-error",
      message:
        `Tool call '${toolName}' had arguments that are not valid JSON. ` +
        `Retry with a well-formed JSON object.`,
    };
  }

  const targetPath = extractPath(parsed);
  // A write tool without any recognisable path — block with an explanatory
  // message rather than silently allowing. If a legit tool matches this
  // pattern in the future, add its key to WRITE_TOOL_PATH_KEYS.
  if (!targetPath) {
    return {
      tool: toolName,
      reason: "unknown-tool-write-intent",
      message:
        `Tool call '${toolName}' is write-capable but its target path cannot be resolved ` +
        `from the arguments. Ensure the call includes a 'path', 'note_path', or 'target' field.`,
    };
  }

  // 02-KB-main schema routing. Writes to 02-KB-main/ MUST go through
  // nlr_wiki_create or nlr_wiki_update — they enforce the wiki schema
  // frontmatter. Any other write tool targeting 02-KB-main is rejected
  // here even though the path itself is allowed.
  if (isUnder02KbMain(targetPath) && !isNlrWikiWrite(toolName)) {
    return {
      tool: toolName,
      path: targetPath,
      reason: "use-nlr-wiki-for-02kb",
      message:
        `Writes to '02-KB-main/' must go through 'nlr_wiki_create' or 'nlr_wiki_update' ` +
        `to enforce schema frontmatter. Retry the operation using the schema-aware tool.`,
    };
  }

  if (!isAllowed(targetPath, allowed)) {
    return {
      tool: toolName,
      path: targetPath,
      reason: "path-not-allowed",
      message:
        `Path '${targetPath}' is outside the @neuro agent's allowed write zones. ` +
        `Allowed globs: ${allowed.join(", ")}. ` +
        `If this write is legitimate, the user must relax 'allowed_paths' in config/neuro-link.md.`,
    };
  }

  return null;
}

// ── helpers ────────────────────────────────────────────────────────────────

function isWriteIntent(tool: string): boolean {
  // Explicit allowlist. We treat anything starting with nlr_wiki_ or
  // nlr_task_ as write-intent too (defensive — some newer wiki/task tools
  // may not be in the static set above).
  if (WRITE_INTENT_TOOLS.has(tool)) return true;
  if (tool.startsWith("nlr_wiki_") && !tool.endsWith("_read") && !tool.endsWith("_list") && !tool.endsWith("_search")) {
    return true;
  }
  if (tool.startsWith("nlr_task_") && !tool.endsWith("_list") && !tool.endsWith("_read")) {
    return true;
  }
  // tv_* writes: `write`, `edit`, `delete`, `rename`, `move`, `create`.
  if (/^tv_(write|edit|delete|rename|move|create|append|batch_execute)/i.test(tool)) return true;
  return false;
}

function isNlrWikiWrite(tool: string): boolean {
  return tool === "nlr_wiki_create" || tool === "nlr_wiki_update";
}

function extractPath(args: Record<string, unknown>): string | null {
  for (const key of WRITE_TOOL_PATH_KEYS) {
    const v = args[key];
    if (typeof v === "string" && v.length > 0) return v.replace(/\\/g, "/");
  }
  return null;
}

function isUnder02KbMain(p: string): boolean {
  const norm = p.replace(/^\.\//, "").replace(/\\/g, "/");
  return norm.startsWith("02-KB-main/") || norm === "02-KB-main";
}

/**
 * Glob matcher. Supports `**` (any depth, including zero segments) and `*`
 * (one segment, no slashes). Anything else is a literal character. Anchored
 * to the full string (both ends).
 */
export function isAllowed(targetPath: string, globs: readonly string[]): boolean {
  const norm = targetPath.replace(/^\.\//, "").replace(/\\/g, "/");
  for (const g of globs) {
    if (matchGlob(norm, g)) return true;
  }
  return false;
}

function matchGlob(pathStr: string, glob: string): boolean {
  // Translate glob to an anchored RegExp. Escape every regex metachar except
  // our two wildcards, then substitute them.
  let re = "";
  let i = 0;
  while (i < glob.length) {
    const ch = glob[i];
    if (ch === "*") {
      if (glob[i + 1] === "*") {
        // `**` matches any run of any chars (including `/`) or empty.
        re += ".*";
        i += 2;
        // Eat a trailing `/` after `**` so `**/foo` matches `foo` too.
        if (glob[i] === "/") i++;
        continue;
      }
      // `*` matches one path segment — no slashes.
      re += "[^/]*";
      i++;
      continue;
    }
    // Escape regex metachars.
    if (/[\\^$.*+?()[\]{}|]/.test(ch)) re += `\\${ch}`;
    else re += ch;
    i++;
  }
  const regex = new RegExp(`^${re}$`);
  return regex.test(pathStr);
}
