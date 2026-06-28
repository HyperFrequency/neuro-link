/**
 * Direct OpenAI provider — uses the same chat/completions shape as OpenRouter,
 * so we reuse the body builder + SSE parser from openrouter.ts.
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

const DEFAULT_BASE_URL = "https://api.openai.com/v1";

class OpenAIProvider implements LLMProvider {
  readonly id = "openai" as const;
  readonly displayName = "OpenAI";
  private readonly apiKey: string;
  private readonly baseUrl: string;

  constructor(config: ProviderConfig) {
    if (!config.apiKey) {
      throw new LLMProviderError("openai", "auth", "OpenAI API key not set");
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
    return this.chat(options);
  }

  async *chatStream(options: LLMChatOptions): AsyncIterable<LLMStreamChunk> {
    const { response, signal, cleanup } = await this.post(options, true);
    if (!response.body) {
      cleanup();
      throw new LLMProviderError("openai", "server_error", "Streaming response has no body");
    }
    yield* streamOpenAIChunks(response.body, signal, cleanup);
  }

  private async post(
    options: LLMChatOptions,
    stream: boolean
  ): Promise<{ response: Response; signal: AbortSignal | undefined; cleanup: () => void }> {
    return fetchWithTimeout(
      "openai",
      `${this.baseUrl}/chat/completions`,
      {
        method: "POST",
        headers: {
          Authorization: `Bearer ${this.apiKey}`,
          "Content-Type": "application/json",
        },
        body: JSON.stringify(buildOpenAIBody(options, stream)),
      },
      options
    );
  }
}

const mod: ProviderModule = {
  create: (config: ProviderConfig): LLMProvider => new OpenAIProvider(config),
};
export default mod;
