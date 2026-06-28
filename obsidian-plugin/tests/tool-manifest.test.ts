// SPDX-License-Identifier: MIT
import { describe, expect, test } from "bun:test";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";
import {
  ToolManifestLoader,
  parseSkillFrontmatter,
  skillToolDef,
  skillToolName,
} from "../src/agent/tool-manifest";

describe("parseSkillFrontmatter", () => {
  test("extracts name + description", () => {
    const text = `---
name: my-skill
description: Do the thing efficiently
---

# Body`;
    expect(parseSkillFrontmatter(text)).toEqual({
      name: "my-skill",
      description: "Do the thing efficiently",
    });
  });

  test("strips surrounding quotes", () => {
    const text = `---
name: "quoted-skill"
description: 'single quoted'
---
Body`;
    expect(parseSkillFrontmatter(text)).toEqual({
      name: "quoted-skill",
      description: "single quoted",
    });
  });

  test("returns null if no frontmatter", () => {
    expect(parseSkillFrontmatter("no frontmatter here")).toBe(null);
  });

  test("returns null if name missing", () => {
    const text = `---
description: only desc
---`;
    expect(parseSkillFrontmatter(text)).toBe(null);
  });

  test("falls back to stub description when missing", () => {
    // Frontmatter block must end with a newline after `---` per our parser
    // — matches how real markdown files look on disk.
    const text = `---
name: desc-less
---

# body`;
    expect(parseSkillFrontmatter(text)?.description).toBe("Skill desc-less");
  });
});

describe("skillToolDef / skillToolName", () => {
  test("produces tool name with run_skill_ prefix", () => {
    expect(skillToolName("my-skill")).toBe("run_skill_my-skill");
    expect(skillToolName("bad name!")).toBe("run_skill_bad_name_");
  });

  test("tool def has {args: string} input schema", () => {
    const def = skillToolDef({ name: "x", description: "y" });
    expect(def.name).toBe("run_skill_x");
    const params = def.parameters as Record<string, unknown>;
    expect((params.required as string[])).toContain("args");
    const props = params.properties as Record<string, { type: string }>;
    expect(props.args.type).toBe("string");
  });
});

describe("ToolManifestLoader", () => {
  test("filters MCP tools to tv_* and includes skill shims", async () => {
    const tmp = mkTempSkillsDir([
      { name: "alpha-skill", desc: "Alpha test" },
      { name: "beta-skill", desc: "Beta test" },
    ]);

    const loader = new ToolManifestLoader({
      mcp: {
        listTools: () =>
          Promise.resolve([
            { name: "tv_search", description: "search" },
            { name: "tv_read", description: "read" },
            { name: "other_tool", description: "not-tv" },
          ]),
      },
      skills: { skillsDir: tmp },
    });
    const tools = await loader.get();
    const names = tools.map((t) => t.name).sort();
    expect(names).toEqual([
      "run_skill_alpha-skill",
      "run_skill_beta-skill",
      "tv_read",
      "tv_search",
    ]);
    const counts = loader.lastCounts();
    expect(counts.mcp).toBe(2);
    expect(counts.skills).toBe(2);
    cleanupTmp(tmp);
  });

  test("caches within TTL, refreshes after", async () => {
    const tmp = mkTempSkillsDir([{ name: "s1", desc: "desc" }]);
    let calls = 0;
    const clock = { t: 1000 };
    const loader = new ToolManifestLoader({
      mcp: {
        listTools: () => {
          calls++;
          return Promise.resolve([{ name: "tv_a" }]);
        },
      },
      skills: { skillsDir: tmp },
      ttlMs: 100,
      now: () => clock.t,
    });
    await loader.get();
    expect(calls).toBe(1);
    clock.t = 1050; // within TTL
    await loader.get();
    expect(calls).toBe(1);
    clock.t = 2000; // past TTL
    await loader.get();
    expect(calls).toBe(2);
    cleanupTmp(tmp);
  });

  test("degrades gracefully when MCP listTools throws", async () => {
    const loader = new ToolManifestLoader({
      mcp: {
        listTools: () => Promise.reject(new Error("not connected")),
      },
    });
    const tools = await loader.get();
    expect(tools).toEqual([]);
  });
});

// ── helpers ──────────────────────────────────────────────────────────────

function mkTempSkillsDir(
  skills: Array<{ name: string; desc: string }>
): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "nlr-skills-test-"));
  for (const s of skills) {
    const sdir = path.join(dir, s.name);
    fs.mkdirSync(sdir);
    fs.writeFileSync(
      path.join(sdir, "SKILL.md"),
      `---\nname: ${s.name}\ndescription: ${s.desc}\n---\n# Body\n`
    );
  }
  return dir;
}

function cleanupTmp(dir: string): void {
  fs.rmSync(dir, { recursive: true, force: true });
}
