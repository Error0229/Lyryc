/**
 * LEGACY COMPATIBILITY FILE
 *
 * Re-exports translation functionality from the new architecture.
 */

import { useConfigStore } from './configStore';
import { TauriBridge } from '../services/ipc/tauri-bridge';
import type {
  TranslationConfig,
  TranslationDisplayConfig,
  TranslatedLyricLine,
  TranslationResult,
  LyricLine,
} from './types';

export type {
  TranslationConfig,
  TranslatedLyricLine,
  TranslationResult,
};

export interface TranslationDisplayMode extends TranslationDisplayConfig {}
export interface BackendLyricLine {
  time: number;
  text: string;
  duration?: number;
}

// Legacy store hook with setState attached
const useTranslationStoreBase = () => {
  const translation = useConfigStore((s) => s.translation);
  const updateConfig = useConfigStore((s) => s.updateTranslationConfig);
  const updateDisplay = useConfigStore((s) => s.updateTranslationDisplay);
  const setEnabled = useConfigStore((s) => s.setTranslationEnabled);
  const setAutoTranslate = useConfigStore((s) => s.setAutoTranslate);

  return {
    // Config
    enabled: translation.enabled,
    autoTranslate: translation.autoTranslate,
    config: translation.config,
    displayMode: translation.display,
    showLanguageIndicator: translation.showLanguageIndicator,
    showTranslationToggle: true,
    showConfidenceScore: translation.showConfidenceScore,
    preferredSourceLanguages: [] as string[],
    excludedSourceLanguages: [] as string[],
    fallbackTargetLanguage: 'en',

    // Runtime state (now handled by appState, but keeping interface)
    isTranslating: false,
    translationError: null as string | null,
    lastTranslationTime: null as number | null,

    // Actions
    setEnabled,
    setAutoTranslate,
    updateConfig,
    updateDisplayMode: updateDisplay,
    setPreferredSourceLanguages: () => {},
    setExcludedSourceLanguages: () => {},
    setFallbackTargetLanguage: () => {},
    setIsTranslating: () => {},
    setTranslationError: () => {},
    setLastTranslationTime: () => {},
    resetToDefaults: () => useConfigStore.getState().resetAll(),
  };
};

// Attach setState to make it behave like a Zustand store
export const useTranslationStore = Object.assign(useTranslationStoreBase, {
  setState: (updates: Partial<{ showLanguageIndicator: boolean }>) => {
    console.warn('[translationStore] setState is deprecated, use store actions');
  },
  getState: useTranslationStoreBase,
});

// Utility hooks
export const useTranslationConfig = () => {
  const store = useTranslationStore();
  return {
    config: store.config,
    updateConfig: store.updateConfig,
    enabled: store.enabled,
    setEnabled: store.setEnabled,
  };
};

export const useTranslationDisplay = () => {
  const store = useTranslationStore();
  return {
    displayMode: store.displayMode,
    updateDisplayMode: store.updateDisplayMode,
  };
};

export const useTranslationStatus = () => {
  return {
    isTranslating: false,
    error: null,
    lastTranslationTime: null,
  };
};

// Translation function - delegates to TauriBridge
export const translateLyrics = async (
  lyrics: LyricLine[],
  config: TranslationConfig
): Promise<TranslationResult> => {
  // Check if same language
  if (config.source_language !== 'auto' && config.source_language === config.target_language) {
    return {
      success: true,
      translated_lines: lyrics.map((line) => ({
        ...line,
        words: line.words || [],
        originalText: line.text,
        translatedText: line.text,
        translationConfidence: 1.0,
        detectedLanguage: config.source_language,
      })),
      detected_language: config.source_language,
      processing_time_ms: 0,
      is_same_language: true,
    };
  }

  // Auto-detect language first
  if (config.source_language === 'auto') {
    const sampleText = lyrics.slice(0, 3).map((l) => l.text).join(' ');
    const detected = await TauriBridge.translation.detectLanguage(sampleText);
    if (detected && detected === config.target_language) {
      return {
        success: true,
        translated_lines: lyrics.map((line) => ({
          ...line,
          words: line.words || [],
          originalText: line.text,
          translatedText: line.text,
          translationConfidence: 1.0,
          detectedLanguage: detected,
        })),
        detected_language: detected,
        processing_time_ms: 0,
        is_same_language: true,
      };
    }
  }

  try {
    const result = await TauriBridge.translation.translate(lyrics, config);
    return result;
  } catch (error) {
    return {
      success: false,
      translated_lines: [],
      detected_language: undefined,
      processing_time_ms: 0,
      error: error instanceof Error ? error.message : 'Unknown error',
      is_same_language: false,
    };
  }
};

export const getSupportedLanguages = async (): Promise<[string, string][]> => {
  return TauriBridge.translation.getSupportedLanguages();
};

export const getTranslationStyleOptions = () => ({
  natural: 'Context-Aware & Natural',
  literal: 'Literal Translation',
  poetic: 'Artistic Adaptation',
  rhyming: 'Musical & Rhyming',
  custom: 'Custom Prompt',
});
