/**
 * In-memory stand-in for LlmBridge, used in a plain browser and whenever the
 * real llm_* commands are missing from the running backend. Behaves like the
 * contract says: keys are write-only and come back as a masked hint.
 * A key containing "bad" fails the connection test, to exercise the UI.
 */
import type {
  LegacyKeys,
  LlmBridge,
  LlmPreset,
  LlmProvider,
  LlmProviderInput,
  LlmRoute,
  LlmSettings,
  LlmTask,
} from "./llm";

type Bridge = typeof LlmBridge;

const presets: LlmPreset[] = [
  { id: "openai", name: "OpenAI", dialect: "openai", baseUrl: "https://api.openai.com/v1", keyRequired: true, keyUrl: "https://platform.openai.com/api-keys", suggestedModels: ["gpt-4.1-mini", "gpt-4.1", "o4-mini"] },
  { id: "anthropic", name: "Anthropic", dialect: "anthropic", baseUrl: "https://api.anthropic.com", keyRequired: true, keyUrl: "https://console.anthropic.com/settings/keys", suggestedModels: ["claude-haiku-5-5", "claude-sonnet-5-5"] },
  { id: "gemini", name: "Google Gemini", dialect: "gemini", baseUrl: "https://generativelanguage.googleapis.com", keyRequired: true, keyUrl: "https://aistudio.google.com/apikey", suggestedModels: ["gemini-2.5-flash", "gemini-2.5-pro"] },
  { id: "openrouter", name: "OpenRouter", dialect: "openai", baseUrl: "https://openrouter.ai/api/v1", keyRequired: true, keyUrl: "https://openrouter.ai/keys", suggestedModels: ["openrouter/auto"] },
  { id: "groq", name: "Groq", dialect: "openai", baseUrl: "https://api.groq.com/openai/v1", keyRequired: true, keyUrl: "https://console.groq.com/keys", suggestedModels: ["llama-3.3-70b-versatile"] },
  { id: "deepseek", name: "DeepSeek", dialect: "openai", baseUrl: "https://api.deepseek.com", keyRequired: true, keyUrl: "https://platform.deepseek.com/api_keys", suggestedModels: ["deepseek-chat"] },
  { id: "github", name: "GitHub Models", dialect: "openai", baseUrl: "https://models.github.ai/inference", keyRequired: true, keyUrl: "https://github.com/settings/tokens", suggestedModels: ["openai/gpt-4.1-mini"] },
  { id: "ollama", name: "Ollama", dialect: "openai", baseUrl: "http://localhost:11434/v1", keyRequired: false, suggestedModels: ["llama3.2", "qwen2.5"] },
  { id: "custom", name: "Custom (OpenAI-compatible)", dialect: "openai", baseUrl: "", keyRequired: false, suggestedModels: [] },
];

const hint = (key: string) => (key.length > 8 ? `${key.slice(0, 3)}…${key.slice(-4)}` : "•••");
const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));
const clone = <T>(v: T): T => structuredClone(v);

export function createFakeLlm(): Bridge {
  const providers: LlmProvider[] = [];
  const keys = new Map<string, string>();
  const routes: Partial<Record<LlmTask, LlmRoute>> = {};
  let seq = 0;

  const snapshot = (): LlmSettings => clone({ providers, routes });
  const preset = (id: string) => presets.find((p) => p.id === id);
  const find = (id: string) => {
    const p = providers.find((x) => x.id === id);
    if (!p) throw `Unknown provider: ${id}`;
    return p;
  };

  function upsert(input: LlmProviderInput): LlmProvider {
    const pre = preset(input.presetId);
    if (!pre) throw `Unknown preset: ${input.presetId}`;
    const baseUrl = input.baseUrl?.trim() || pre.baseUrl;
    if (!baseUrl) throw "A base URL is required for this provider.";
    const existing = input.id ? find(input.id) : undefined;
    const p: LlmProvider = existing ?? { id: `${pre.id}-${++seq}`, presetId: pre.id, label: "", baseUrl, hasKey: false };
    p.label = input.label?.trim() || existing?.label || pre.name;
    p.baseUrl = baseUrl;
    if (input.apiKey !== undefined) {
      if (input.apiKey === "") keys.delete(p.id);
      else keys.set(p.id, input.apiKey);
    }
    p.hasKey = keys.has(p.id);
    p.keyHint = p.hasKey ? hint(keys.get(p.id)!) : undefined;
    if (!existing) providers.push(p);
    return clone(p);
  }

  return {
    listPresets: async () => clone(presets),
    getSettings: async () => snapshot(),
    upsertProvider: async (input) => upsert(input),

    async removeProvider(id) {
      const i = providers.findIndex((p) => p.id === id);
      if (i >= 0) providers.splice(i, 1);
      keys.delete(id);
      for (const t of Object.keys(routes) as LlmTask[]) if (routes[t]?.providerId === id) delete routes[t];
    },

    async setRoute(task, route) {
      if (route) routes[task] = { ...route };
      else delete routes[task];
    },

    async listModels(providerId) {
      await wait(450);
      const p = find(providerId);
      return [...(preset(p.presetId)?.suggestedModels ?? []), "fake-model-mini"];
    },

    async test(providerId, model) {
      await wait(700);
      const p = find(providerId);
      const key = keys.get(providerId) ?? "";
      if (preset(p.presetId)?.keyRequired && !key) throw "401 Unauthorized: no API key set";
      if (key.includes("bad")) throw "401 Unauthorized: invalid API key";
      return { latencyMs: 180 + Math.round(Math.random() * 240), model, sample: "Hello! I'm ready." };
    },

    async importLegacyKeys(legacy: LegacyKeys) {
      if (legacy.geminiApiKey) upsert({ presetId: "gemini", apiKey: legacy.geminiApiKey });
      if (legacy.githubApiKey) upsert({ presetId: "github", apiKey: legacy.githubApiKey });
      return snapshot();
    },
  };
}
