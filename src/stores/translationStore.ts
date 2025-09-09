import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import { invoke } from '@tauri-apps/api/core';
import { LyricLine } from './lyricsStore';

// Translation interfaces - must match backend structures exactly
export interface TranslationConfig {
  source_language: string;
  target_language: string;
  style: 'natural' | 'literal' | 'poetic' | 'rhyming' | 'custom';
  custom_prompt?: string;
  preserve_rhyme: boolean;
  preserve_meaning: boolean;
  maintain_structure: boolean;
  provider: 'github' | 'gemini';
  github_api_key?: string;
  gemini_api_key?: string;
}

export interface BackendLyricLine {
  time: number;
  text: string;
  duration?: number;
}

export interface TranslatedLyricLine extends LyricLine {
  originalText: string;
  translatedText: string;
  translationConfidence?: number;
  detectedLanguage?: string;
}

export interface TranslationResult {
  success: boolean;
  translated_lines: TranslatedLyricLine[];
  detected_language?: string;
  processing_time_ms: number;
  error?: string;
}

export interface TranslationDisplayMode {
  mode: 'original' | 'translated' | 'bilingual' | 'side-by-side';
  fontSize: {
    original: number;
    translated: number;
  };
  opacity: {
    original: number;
    translated: number;
  };
  spacing: number;
  colors: {
    original: string;
    translated: string;
  };
  animation: {
    enabled: boolean;
    type: 'fade' | 'slide' | 'scale';
    duration: number;
  };
}

export interface TranslationSettings {
  // Core translation config  
  enabled: boolean;
  autoTranslate: boolean;
  config: TranslationConfig;
  
  // Display preferences
  displayMode: TranslationDisplayMode;
  
  // Language preferences
  preferredSourceLanguages: string[];
  excludedSourceLanguages: string[];
  fallbackTargetLanguage: string;
  
  // UI preferences
  showLanguageIndicator: boolean;
  showTranslationToggle: boolean;
  showConfidenceScore: boolean;
}

interface TranslationState extends TranslationSettings {
  // Runtime state
  isTranslating: boolean;
  translationError: string | null;
  lastTranslationTime: number | null;
  
  // Actions
  setEnabled: (enabled: boolean) => void;
  setAutoTranslate: (auto: boolean) => void;
  updateConfig: (config: Partial<TranslationConfig>) => void;
  updateDisplayMode: (mode: Partial<TranslationDisplayMode>) => void;
  setPreferredSourceLanguages: (languages: string[]) => void;
  setExcludedSourceLanguages: (languages: string[]) => void;
  setFallbackTargetLanguage: (language: string) => void;
  setIsTranslating: (translating: boolean) => void;
  setTranslationError: (error: string | null) => void;
  setLastTranslationTime: (time: number) => void;
  resetToDefaults: () => void;
}

const defaultDisplayMode: TranslationDisplayMode = {
  mode: 'bilingual',
  fontSize: {
    original: 1.0,
    translated: 0.85
  },
  opacity: {
    original: 1.0,
    translated: 0.9
  },
  spacing: 8,
  colors: {
    original: 'rgb(255, 255, 255)',
    translated: 'rgb(200, 220, 255)'
  },
  animation: {
    enabled: true,
    type: 'fade',
    duration: 300
  }
};

const defaultConfig: TranslationConfig = {
  source_language: 'auto',
  target_language: 'en',
  style: 'natural',
  custom_prompt: undefined,
  preserve_rhyme: true,
  preserve_meaning: true,
  maintain_structure: true,
  provider: 'gemini', // Default to Gemini for better rate limits
  github_api_key: undefined,
  gemini_api_key: undefined
};

const defaultSettings: TranslationSettings = {
  enabled: false,
  autoTranslate: false,
  config: defaultConfig,
  displayMode: defaultDisplayMode,
  preferredSourceLanguages: [],
  excludedSourceLanguages: [],
  fallbackTargetLanguage: 'en',
  showLanguageIndicator: true,
  showTranslationToggle: true,
  showConfidenceScore: false
};

export const useTranslationStore = create<TranslationState>()(
  persist(
    (set, get) => ({
      ...defaultSettings,
      
      // Runtime state
      isTranslating: false,
      translationError: null,
      lastTranslationTime: null,

      // Actions
      setEnabled: (enabled) => 
        set((state) => ({
          enabled,
          config: { ...state.config, enabled }
        })),

      setAutoTranslate: (auto) => set({ autoTranslate: auto }),

      updateConfig: (configUpdate) =>
        set((state) => ({
          config: { ...state.config, ...configUpdate }
        })),

      updateDisplayMode: (modeUpdate) =>
        set((state) => ({
          displayMode: { ...state.displayMode, ...modeUpdate }
        })),

      setPreferredSourceLanguages: (languages) => 
        set({ preferredSourceLanguages: languages }),

      setExcludedSourceLanguages: (languages) => 
        set({ excludedSourceLanguages: languages }),

      setFallbackTargetLanguage: (language) => 
        set({ fallbackTargetLanguage: language }),

      setIsTranslating: (translating) => set({ isTranslating: translating }),

      setTranslationError: (error) => set({ translationError: error }),

      setLastTranslationTime: (time) => set({ lastTranslationTime: time }),

      resetToDefaults: () => set({
        ...defaultSettings,
        isTranslating: false,
        translationError: null,
        lastTranslationTime: null
      })
    }),
    {
      name: 'lyryc-translation-settings',
      version: 1,
      // Only persist user settings, not runtime state
      partialize: (state) => ({
        enabled: state.enabled,
        autoTranslate: state.autoTranslate,
        config: state.config,
        displayMode: state.displayMode,
        preferredSourceLanguages: state.preferredSourceLanguages,
        excludedSourceLanguages: state.excludedSourceLanguages,
        fallbackTargetLanguage: state.fallbackTargetLanguage,
        showLanguageIndicator: state.showLanguageIndicator,
        showTranslationToggle: state.showTranslationToggle,
        showConfidenceScore: state.showConfidenceScore
      })
    }
  )
);

