/**
 * LLM provider contract (BYOK).
 *
 * This file is the single source of truth for the frontend <-> backend LLM API.
 * The Rust side lives in src-tauri/src/llm/. Field names here are camelCase;
 * Rust structs use #[serde(rename_all = "camelCase")]. Tauri converts camelCase
 * command arguments to snake_case parameters automatically.
 *
 * Model: a handful of wire *dialects* (openai-compatible, anthropic, gemini) and
 * a data table of *presets* on top of them. A user adds provider instances from
 * presets, pastes a key, and routes each task (translation, cleaning) to a
 * provider + model. Keys are stored by the backend and never sent back in full.
 */

import { invoke } from "@tauri-apps/api/core";

export type LlmDialect = "openai" | "anthropic" | "gemini";
export type LlmTask = "translation" | "cleaning";

export interface LlmPreset {
  id: string; // e.g. "openai", "anthropic", "gemini", "openrouter", "groq", "deepseek", "github", "ollama", "custom"
  name: string;
  dialect: LlmDialect;
  baseUrl: string; // "" for custom: user must supply one
  keyRequired: boolean; // false for local servers (Ollama, LM Studio)
  keyUrl?: string; // where the user gets a key
  suggestedModels: string[];
  /** The provider's built-in web search, if any (needed to identify songs with search). */
  webSearch?: "responses" | "openrouter" | "native";
}

export interface LlmProvider {
  id: string; // instance id, stable
  presetId: string;
  label: string;
  baseUrl: string;
  hasKey: boolean;
  keyHint?: string; // masked, e.g. "sk-…a1b2"
}

export interface LlmRoute {
  providerId: string;
  model: string;
}

export interface LlmSettings {
  providers: LlmProvider[];
  routes: Partial<Record<LlmTask, LlmRoute>>;
}

export interface LlmProviderInput {
  id?: string; // omit to create a new instance
  presetId: string;
  label?: string;
  baseUrl?: string; // omit to use the preset's
  apiKey?: string; // omit = keep existing, "" = clear, otherwise set
}

export interface LlmTestResult {
  latencyMs: number;
  model: string;
  sample: string; // first few chars of the model's reply
}

export interface LegacyKeys {
  githubApiKey?: string;
  geminiApiKey?: string;
}

/**
 * Errors from any LLM-backed command (translate_lyrics_command, cleaning, llm_test)
 * are plain strings. When no provider is routed for a task the string starts with
 * this prefix so the UI can offer "Set up a provider" instead of a raw error.
 */
export const LLM_NOT_CONFIGURED = "LLM_NOT_CONFIGURED:";

export const LlmBridge = {
  listPresets: () => invoke<LlmPreset[]>("llm_list_presets"),
  getSettings: () => invoke<LlmSettings>("llm_get_settings"),
  upsertProvider: (input: LlmProviderInput) => invoke<LlmProvider>("llm_upsert_provider", { input }),
  removeProvider: (id: string) => invoke<void>("llm_remove_provider", { id }),
  setRoute: (task: LlmTask, route: LlmRoute | null) => invoke<void>("llm_set_route", { task, route }),
  listModels: (providerId: string) => invoke<string[]>("llm_list_models", { providerId }),
  test: (providerId: string, model: string) => invoke<LlmTestResult>("llm_test", { providerId, model }),
  /** One-shot migration of keys that used to live in localStorage. Idempotent. */
  importLegacyKeys: (keys: LegacyKeys) => invoke<LlmSettings>("llm_import_legacy_keys", { keys }),
};
