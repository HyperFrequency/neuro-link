/**
 * Local llama-server provider — targets llama.cpp's /v1/chat/completions
 * endpoint which is OpenAI-compatible. Default base URL assumes Octen on
 * :8400 (the embedding model uses the same convention); callers can point
 * it at Qwen3 on :8401 (or any other OpenAI-compatible local server) via
 * the `baseUrl` config field.
 *
 * API keys are typically not required for local servers; we accept an
 * empty apiKey and skip the Authorization header when it's absent.
 */

import {
  LLMChatOptions,
  LLMChatResult,
  LLMProvider,
  LLMProviderError,
  LLMStreamChunk,
  ProviderConfig,
  ProviderModule,
} from "./base";
import {
  buildOpenAIBody,
  fetchWithTimeout,
  normaliseResponse,
  OpenAIChatResponse,
  streamOpenAIChunks,
} from "./openrouter";

const DEFAULT_BASE_URL = "http://localhost:8400/v1";

class LocalLlamaProvider implements LLMProvider {
  readonly id = "local-llama" as const;
  readonly displayName = "Local llama-server";
  private readonly apiKey: string;
  private readonly baseUrl: string;

  constructor(config: ProviderConfig) {
    this.apiKey = config.apiKey || "";
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
    // Most llama-server builds support tool calling in GBNF mode; if the
    // underlying model doesn't, the caller gets back a content-only response,
    // which the dispatcher handles gracefully.
    return this.chat(options);
  }

  async *chatStream(options: LLMChatOptions): AsyncIterable<LLMStreamChunk> {
    const { response, signal, cleanup } = await this.post(options, true);
    if (!response.body) {
      cleanup();
      throw new LLMProviderError("local-llama", "server_error", "Streaming response has no body");
    }
    yield* streamOpenAIChunks(response.body, signal, cleanup);
  }

  private async post(
    options: LLMChatOptions,
    stream: boolean
  ): Promise<{ response: Response; signal: AbortSignal | undefined; cleanup: () => void }> {
    const headers: Record<string, string> = { "Content-Type": "application/json" };
    if (this.apiKey) headers.Authorization = `Bearer ${this.apiKey}`;
    return fetchWithTimeout(
      "local-llama",
      `${this.baseUrl}/chat/completions`,
      { method: "POST", headers, body: JSON.stringify(buildOpenAIBody(options, stream)) },
      options
    );
  }
}

const mod: ProviderModule = {
  create: (config: ProviderConfig): LLMProvider => new LocalLlamaProvider(config),
};
export default mod;