// Utility hooks
export const useTranslationConfig = () => {
  const store = useTranslationStore();
  return {
    config: store.config,
    updateConfig: store.updateConfig,
    enabled: store.enabled,
    setEnabled: store.setEnabled
  };
};

export const useTranslationDisplay = () => {
  const store = useTranslationStore();
  return {
    displayMode: store.displayMode,
    updateDisplayMode: store.updateDisplayMode
  };
};

export const useTranslationStatus = () => {
  const store = useTranslationStore();
  return {
    isTranslating: store.isTranslating,
    error: store.translationError,
    lastTranslationTime: store.lastTranslationTime
  };
};

// Language detection helper
const detectLanguage = async (lyrics: LyricLine[]): Promise<string | null> => {
  if (!lyrics.length) return null;
  
  try {
    // Use the first few lines to detect language
    const sampleText = lyrics.slice(0, 3).map(line => line.text).join(' ');
    const result = await invoke<{detected_language: string}>('detect_language', {
      text: sampleText
    });
    
    return result.detected_language;
  } catch (error) {
    console.warn('[Translation] Language detection failed, will rely on backend detection:', error);
    return null;
  }
};

// Direct Tauri invoke functions - replacing the service layer
export const translateLyrics = async (lyrics: LyricLine[], config: TranslationConfig): Promise<TranslationResult> => {
  try {
    // Check if source language is specified and equals target language
    if (config.source_language !== 'auto' && config.source_language === config.target_language) {
      console.log(`[Translation] Skipping translation: source (${config.source_language}) equals target (${config.target_language})`);
      return {
        success: true,
        translated_lines: lyrics.map(line => ({
          ...line,
          words: line.words || [],
          originalText: line.text,
          translatedText: line.text,
          translationConfidence: 1.0,
          detectedLanguage: config.source_language
        })),
        detected_language: config.source_language,
        processing_time_ms: 0
      };
    }

    // If auto-detect is enabled, try to detect language first
    if (config.source_language === 'auto') {
      const detectedLang = await detectLanguage(lyrics);
      if (detectedLang && detectedLang === config.target_language) {
        console.log(`[Translation] Skipping translation: detected language (${detectedLang}) equals target (${config.target_language})`);
        return {
          success: true,
          translated_lines: lyrics.map(line => ({
            ...line,
            words: line.words || [],
            originalText: line.text,
            translatedText: line.text,
            translationConfidence: 1.0,
            detectedLanguage: detectedLang
          })),
          detected_language: detectedLang,
          processing_time_ms: 0
        };
      }
    }

    // Convert frontend lyrics to backend format
    const backendLyrics: BackendLyricLine[] = lyrics.map(line => ({
      time: line.time,
      text: line.text,
      duration: line.duration
    }));

    console.log(`[Translation] Calling backend with ${lyrics.length} lines`);
    
    // Call backend translation command directly
    const result = await invoke<TranslationResult>('translate_lyrics_command', {
      lyrics: backendLyrics,
      config: config
    });

    console.log(`[Translation] Backend translation ${result.success ? 'successful' : 'failed'} in ${result.processing_time_ms}ms`);
    
    return result;
  } catch (error) {
    console.error('[Translation] Backend call failed:', error);
    return {
      success: false,
      translated_lines: [],
      detected_language: undefined,
      processing_time_ms: 0,
      error: error instanceof Error ? error.message : 'Unknown translation error'
    };
  }
};

export const getSupportedLanguages = async (): Promise<[string, string][]> => {
  try {
    return await invoke<[string, string][]>('get_supported_languages');
  } catch (error) {
    console.error('[Translation] Failed to get supported languages:', error);
    return [
      ['auto', 'Auto-detect'],
      ['en', 'English'],
      ['es', 'Spanish'],
      ['fr', 'French'],
      ['de', 'German'],
    ];
  }
};

// Utility functions
export const getTranslationStyleOptions = () => {
  return {
    'natural': 'Context-Aware & Natural',
    'literal': 'Literal Translation', 
    'poetic': 'Artistic Adaptation',
    'rhyming': 'Musical & Rhyming',
    'custom': 'Custom Prompt'
  };
};