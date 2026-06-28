/**
 * Direct Anthropic provider — uses the /v1/messages endpoint.
 *
 * Differs materially from OpenAI: system prompt is a top-level field, not a
 * role; tool calls are "tool_use" content blocks; tool results are "tool_result"
 * content blocks on user turns. We translate in both directions.
 */

import {
  LLMChatOptions,
  LLMChatResult,
  LLMMessage,
  LLMProvider,
  LLMProviderError,
  LLMStreamChunk,
  LLMToolCall,
  ProviderConfig,
  ProviderModule,
} from "./base";
import { parseSseStream } from "./sse";

const DEFAULT_BASE_URL = "https://api.anthropic.com/v1";
const ANTHROPIC_VERSION = "2023-06-01";

class AnthropicProvider implements LLMProvider {
  readonly id = "anthropic" as const;
  readonly displayName = "Anthropic";
  private readonly apiKey: string;
  private readonly baseUrl: string;

  constructor(config: ProviderConfig) {
    if (!config.apiKey) {
      throw new LLMProviderError("anthropic", "auth", "Anthropic API key not set");
    }
    this.apiKey = config.apiKey;
    this.baseUrl = (config.baseUrl || DEFAULT_BASE_URL).replace(/\/$/, "");
  }

  async chat(options: LLMChatOptions): Promise<LLMChatResult> {
    const { response, cleanup } = await this.post(options, false);
    try {
      const json = (await response.json()) as AnthropicMessageResponse;
      return normaliseAnthropic(json);
    } finally {
      cleanup();
    }
  }

  async tool_use(options: LLMChatOptions): Promise<LLMChatResult> {
    return this.chat(options);
  }

  async *chatStream(options: LLMChatOptions): AsyncIterable<LLMStreamChunk> {
    const { response, signal, cleanup } = await this.post(options, true);
    if (!response.body) {
      cleanup();
      throw new LLMProviderError("anthropic", "server_error", "Streaming response has no body");
    }
    yield* streamAnthropic(response.body, signal, cleanup);
  }

  private async post(
    options: LLMChatOptions,
    stream: boolean
  ): Promise<{ response: Response; signal: AbortSignal | undefined; cleanup: () => void }> {
    const { system, messages } = splitSystemAndTurns(options.messages);
    const body: Record<string, unknown> = {
      model: options.model,
      messages: messages.map(toAnthropicMessage),
      max_tokens: options.maxTokens ?? 4096,
      stream,
    };
    if (system) body.system = system;
    if (options.temperature !== undefined) body.temperature = options.temperature;
    if (options.tools && options.tools.length > 0) {
      body.tools = options.tools.map((t) => ({
        name: t.name,
        description: t.description,
        input_schema: t.parameters,
      }));
    }
    if (options.extra) {
      for (const [k, v] of Object.entries(options.extra)) body[k] = v;
    }

    const { combinedSignal, cleanup } = combineSignals(options.signal, options.timeoutMs);
    let response: Response;
    try {
      response = await fetch(`${this.baseUrl}/messages`, {
        method: "POST",
        headers: {
          "x-api-key": this.apiKey,
          "anthropic-version": ANTHROPIC_VERSION,
          "Content-Type": "application/json",
        },
        body: JSON.stringify(body),
        signal: combinedSignal,
      });
    } catch (e) {
      cleanup();
      const err = e as { name?: string; message?: string };
      if (err.name === "AbortError") {
        throw new LLMProviderError("anthropic", "aborted", err.message || "aborted");
      }
      throw new LLMProviderError("anthropic", "network", err.message || "network error", { cause: e });
    }
    if (!response.ok) {
      cleanup();
      const text = await safeReadText(response);
      throw new LLMProviderError(
        "anthropic",
        statusToKind(response.status),
        `anthropic ${response.status}: ${text}`,
        { status: response.status }
      );
    }
    return { response, signal: combinedSignal, cleanup };
  }
}

function splitSystemAndTurns(messages: LLMMessage[]): { system: string | undefined; messages: LLMMessage[] } {
  const sys: string[] = [];
  const turns: LLMMessage[] = [];
  for (const m of messages) {
    if (m.role === "system") sys.push(m.content);
    else turns.push(m);
  }
  return { system: sys.length > 0 ? sys.join("\n\n") : undefined, messages: turns };
}

function toAnthropicMessage(m: LLMMessage): Record<string, unknown> {
  // Tool results arrive as role: "tool"; Anthropic wants them as user messages
  // with a tool_result content block.
  if (m.role === "tool") {
    return {
      role: "user",
      content: [
        {
          type: "tool_result",
          tool_use_id: m.tool_call_id,
          content: m.content,
        },
      ],
    };
  }
  // Assistant messages with tool_calls become mixed content blocks.
  if (m.role === "assistant" && m.tool_calls && m.tool_calls.length > 0) {
    const blocks: Array<Record<string, unknown>> = [];
    if (m.content) blocks.push({ type: "text", text: m.content });
    for (const tc of m.tool_calls) {
      let input: unknown = {};
      try {
        input = JSON.parse(tc.arguments);
      } catch {
        input = {};
      }
      blocks.push({ type: "tool_use", id: tc.id, name: tc.name, input });
    }
    return { role: "assistant", content: blocks };
  }
  // Simple user/assistant turns — plain string content is fine.
  return { role: m.role, content: m.content };
}

interface AnthropicMessageResponse {
  id: string;
  type: string;
  role: string;
  content: Array<
    | { type: "text"; text: string }
    | { type: "tool_use"; id: string; name: string; input: unknown }
  >;
  stop_reason?: string;
  usage?: { input_tokens?: number; output_tokens?: number };
}

