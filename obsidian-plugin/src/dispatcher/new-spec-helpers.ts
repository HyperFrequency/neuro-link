/**
 * Pure helpers for the new-spec dispatcher. Kept separate from new-spec.ts
 * so the unit tests can import them without pulling the `obsidian` runtime.
 */

import { createHash } from "crypto";

/**
 * Stable content hash used to detect mid-flight edits — if the source file
 * changes between the pre-LLM read and the post-LLM write, we must discard
 * the generated spec rather than persist stale content.
 *
 * SHA-256 is cheap for the small markdown files the dispatcher sees
 * (typically < 10 KiB) and gives effectively zero collision risk without
 * bringing in a crypto dependency — Node's `crypto` module is available in
 * Electron / the Obsidian runtime.
 */
export function hashContent(content: string): string {
  return createHash("sha256").update(content, "utf8").digest("hex");
}

export interface TaskSpec {
  slug: string;
  title: string;
  type: string;
  priority: number;
  description: string;
  dependencies: string[];
}

export function validateSpec(raw: Record<string, unknown>): TaskSpec {
  const slug = asString(raw.slug);
  const title = asString(raw.title);
  const type = asString(raw.type || "other");
  const description = asString(raw.description);
  if (!slug || !title || !description) {
    throw new Error("task spec missing required fields");
  }
  const priorityRaw = raw.priority;
  const priority =
    typeof priorityRaw === "number" && priorityRaw >= 1 && priorityRaw <= 5 ? priorityRaw : 3;
  const deps = Array.isArray(raw.dependencies)
    ? raw.dependencies.filter((d): d is string => typeof d === "string")
    : [];
  return { slug, title, type, priority, description, dependencies: deps };
}

function asString(v: unknown): string {
  return typeof v === "string" ? v.trim() : "";
}

export function sanitiseSlug(s: string): string {
  const cleaned = s
    .toLowerCase()
    .replace(/[^a-z0-9\-_.]/g, "-")
    .replace(/-+/g, "-")
    .replace(/^-|-$/g, "");
  return cleaned || "task";
}

export function renderTaskMarkdown(sourcePath: string, spec: TaskSpec): string {
  const now = new Date().toISOString();
  const frontmatter = [
    "---",
    `title: "${escapeYaml(spec.title)}"`,
    `type: ${spec.type}`,
    `priority: ${spec.priority}`,
    "status: pending",
    `source: "${escapeYaml(sourcePath)}"`,
    `created: "${now}"`,
    spec.dependencies.length > 0
      ? `dependencies:\n${spec.dependencies.map((d) => `  - ${d}`).join("\n")}`
      : "dependencies: []",
    "---",
  ].join("\n");
  return `${frontmatter}\n\n# ${spec.title}\n\n${spec.description}\n\n_Generated from ${sourcePath} by the new-spec dispatcher on ${now}._\n`;
}

function escapeYaml(s: string): string {
  return s.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
}

export const FALLBACK_PROMPT = `You are the neuro-link task-spec generator.

A user dropped a new spec file into \`00-neuro-link/\`. Read its contents and
emit a single task spec via the \`emit_task_spec\` tool. The spec should
capture what the user wants done, with:

- A kebab-case \`slug\` suitable for a filename (no .md).
- A concise \`title\` (< 60 chars).
- A \`type\` from: ingest, curate, scan, repair, report, ontology, other.
- A \`priority\` 1-5 (1 = critical, 5 = background). Default 3.
- A \`description\` explaining what the downstream /job-scanner should do.
- Any known \`dependencies\` (as task slugs).

Source file: {{ file_path }}

---

{{ content }}
`;
