// SPDX-License-Identifier: MIT
import { describe, expect, test } from "bun:test";
import { parseSseStream, DEFAULT_MAX_EVENT_BYTES } from "../src/providers/sse";
import { LLMProviderError } from "../src/providers/base";

function streamFromString(s: string): ReadableStream<Uint8Array> {
  const encoder = new TextEncoder();
  const bytes = encoder.encode(s);
  return new ReadableStream({
    start(controller) {
      controller.enqueue(bytes);
      controller.close();
    },
  });
}

function chunkedStream(chunks: string[]): ReadableStream<Uint8Array> {
  const encoder = new TextEncoder();
  return new ReadableStream({
    start(controller) {
      for (const c of chunks) controller.enqueue(encoder.encode(c));
      controller.close();
    },
  });
}

async function collect(iter: AsyncIterable<string>): Promise<string[]> {
  const out: string[] = [];
  for await (const v of iter) out.push(v);
  return out;
}

describe("parseSseStream", () => {
  test("yields single-line data payloads", async () => {
    const stream = streamFromString(`data: hello\n\ndata: world\n\n`);
    expect(await collect(parseSseStream(stream))).toEqual(["hello", "world"]);
  });

  test("handles CRLF line endings", async () => {
    const stream = streamFromString(`data: one\r\n\r\ndata: two\r\n\r\n`);
    expect(await collect(parseSseStream(stream))).toEqual(["one", "two"]);
  });

  test("concatenates multi-line data within one event", async () => {
    const stream = streamFromString(`data: line1\ndata: line2\n\n`);
    expect(await collect(parseSseStream(stream))).toEqual(["line1\nline2"]);
  });

  test("survives chunk splits mid-event", async () => {
    const stream = chunkedStream(["data: par", "tial pay", "load\n\n"]);
    expect(await collect(parseSseStream(stream))).toEqual(["partial payload"]);
  });

  test("flushes trailing event on EOF without blank line", async () => {
    const stream = streamFromString(`data: trailing`);
    expect(await collect(parseSseStream(stream))).toEqual(["trailing"]);
  });

  test("strips single leading space from data line", async () => {
    const stream = streamFromString(`data:no-space\n\ndata: with-space\n\n`);
    expect(await collect(parseSseStream(stream))).toEqual(["no-space", "with-space"]);
  });

  test("ignores non-data event fields", async () => {
    const stream = streamFromString(`event: notice\ndata: payload\n\nid: 42\n\n`);
    // event: notice is paired with its data line; the id-only event has no data.
    expect(await collect(parseSseStream(stream))).toEqual(["payload"]);
  });
});

// ── Should-fix #6: SSE buffer cap ─────────────────────────────────────────
//
// A misbehaving provider that streams MB-scale events (or never terminates
// an event with a blank line) could OOM the plugin's renderer. parseSseStream
// now caps the in-progress buffer; exceeding throws an LLMProviderError with
// kind="bad_request" and the configured provider name.
describe("parseSseStream buffer cap", () => {
  test("DEFAULT_MAX_EVENT_BYTES is 1 MiB", () => {
    expect(DEFAULT_MAX_EVENT_BYTES).toBe(1024 * 1024);
  });

  test("throws LLMProviderError when a single event exceeds the cap", async () => {
    // Feed 2 MiB of 'a' with no blank-line terminator — the parser can
    // never emit, so the buffer grows past the 1 MiB default.
    const payload = "data: " + "a".repeat(2 * 1024 * 1024);
    // Chunk so the parser actually reads multiple times; overflow is checked
    // after each read.
    const chunks: string[] = [];
    const chunkSize = 256 * 1024;
    for (let i = 0; i < payload.length; i += chunkSize) {
      chunks.push(payload.substring(i, i + chunkSize));
    }
    const stream = chunkedStream(chunks);

    let caught: unknown;
    try {
      await collect(parseSseStream(stream, { providerName: "test-provider" }));
    } catch (e) {
      caught = e;
    }
    expect(caught).toBeInstanceOf(LLMProviderError);
    const err = caught as LLMProviderError;
    expect(err.kind).toBe("bad_request");
    expect(err.provider).toBe("test-provider");
    expect(err.message).toContain("SSE event exceeded");
  });

  test("accepts a custom cap via options", async () => {
    // Small cap, single big event — should trip early.
    const stream = streamFromString(`data: ${"x".repeat(1000)}`);
    let caught: unknown;
    try {
      await collect(parseSseStream(stream, { maxEventBytes: 256, providerName: "tiny" }));
    } catch (e) {
      caught = e;
    }
    expect(caught).toBeInstanceOf(LLMProviderError);
  });

  test("normal-sized events pass the cap check", async () => {
    // Much smaller than 1 MiB, with proper terminators.
    const stream = streamFromString(`data: ${"a".repeat(1024)}\n\n`);
    const result = await collect(parseSseStream(stream));
    expect(result.length).toBe(1);
    expect(result[0].length).toBe(1024);
  });

  test("still accepts a bare AbortSignal for backward compatibility", async () => {
    const controller = new AbortController();
    const stream = streamFromString(`data: hi\n\n`);
    const out = await collect(parseSseStream(stream, controller.signal));
    expect(out).toEqual(["hi"]);
  });
});
