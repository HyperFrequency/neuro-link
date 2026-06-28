/**
 * LLM Provider abstraction for the neuro-link Obsidian plugin.
 *
 * Providers implement a minimal interface covering:
 *   - non-streaming chat (used by the new-spec dispatcher)
 *   - streaming chat (used by the future @neuro chat panel)
 *   - tool_use / function calling (used by the future @neuro agent mode)
 *
 * Error shapes are normalised into LLMProviderError so callers can treat
 * rate limits, auth failures, and transient network errors uniformly.
 *
 * Intentional non-goals (keep the interface narrow):
 *   - multi-turn session state (callers own the message list)
 *   - multi-modal inputs (text only for now)
 *   - vendor-specific extensions (caching, etc.) — caller passes through
 *     as provider-specific options via the `extra` bag.
 */

export type LLMRole = "system" | "user" | "assistant" | "tool";

export interface LLMMessage {
  role: LLMRole;
  content: string;
  /** Tool call results: present on `role === "tool"` messages. */
  tool_call_id?: string;
  /** Tool calls the assistant wants to make (present on assistant turns). */
  tool_calls?: LLMToolCall[];
  /** Free-form name (assistant name, tool name). */
  name?: string;
}

export interface LLMToolDefinition {
  name: string;
  description: string;
  /** JSON Schema for the tool's parameters. */
  parameters: Record<string, unknown>;
}

export interface LLMToolCall {
  id: string;
  name: string;
  /** JSON-encoded arguments object. Providers vary on whether they ship parsed or raw; we ship raw JSON. */
  arguments: string;
}

export interface LLMChatOptions {
  model: string;
  messages: LLMMessage[];
  maxTokens?: number;
  temperature?: number;
  /** Tools advertised to the model for this turn. */
  tools?: LLMToolDefinition[];
  /** Abort signal — providers MUST cancel the underlying fetch on abort. */
  signal?: AbortSignal;
  /** Per-call timeout in ms. If set, providers enforce it in addition to `signal`. */
  timeoutMs?: number;
  /** Provider-specific escape hatch. */
  extra?: Record<string, unknown>;
}

export interface LLMChatResult {
  /** Plain-text content. Empty string if the model only produced tool_calls. */
  content: string;
  tool_calls?: LLMToolCall[];
  /** Model-reported stop reason, normalised. */
  finishReason?: "stop" | "length" | "tool_calls" | "error" | "other";
  usage?: {
    inputTokens?: number;
    outputTokens?: number;
  };
  /** The raw payload — useful for debugging and provider-specific postprocessing. */
  raw?: unknown;
}

export interface LLMStreamChunk {
  /** Text delta appended to the accumulating content. */
  contentDelta?: string;
  /** Newly-formed tool call (full, not a partial). */
  toolCall?: LLMToolCall;
  /** Terminal chunk. */
  done?: boolean;
  finishReason?: LLMChatResult["finishReason"];
  usage?: LLMChatResult["usage"];
}

/**
 * Standard error shape. Providers MUST throw this (not raw Error) for
 * known failure modes. Unknown failures are wrapped with kind = "unknown".
 */
export class LLMProviderError extends Error {
  readonly kind: LLMErrorKind;
  readonly status?: number;
  readonly provider: string;
  readonly retryable: boolean;

  constructor(
    provider: string,
    kind: LLMErrorKind,
    message: string,
    opts: { status?: number; retryable?: boolean; cause?: unknown } = {}
  ) {
    super(message);
    this.name = "LLMProviderError";
    this.provider = provider;
    this.kind = kind;
    this.status = opts.status;
    this.retryable = opts.retryable ?? defaultRetryable(kind);
    if (opts.cause !== undefined) {
      (this as { cause?: unknown }).cause = opts.cause;
    }
  }
}

export type LLMErrorKind =
  | "auth" // 401/403
  | "rate_limit" // 429
  | "bad_request" // 4xx client error
  | "server_error" // 5xx
  | "timeout"
  | "aborted"
  | "network"
  | "tool_schema"
  | "unknown";

function defaultRetryable(kind: LLMErrorKind): boolean {
  return kind === "rate_limit" || kind === "server_error" || kind === "timeout" || kind === "network";
}

/**
 * The main contract. Implementations live in sibling files.
 *
 * All methods are async; `chatStream` returns an async iterator so callers
 * can consume with `for await (const chunk of ...)`.
 */
export interface LLMProvider {
  readonly id: ProviderId;
  readonly displayName: string;

  chat(options: LLMChatOptions): Promise<LLMChatResult>;

  chatStream(options: LLMChatOptions): AsyncIterable<LLMStreamChunk>;

  /**
   * Single-turn tool use. The provider sends the tool definitions and
   * returns either a final answer (no tool_calls) or the list of tool
   * invocations the caller must perform. Callers loop themselves; this
   * method does not.
   */
  tool_use(options: LLMChatOptions): Promise<LLMChatResult>;
}

export type ProviderId = "openrouter" | "anthropic" | "openai" | "local-llama";

export interface ProviderConfig {
  apiKey: string;
  baseUrl?: string;
  defaultModel?: string;
}

/**
 * A thin factory-by-id that LLMManager uses to lazily import providers.
 * Keeps bundle size down — only the configured provider's code is loaded.
 */
export interface ProviderModule {
  create(config: ProviderConfig): LLMProvider;
}
