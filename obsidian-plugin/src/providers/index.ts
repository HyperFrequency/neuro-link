/**
 * LLMManager — single entry point on the plugin for LLM calls.
 *
 * Responsibilities:
 *   - Lazily imports the configured provider(s) via dynamic import (keeps
 *     the bundle small — only the providers actually referenced by the
 *     user's priority list get loaded).
 *   - Honours the user-configured fallback order: on retryable errors
 *     (rate limit, network, timeout), try the next provider.
 *   - Exposes a `tool_use()` shortcut used by the new-spec dispatcher.
 */

import {
  LLMChatOptions,
  LLMChatResult,
  LLMProvider,
  LLMProviderError,
  LLMStreamChunk,
  ProviderConfig,
  ProviderId,
  ProviderModule,
} from "./base";

export interface LLMProviderSettings {
  apiKey: string;
  baseUrl: string;
  defaultModel: string;
}

export interface LLMManagerSettings {
  /** The active provider order — first entry is the primary. */
  priority: ProviderId[];
  providers: Record<ProviderId, LLMProviderSettings>;
}

export const DEFAULT_LLM_SETTINGS: LLMManagerSettings = {
  priority: ["openrouter"],
  providers: {
    openrouter: {
      apiKey: "",
      baseUrl: "https://openrouter.ai/api/v1",
      defaultModel: "anthropic/claude-sonnet-4-20250514",
    },
    anthropic: {
      apiKey: "",
      baseUrl: "https://api.anthropic.com/v1",
      defaultModel: "claude-sonnet-4-20250514",
    },
    openai: {
      apiKey: "",
      baseUrl: "https://api.openai.com/v1",
      defaultModel: "gpt-4o",
    },
    "local-llama": {
      apiKey: "",
      baseUrl: "http://localhost:8400/v1",
      defaultModel: "octen",
    },
  },
};

export class LLMManager {
  private settings: LLMManagerSettings;
  private cache = new Map<ProviderId, LLMProvider>();

  constructor(settings: LLMManagerSettings) {
    this.settings = settings;
  }

  updateSettings(settings: LLMManagerSettings): void {
    this.settings = settings;
    this.cache.clear(); // provider configs may have changed
  }

  /** Default model for the current primary provider. */
  defaultModel(): string {
    const primary = this.settings.priority[0];
    if (!primary) return "";
    return this.settings.providers[primary]?.defaultModel || "";
  }

  async chat(options: LLMChatOptions): Promise<LLMChatResult> {
    return this.run((p) => p.chat(options));
  }

  async tool_use(options: LLMChatOptions): Promise<LLMChatResult> {
    return this.run((p) => p.tool_use(options));
  }

  /**
   * Streaming never falls back mid-stream (the caller has already rendered
   * partial output); we only fall back on the initial call setup. If the
   * first provider throws before yielding any chunks, we retry with the
   * next one.
   */
  async *chatStream(options: LLMChatOptions): AsyncIterable<LLMStreamChunk> {
    const errors: Error[] = [];
    for (const id of this.settings.priority) {
      const provider = await this.getProvider(id);
      if (!provider) continue;
      let started = false;
      try {
        for await (const chunk of provider.chatStream(options)) {
          started = true;
          yield chunk;
        }
        return;
      } catch (e) {
        if (started || !isRetryable(e)) throw e;
        errors.push(e as Error);
      }
    }
    throw aggregate("No provider succeeded", errors);
  }

  private async run(call: (p: LLMProvider) => Promise<LLMChatResult>): Promise<LLMChatResult> {
    const errors: Error[] = [];
    for (const id of this.settings.priority) {
      const provider = await this.getProvider(id);
      if (!provider) continue;
      try {
        return await call(provider);
      } catch (e) {
        errors.push(e as Error);
        if (!isRetryable(e)) throw e;
      }
    }
    throw aggregate("No provider succeeded", errors);
  }

  private async getProvider(id: ProviderId): Promise<LLMProvider | null> {
    const cached = this.cache.get(id);
    if (cached) return cached;
    const cfg = this.settings.providers[id];
    if (!cfg) return null;
    // local-llama may legitimately not need an API key.
    if (id !== "local-llama" && !cfg.apiKey) return null;
    try {
      const module = await importProvider(id);
      const provider = module.create({
        apiKey: cfg.apiKey,
        baseUrl: cfg.baseUrl,
        defaultModel: cfg.defaultModel,
      });
      this.cache.set(id, provider);
      return provider;
    } catch {
      return null;
    }
  }
}

/**
 * Dynamic import keeps unused providers out of the bundle's hot path. esbuild
 * still bundles all four (they're all referenced here) but splits them into
 * their own chunks with treeShaking — and importantly, they only parse when
 * `await import(...)` is hit, so load is lazy even for the bundled case.
 */
async function importProvider(id: ProviderId): Promise<ProviderModule> {
  switch (id) {
    case "openrouter": {
      const m = await import("./openrouter");
      return m.default;
    }
    case "anthropic": {
      const m = await import("./anthropic");
      return m.default;
    }
    case "openai": {
      const m = await import("./openai");
      return m.default;
    }
    case "local-llama": {
      const m = await import("./local-llama");
      return m.default;
    }
  }
}

function isRetryable(e: unknown): boolean {
  if (e instanceof LLMProviderError) return e.retryable;
  return false;
}

function aggregate(msg: string, errors: Error[]): Error {
  if (errors.length === 0) return new Error(`${msg}: no providers configured`);
  if (errors.length === 1) return errors[0];
  const detail = errors.map((e) => `  - ${e.message}`).join("\n");
  return new Error(`${msg}:\n${detail}`);
}

export * from "./base";
