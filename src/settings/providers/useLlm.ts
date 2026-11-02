import { useCallback, useEffect, useState } from "react";
import type { LlmPreset, LlmSettings } from "../../services/ipc/llm";
import { errorText, llm } from "../../services/ipc/llmClient";

export interface LlmState {
  presets: LlmPreset[];
  settings: LlmSettings | null;
  error: string | null;
  refresh: () => Promise<void>;
}

/** Presets and provider settings from the backend, with a manual refresh. */
export function useLlm(): LlmState {
  const [presets, setPresets] = useState<LlmPreset[]>([]);
  const [settings, setSettings] = useState<LlmSettings | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setSettings(await llm.getSettings());
      setError(null);
    } catch (e) {
      setError(errorText(e));
    }
  }, []);

  useEffect(() => {
    llm.listPresets().then(setPresets, (e) => setError(errorText(e)));
    void refresh();
  }, [refresh]);

  return { presets, settings, error, refresh };
}

export const presetOf = (presets: LlmPreset[], id: string) => presets.find((p) => p.id === id);

export function hostOf(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return url;
  }
}
