// SPDX-License-Identifier: MIT
//
// Deterministic, scripted LLMProvider / LLMManager fake for tests. Callers
// push a series of LLMChatResult objects; each tool_use / chat call pops the
// next one. If you want to assert what the agent sent us, inspect `calls`.
//
// Two fakes here:
//   - FakeLLMProvider — satisfies LLMProvider (chat / tool_use / chatStream)
//   - FakeLLMManager  — satisfies LLMManager's public surface (what the
//     agent loop actually consumes). We use a structural-typed fake rather
//     than extending the real class so we don't need to wire up provider
//     imports or settings plumbing.

import type {
  LLMChatOptions,
  LLMChatResult,
  LLMStreamChunk,
} from "../src/providers/base";

export interface ScriptedCall {
  opts: LLMChatOptions;
  /** The script entry returned (for cross-referencing). */
  returned: LLMChatResult;
}

export class FakeLLMManager {
  readonly calls: ScriptedCall[] = [];
  private script: LLMChatResult[] = [];
  private streamScript: LLMStreamChunk[][] = [];
  private model = "fake-model";

  /** Queue a tool_use / chat response. */
  enqueue(result: LLMChatResult): void {
    this.script.push(result);
  }

  /** Queue a stream (emitted as one async iterable). */
  enqueueStream(chunks: LLMStreamChunk[]): void {
    this.streamScript.push(chunks);
  }

  defaultModel(): string {
    return this.model;
  }

  setDefaultModel(m: string): void {
    this.model = m;
  }

  async chat(opts: LLMChatOptions): Promise<LLMChatResult> {
    return this.popCall(opts);
  }

  async tool_use(opts: LLMChatOptions): Promise<LLMChatResult> {
    return this.popCall(opts);
  }

  // eslint-disable-next-line @typescript-eslint/require-await
  async *chatStream(opts: LLMChatOptions): AsyncIterable<LLMStreamChunk> {
    // Record the call so tests can inspect it.
    this.calls.push({
      opts,
      returned: { content: "", finishReason: "stop" },
    });
    const chunks = this.streamScript.shift() ?? [
      { contentDelta: "", done: true, finishReason: "stop" },
    ];
    for (const c of chunks) {
      yield c;
    }
  }

  private popCall(opts: LLMChatOptions): LLMChatResult {
    if (this.script.length === 0) {
      throw new Error("FakeLLMManager: no scripted response available");
    }
    const returned = this.script.shift()!;
    this.calls.push({ opts, returned });
    return returned;
  }
}