function normaliseAnthropic(raw: AnthropicMessageResponse): LLMChatResult {
  let content = "";
  const toolCalls: LLMToolCall[] = [];
  for (const block of raw.content || []) {
    if (block.type === "text") {
      content += block.text;
    } else if (block.type === "tool_use") {
      toolCalls.push({
        id: block.id,
        name: block.name,
        arguments: JSON.stringify(block.input ?? {}),
      });
    }
  }
  return {
    content,
    tool_calls: toolCalls.length > 0 ? toolCalls : undefined,
    finishReason: mapStopReason(raw.stop_reason),
    usage: raw.usage
      ? { inputTokens: raw.usage.input_tokens, outputTokens: raw.usage.output_tokens }
      : undefined,
    raw,
  };
}

function mapStopReason(r?: string): LLMChatResult["finishReason"] {
  switch (r) {
    case "end_turn":
    case "stop_sequence":
      return "stop";
    case "max_tokens":
      return "length";
    case "tool_use":
      return "tool_calls";
    case undefined:
      return undefined;
    default:
      return "other";
  }
}

export async function* streamAnthropic(
  body: ReadableStream<Uint8Array>,
  signal: AbortSignal | undefined,
  cleanup?: () => void
): AsyncIterable<LLMStreamChunk> {
  // Anthropic streaming: events like content_block_start, content_block_delta,
  // content_block_stop, message_delta, message_stop. Each event has `type`
  // and relevant fields. We track per-index content blocks to reconstruct
  // tool_use calls and stream text deltas.

  interface ToolBuild {
    id?: string;
    name?: string;
    input: string;
  }
  const blocks = new Map<number, { kind: "text" | "tool_use"; tool?: ToolBuild }>();

  try {
    for await (const data of parseSseStream(body, { signal, providerName: "anthropic" })) {
      let evt: AnthropicStreamEvent;
      try {
        evt = JSON.parse(data) as AnthropicStreamEvent;
      } catch {
        continue;
      }

      if (evt.type === "content_block_start") {
        const idx = evt.index ?? 0;
        const block = evt.content_block;
        if (!block) continue;
        if (block.type === "text") {
          blocks.set(idx, { kind: "text" });
        } else if (block.type === "tool_use") {
          blocks.set(idx, {
            kind: "tool_use",
            tool: { id: block.id, name: block.name, input: "" },
          });
        }
      } else if (evt.type === "content_block_delta") {
        const idx = evt.index ?? 0;
        const delta = evt.delta;
        if (!delta) continue;
        const entry = blocks.get(idx);
        if (!entry) continue;
        if (entry.kind === "text" && delta.type === "text_delta" && delta.text) {
          yield { contentDelta: delta.text };
        } else if (entry.kind === "tool_use" && delta.type === "input_json_delta" && delta.partial_json) {
          entry.tool!.input += delta.partial_json;
        }
      } else if (evt.type === "content_block_stop") {
        const idx = evt.index ?? 0;
        const entry = blocks.get(idx);
        if (entry?.kind === "tool_use" && entry.tool?.id && entry.tool?.name) {
          yield {
            toolCall: {
              id: entry.tool.id,
              name: entry.tool.name,
              arguments: entry.tool.input || "{}",
            },
          };
        }
      } else if (evt.type === "message_delta") {
        // contains stop_reason + usage
        if (evt.delta?.stop_reason || evt.usage) {
          yield {
            done: true,
            finishReason: mapStopReason(evt.delta?.stop_reason),
            usage: evt.usage
              ? { inputTokens: evt.usage.input_tokens, outputTokens: evt.usage.output_tokens }
              : undefined,
          };
          return;
        }
      } else if (evt.type === "message_stop") {
        yield { done: true };
        return;
      }
    }
  } finally {
    // Clears the combineSignals timeout timer and detaches the abort-signal
    // listener — even if the consumer breaks out of the for-await early, or
    // the server drops mid-stream. See PR #26 review, should-fix #10.
    if (cleanup) cleanup();
  }
}

interface AnthropicStreamEvent {
  type: string;
  index?: number;
  content_block?: {
    type: string;
    id?: string;
    name?: string;
  };
  delta?: {
    type?: string;
    text?: string;
    partial_json?: string;
    stop_reason?: string;
  };
  usage?: { input_tokens?: number; output_tokens?: number };
}

function combineSignals(
  caller: AbortSignal | undefined,
  timeoutMs: number | undefined
): { combinedSignal: AbortSignal; cleanup: () => void } {
  const controller = new AbortController();
  let timer: ReturnType<typeof setTimeout> | undefined;
  const onAbort = (): void => controller.abort();
  if (caller) {
    if (caller.aborted) controller.abort();
    else caller.addEventListener("abort", onAbort, { once: true });
  }
  if (timeoutMs !== undefined && timeoutMs > 0) {
    timer = setTimeout(() => controller.abort(), timeoutMs);
  }
  const cleanup = (): void => {
    if (timer) clearTimeout(timer);
    if (caller) caller.removeEventListener("abort", onAbort);
  };
  return { combinedSignal: controller.signal, cleanup };
}

async function safeReadText(response: Response): Promise<string> {
  try {
    return await response.text();
  } catch {
    return `<no body: ${response.statusText}>`;
  }
}

function statusToKind(status: number): "auth" | "rate_limit" | "bad_request" | "server_error" {
  if (status === 401 || status === 403) return "auth";
  if (status === 429) return "rate_limit";
  if (status >= 500) return "server_error";
  return "bad_request";
}

const mod: ProviderModule = {
  create: (config: ProviderConfig): LLMProvider => new AnthropicProvider(config),
};
export default mod;
