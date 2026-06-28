/**
 * OpenRouter provider — default. OpenAI-chat-completions-compatible shape,
 * hits https://openrouter.ai/api/v1/chat/completions.
 *
 * Sibling of openai.ts — differs mainly in base URL + the
 * HTTP-Referer / X-Title headers OpenRouter recommends.
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

const DEFAULT_BASE_URL = "https://openrouter.ai/api/v1";
const REFERER = "https://github.com/HyperFrequency/neuro-link";
const CLIENT_TITLE = "NLR Obsidian Plugin";

class OpenRouterProvider implements LLMProvider {
  readonly id = "openrouter" as const;
  readonly displayName = "OpenRouter";
  private readonly apiKey: string;
  private readonly baseUrl: string;

  constructor(config: ProviderConfig) {
    if (!config.apiKey) {
      throw new LLMProviderError("openrouter", "auth", "OpenRouter API key not set");
    }
    this.apiKey = config.apiKey;
    this.baseUrl = (config.baseUrl || DEFAULT_BASE_URL).replace(/\/$/, "");
  }

  async chat(options: LLMChatOptions): Promise<LLMChatResult> {
    const { response, signal, cleanup } = await this.post(options, false);
    try {
      const json = (await response.json()) as OpenAIChatResponse;
      return normaliseResponse(json, signal);
    } finally {
      cleanup();
    }
  }

  async tool_use(options: LLMChatOptions): Promise<LLMChatResult> {
    // OpenRouter follows the OpenAI tools schema; same endpoint.
    return this.chat(options);
  }

  async *chatStream(options: LLMChatOptions): AsyncIterable<LLMStreamChunk> {
    const { response, signal, cleanup } = await this.post(options, true);
    if (!response.body) {
      cleanup();
      throw new LLMProviderError("openrouter", "server_error", "Streaming response has no body");
    }
    yield* streamOpenAIChunks(response.body, signal, cleanup);
  }

  private async post(
    options: LLMChatOptions,
    stream: boolean
  ): Promise<{ response: Response; signal: AbortSignal | undefined; cleanup: () => void }> {
    const body = buildOpenAIBody(options, stream);
    const headers: Record<string, string> = {
      Authorization: `Bearer ${this.apiKey}`,
      "Content-Type": "application/json",
      "HTTP-Referer": REFERER,
      "X-Title": CLIENT_TITLE,
    };
    return fetchWithTimeout(
      "openrouter",
      `${this.baseUrl}/chat/completions`,
      { method: "POST", headers, body: JSON.stringify(body) },
      options
    );
  }
}

// ── shared OpenAI-shaped helpers (reused by openai.ts and local-llama.ts) ──

export interface OpenAIChatResponse {
  choices: Array<{
    message: {
      role: string;
      content: string | null;
      tool_calls?: Array<{
        id: string;
        type: string;
        function: { name: string; arguments: string };
      }>;
    };
    finish_reason?: string;
  }>;
  usage?: { prompt_tokens?: number; completion_tokens?: number };
}

export function buildOpenAIBody(options: LLMChatOptions, stream: boolean): Record<string, unknown> {
  const messages = options.messages.map(toOpenAIMessage);
  const body: Record<string, unknown> = { model: options.model, messages, stream };
  if (options.maxTokens !== undefined) body.max_tokens = options.maxTokens;
  if (options.temperature !== undefined) body.temperature = options.temperature;
  if (options.tools && options.tools.length > 0) {
    body.tools = options.tools.map((t) => ({
      type: "function",
      function: { name: t.name, description: t.description, parameters: t.parameters },
    }));
  }
  if (options.extra) {
    for (const [k, v] of Object.entries(options.extra)) body[k] = v;
  }
  return body;
}

function toOpenAIMessage(m: LLMMessage): Record<string, unknown> {
  // Tool-result messages use `role: "tool"` + `tool_call_id`.
  if (m.role === "tool") {
    return { role: "tool", content: m.content, tool_call_id: m.tool_call_id };
  }
  const msg: Record<string, unknown> = { role: m.role, content: m.content };
  if (m.name) msg.name = m.name;
  if (m.tool_calls && m.tool_calls.length > 0) {
    msg.tool_calls = m.tool_calls.map((tc) => ({
      id: tc.id,
      type: "function",
      function: { name: tc.name, arguments: tc.arguments },
    }));
  }
  return msg;
}

export function normaliseResponse(
  raw: OpenAIChatResponse,
  _signal: AbortSignal | undefined
): LLMChatResult {
  const choice = raw.choices?.[0];
  if (!choice) {
    return { content: "", finishReason: "error", raw };
  }
  const content = choice.message?.content || "";
  const toolCalls: LLMToolCall[] | undefined = choice.message?.tool_calls?.map((tc) => ({
    id: tc.id,
    name: tc.function.name,
    arguments: tc.function.arguments,
  }));
  return {
    content,
    tool_calls: toolCalls && toolCalls.length > 0 ? toolCalls : undefined,
    finishReason: mapFinishReason(choice.finish_reason),
    usage: raw.usage
      ? { inputTokens: raw.usage.prompt_tokens, outputTokens: raw.usage.completion_tokens }
      : undefined,
    raw,
  };
}

function mapFinishReason(reason?: string): LLMChatResult["finishReason"] {
  switch (reason) {
    case "stop":
      return "stop";
    case "length":
      return "length";
    case "tool_calls":
    case "function_call":
      return "tool_calls";
    case undefined:
      return undefined;
    default:
      return "other";
  }
}

export async function fetchWithTimeout(
  provider: string,
  url: string,
  init: RequestInit,
  options: LLMChatOptions
): Promise<{ response: Response; signal: AbortSignal | undefined; cleanup: () => void }> {
  const { combinedSignal, cleanup } = combineSignals(options.signal, options.timeoutMs);
  let response: Response;
  try {
    response = await fetch(url, { ...init, signal: combinedSignal });
  } catch (e) {
    cleanup();
    throw wrapFetchError(provider, e);
  }
  if (!response.ok) {
    cleanup();
    const bodyText = await safeReadText(response);
    throw new LLMProviderError(provider, statusToKind(response.status), `${provider} ${response.status}: ${bodyText}`, {
      status: response.status,
    });
  }
  // Caller must invoke cleanup: `chat()` wraps the json() read in try/finally;
  // stream iterators accept cleanup as a third parameter and call it in the
  // finally block that runs on EOF, error, or consumer-side .return().
  // See PR #26 adversarial review, should-fix #10.
  return { response, signal: combinedSignal, cleanup };
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

function wrapFetchError(provider: string, e: unknown): LLMProviderError {
  const err = e as { name?: string; message?: string };
  if (err.name === "AbortError") {
    return new LLMProviderError(provider, "aborted", err.message || "aborted");
  }
  return new LLMProviderError(provider, "network", err.message || "network error", { cause: e });
}

export async function* streamOpenAIChunks(
  body: ReadableStream<Uint8Array>,
  signal: AbortSignal | undefined,
  cleanup?: () => void
): AsyncIterable<LLMStreamChunk> {
  // Reconstruct per-index tool calls across deltas.
  const toolCallsByIndex = new Map<number, { id?: string; name?: string; args: string }>();
  let emittedToolCallIndices = new Set<number>();

  try {
    for await (const data of parseSseStream(body, { signal, providerName: "openrouter" })) {
      if (data === "[DONE]") {
        // Emit any finalised tool calls that haven't been emitted (some models
        // terminate the stream without a finish_reason event).
        for (const [idx, call] of toolCallsByIndex.entries()) {
          if (!emittedToolCallIndices.has(idx) && call.id && call.name) {
            yield { toolCall: { id: call.id, name: call.name, arguments: call.args || "{}" } };
          }
        }
        yield { done: true };
        return;
      }
      let parsed: OpenAIStreamChunk;
      try {
        parsed = JSON.parse(data) as OpenAIStreamChunk;
      } catch {
        continue; // skip malformed chunks
      }
      const choice = parsed.choices?.[0];
      if (!choice) continue;
      const delta = choice.delta || {};
      if (typeof delta.content === "string" && delta.content.length > 0) {
        yield { contentDelta: delta.content };
      }
      if (delta.tool_calls) {
        for (const tc of delta.tool_calls) {
          const idx = tc.index ?? 0;
          let entry = toolCallsByIndex.get(idx);
          if (!entry) {
            entry = { args: "" };
            toolCallsByIndex.set(idx, entry);
          }
          if (tc.id) entry.id = tc.id;
          if (tc.function?.name) entry.name = tc.function.name;
          if (tc.function?.arguments) entry.args += tc.function.arguments;
        }
      }
      if (choice.finish_reason) {
        // Flush finalised tool calls before the terminal chunk.
        for (const [idx, call] of toolCallsByIndex.entries()) {
          if (!emittedToolCallIndices.has(idx) && call.id && call.name) {
            yield { toolCall: { id: call.id, name: call.name, arguments: call.args || "{}" } };
            emittedToolCallIndices.add(idx);
          }
        }
        yield {
          done: true,
          finishReason: mapFinishReason(choice.finish_reason),
          usage: parsed.usage
            ? { inputTokens: parsed.usage.prompt_tokens, outputTokens: parsed.usage.completion_tokens }
            : undefined,
        };
        return;
      }
    }
  } finally {
    // Fires on EOF, thrown errors, and consumer-side .return()/.throw() —
    // closes the timeout timer and detaches the abort-signal listener.
    if (cleanup) cleanup();
  }
}

interface OpenAIStreamChunk {
  choices?: Array<{
    delta?: {
      content?: string;
      tool_calls?: Array<{
        index?: number;
        id?: string;
        function?: { name?: string; arguments?: string };
      }>;
    };
    finish_reason?: string;
  }>;
  usage?: { prompt_tokens?: number; completion_tokens?: number };
}

const mod: ProviderModule = {
  create: (config: ProviderConfig): LLMProvider => new OpenRouterProvider(config),
};
export default mod;
