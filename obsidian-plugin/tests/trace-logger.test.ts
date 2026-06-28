// SPDX-License-Identifier: MIT
import "./_obsidian-mock";
import { describe, expect, test } from "bun:test";
import {
  formatLine,
  parseTailLines,
  MemoryTraceLogger,
} from "../src/agent/trace-logger";

describe("formatLine", () => {
  test("emits a JSON bullet line", () => {
    const line = formatLine({
      callId: "c1",
      tool: "tv_search",
      arguments: '{"q":"x"}',
      outcome: "ok",
    });
    expect(line.startsWith("- ")).toBe(true);
    const obj = JSON.parse(line.slice(2));
    expect(obj.call_id).toBe("c1");
    expect(obj.tool).toBe("tv_search");
    expect(obj.outcome).toBe("ok");
    expect(typeof obj.ts).toBe("string");
  });

  test("truncates long arg payloads", () => {
    const longArgs = '{"blob":"' + "x".repeat(1000) + '"}';
    const line = formatLine({
      callId: "c",
      tool: "t",
      arguments: longArgs,
      outcome: "ok",
    });
    const obj = JSON.parse(line.slice(2));
    expect(String(obj.args).length).toBeLessThan(500);
    expect(String(obj.args).endsWith("…")).toBe(true);
  });

  test("carries summary, conv id, rollback when present", () => {
    const line = formatLine({
      callId: "c",
      tool: "t",
      arguments: "{}",
      outcome: "refused",
      summary: "not allowed",
      conversationId: "conv-1",
      rollbackCommand: "nlr_wiki_delete",
    });
    const obj = JSON.parse(line.slice(2));
    expect(obj.summary).toBe("not allowed");
    expect(obj.conv).toBe("conv-1");
    expect(obj.rollback).toBe("nlr_wiki_delete");
  });
});

describe("parseTailLines", () => {
  test("parses the last N JSON bullet lines", () => {
    const text = [
      "# header",
      "",
      "- " + JSON.stringify({ call_id: "a", tool: "x", outcome: "ok", args: "{}", ts: "t1" }),
      "- " + JSON.stringify({ call_id: "b", tool: "y", outcome: "refused", args: "{}", ts: "t2" }),
      "- not-json-garbage",
      "- " + JSON.stringify({ call_id: "c", tool: "z", outcome: "ok", args: "{}", ts: "t3" }),
      "",
    ].join("\n");
    const tail = parseTailLines(text, 2);
    // Newest-first ordering
    expect(tail[0].callId).toBe("c");
    expect(tail[1].callId).toBe("b");
  });
});

describe("MemoryTraceLogger", () => {
  test("appends and tails entries", async () => {
    const log = new MemoryTraceLogger();
    await log.append({
      callId: "x",
      tool: "t",
      arguments: "{}",
      outcome: "ok",
    });
    await log.append({
      callId: "y",
      tool: "t",
      arguments: "{}",
      outcome: "error",
      summary: "boom",
    });
    const tail = await log.tail(10);
    expect(tail.length).toBe(2);
    expect(tail[0].callId).toBe("y");
    expect(tail[1].callId).toBe("x");
  });
});
