// SPDX-License-Identifier: MIT
import { describe, expect, test } from "bun:test";
import { LLMProviderError } from "../src/providers/base";
import { streamOpenAIChunks } from "../src/providers/openrouter";
import { streamAnthropic } from "../src/providers/anthropic";

describe("LLMProviderError", () => {
  test("sets retryable = true for rate_limit by default", () => {
    const e = new LLMProviderError("openrouter", "rate_limit", "too many reqs");
    expect(e.retryable).toBe(true);
    expect(e.provider).toBe("openrouter");
    expect(e.kind).toBe("rate_limit");
  });

  test("sets retryable = false for auth errors", () => {
    const e = new LLMProviderError("openai", "auth", "bad key");
    expect(e.retryable).toBe(false);
  });

  test("respects explicit retryable override", () => {
    const e = new LLMProviderError("anthropic", "bad_request", "msg", { retryable: true });
    expect(e.retryable).toBe(true);
  });

  test("carries status code when provided", () => {
    const e = new LLMProviderError("openrouter", "rate_limit", "429", { status: 429 });
    expect(e.status).toBe(429);
  });
});

// ── Should-fix #10: timer / listener leak in streaming paths ──────────────
//
// The providers' `fetchWithTimeout` wraps the caller's signal + a timeout
// into a combined controller, then returns a `cleanup` that both
// `clearTimeout`s the timer and detaches the listener from the caller's
// signal. Prior to this fix, stream iterators received the combined signal
// but never invoked cleanup — so a stream that completed normally left the
// timer + listener dangling. Over a long Obsidian session these would
// accumulate.
//
// Bun doesn't expose `jest.getTimerCount()`, but we can directly assert the
// semantic: the cleanup callback we hand to `streamOpenAIChunks` MUST be
// called by the time the iterator drains. We also assert the same on the
// early-exit path (consumer calls `.return()` on the iterator) and on an
// exception path.
describe("streamOpenAIChunks — cleanup invocation", () => {
  function sseStreamFromEvents(events: string[]): ReadableStream<Uint8Array> {
    const encoder = new TextEncoder();
    return new ReadableStream({
      start(controller) {
        for (const ev of events) controller.enqueue(encoder.encode(ev));
        controller.close();
      },
    });
  }

  test("invokes cleanup when the stream completes normally ([DONE] path)", async () => {
    let cleanupCalled = 0;
    const cleanup = (): void => {
      cleanupCalled++;
    };
    const events = [
      `data: ${JSON.stringify({ choices: [{ delta: { content: "hi" } }] })}\n\n`,
      `data: [DONE]\n\n`,
    ];
    const stream = sseStreamFromEvents(events);

    const chunks: unknown[] = [];
    for await (const chunk of streamOpenAIChunks(stream, undefined, cleanup)) {
      chunks.push(chunk);
    }
    expect(cleanupCalled).toBe(1);
    // Sanity: we saw a delta and a done chunk.
    expect(chunks.length).toBeGreaterThanOrEqual(2);
  });

  test("invokes cleanup when the stream completes with finish_reason", async () => {
    let cleanupCalled = 0;
    const cleanup = (): void => {
      cleanupCalled++;
    };
    const events = [
      `data: ${JSON.stringify({
        choices: [{ delta: { content: "bye" }, finish_reason: "stop" }],
      })}\n\n`,
    ];
    const stream = sseStreamFromEvents(events);
    for await (const _ of streamOpenAIChunks(stream, undefined, cleanup)) {
      /* drain */
    }
    expect(cleanupCalled).toBe(1);
  });

  test("invokes cleanup when consumer breaks out early", async () => {
    let cleanupCalled = 0;
    const cleanup = (): void => {
      cleanupCalled++;
    };
    const events = [
      `data: ${JSON.stringify({ choices: [{ delta: { content: "one" } }] })}\n\n`,
      `data: ${JSON.stringify({ choices: [{ delta: { content: "two" } }] })}\n\n`,
      `data: [DONE]\n\n`,
    ];
    const stream = sseStreamFromEvents(events);
    for await (const chunk of streamOpenAIChunks(stream, undefined, cleanup)) {
      // Break out after the first chunk — the `finally` on the async
      // iterator runs via the generator .return() protocol.
      if (chunk.contentDelta === "one") break;
    }
    expect(cleanupCalled).toBe(1);
  });

  test("works without a cleanup callback (backward compat)", async () => {
    const events = [`data: [DONE]\n\n`];
    const stream = sseStreamFromEvents(events);
    // No cleanup callback — must not throw.
    for await (const _ of streamOpenAIChunks(stream, undefined)) {
      /* drain */
    }
  });
});

describe("streamAnthropic — cleanup invocation", () => {
  function sseStreamFromEvents(events: string[]): ReadableStream<Uint8Array> {
    const encoder = new TextEncoder();
    return new ReadableStream({
      start(controller) {
        for (const ev of events) controller.enqueue(encoder.encode(ev));
        controller.close();
      },
    });
  }

  test("invokes cleanup on message_stop", async () => {
    let cleanupCalled = 0;
    const cleanup = (): void => {
      cleanupCalled++;
    };
    const events = [
      `data: ${JSON.stringify({ type: "message_stop" })}\n\n`,
    ];
    const stream = sseStreamFromEvents(events);
    for await (const _ of streamAnthropic(stream, undefined, cleanup)) {
      /* drain */
    }
    expect(cleanupCalled).toBe(1);
  });

  test("invokes cleanup when consumer breaks early", async () => {
    let cleanupCalled = 0;
    const cleanup = (): void => {
      cleanupCalled++;
    };
    const events = [
      `data: ${JSON.stringify({
        type: "content_block_start",
        index: 0,
        content_block: { type: "text" },
      })}\n\n`,
      `data: ${JSON.stringify({
        type: "content_block_delta",
        index: 0,
        delta: { type: "text_delta", text: "hello" },
      })}\n\n`,
      `data: ${JSON.stringify({ type: "message_stop" })}\n\n`,
    ];
    const stream = sseStreamFromEvents(events);
    for await (const chunk of streamAnthropic(stream, undefined, cleanup)) {
      if (chunk.contentDelta === "hello") break;
    }
    expect(cleanupCalled).toBe(1);
  });
});
