// SPDX-License-Identifier: MIT
import { describe, expect, test } from "bun:test";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";
import { SystemPromptLoader, stripFrontmatter } from "../src/agent/system-prompt";

describe("stripFrontmatter", () => {
  test("removes the leading --- block", () => {
    const text = `---
title: x
---

body content`;
    // stripFrontmatter removes the `---...---\n` block; any blank lines
    // between the closing delimiter and the body are preserved.
    expect(stripFrontmatter(text).trim()).toBe("body content");
  });

  test("returns original text if no frontmatter", () => {
    expect(stripFrontmatter("just body")).toBe("just body");
  });
});

describe("SystemPromptLoader", () => {
  test("precedence: vault path wins over nlr-root", () => {
    const vault = fs.mkdtempSync(path.join(os.tmpdir(), "nlr-vault-"));
    const root = fs.mkdtempSync(path.join(os.tmpdir(), "nlr-root-"));
    makeAgentFile(vault, "vault prompt body");
    makeAgentFile(root, "root prompt body");

    const loader = new SystemPromptLoader({
      vaultPath: vault,
      nlrRoot: root,
    });
    const out = loader.load();
    expect(out).toContain("vault prompt body");
    expect(out).not.toContain("root prompt body");
    expect(loader.lastSource()).toBe("vault");

    cleanupTmp(vault);
    cleanupTmp(root);
  });

  test("falls back to NLR root when vault file is missing", () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), "nlr-root-"));
    makeAgentFile(root, "root prompt body");
    const loader = new SystemPromptLoader({
      vaultPath: "/does/not/exist",
      nlrRoot: root,
    });
    expect(loader.load()).toContain("root prompt body");
    expect(loader.lastSource()).toBe("nlr-root");
    cleanupTmp(root);
  });

  test("falls back to embedded prompt when both files missing", () => {
    const loader = new SystemPromptLoader({
      vaultPath: "/nope1",
      nlrRoot: "/nope2",
    });
    const out = loader.load();
    expect(out).toContain("Never write to '02-KB-main/' directly");
    expect(loader.lastSource()).toBe("fallback");
  });

  test("always appends the prompt-injection guardrail", () => {
    const loader = new SystemPromptLoader({});
    const out = loader.load();
    expect(out).toContain("Prompt-injection guardrail");
    expect(out).toContain("<tool-result");
  });

  test("caches within TTL, invalidate() forces reload", () => {
    const vault = fs.mkdtempSync(path.join(os.tmpdir(), "nlr-vault-"));
    makeAgentFile(vault, "first");
    const clock = { t: 1000 };
    const loader = new SystemPromptLoader({
      vaultPath: vault,
      ttlMs: 10_000,
      now: () => clock.t,
    });
    const first = loader.load();
    expect(first).toContain("first");
    makeAgentFile(vault, "second");
    clock.t = 5000;
    // Still cached
    expect(loader.load()).toContain("first");
    loader.invalidate();
    expect(loader.load()).toContain("second");
    cleanupTmp(vault);
  });
});

function makeAgentFile(dir: string, body: string): void {
  const agentsDir = path.join(dir, ".claude", "agents");
  fs.mkdirSync(agentsDir, { recursive: true });
  fs.writeFileSync(
    path.join(agentsDir, "neuro.md"),
    `---\nname: neuro\n---\n\n${body}\n`
  );
}

function cleanupTmp(dir: string): void {
  fs.rmSync(dir, { recursive: true, force: true });
}
