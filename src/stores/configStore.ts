/**
 * ConfigStore - All persisted user preferences in one place
 *
 * Consolidates: translationConfig, lyricsStyle, hiragana, offset, viewMode, theme
 * Each section is a "slice" but lives in the same store for single import.
 */

import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import type {
  TranslationConfig,
  TranslationDisplayConfig,
  LyricsStyle,
  HiraganaSettings,
  Theme,
  ViewMode,
} from './types';
import { DEFAULT_THEMES, applyThemeToDocument } from './defaults/themes';

// =============================================================================
// Default Values
// =============================================================================

const DEFAULT_TRANSLATION_CONFIG: TranslationConfig = {
  source_language: 'auto',
  target_language: 'en',
  style: 'natural',
  preserve_rhyme: true,
  preserve_meaning: true,
  maintain_structure: true,
  provider: 'gemini',
};

const DEFAULT_TRANSLATION_DISPLAY: TranslationDisplayConfig = {
  mode: 'bilingual',
  fontSize: { original: 1.0, translated: 0.85 },
  opacity: { original: 1.0, translated: 0.9 },
  spacing: 8,
  colors: { original: 'rgb(255, 255, 255)', translated: 'rgb(200, 220, 255)' },
  animation: { enabled: true, type: 'fade', duration: 300 },
};

const DEFAULT_LYRICS_STYLE: LyricsStyle = {
  fontFamily: 'Inter, system-ui, sans-serif',
  fontSize: 2.5,
  fontWeight: 'normal',
  textColor: '#ffffff',
  highlightColor: '#ffffff',
  pastWordColor: '#93c5fd',
  futureWordColor: '#ffffff60',
  backgroundColor: '#000000',
  backgroundOpacity: 0,
  backgroundBlur: 0,
  textShadow: true,
  textShadowColor: '#000000',
  textShadowBlur: 20,
  textGlow: false,
  textGlowColor: '#ffffff',
  lineHeight: 1.5,
  letterSpacing: 0.05,
  textAlign: 'center',
  animationIntensity: 50,
};

const DEFAULT_HIRAGANA: HiraganaSettings = {
  enabled: false,
  displayMode: 'ruby',
  rubyPosition: 'top',
  opacity: 0.8,
  fontSize: 0.7,
  color: '#a0a0a0',
  spacing: 4,
};

// =============================================================================
// Store Interface
// =============================================================================

interface ConfigState {
  // Translation
  translation: {
    enabled: boolean;
    autoTranslate: boolean;
    config: TranslationConfig;
    display: TranslationDisplayConfig;
    showLanguageIndicator: boolean;
    showConfidenceScore: boolean;
  };

  // Display
  lyricsStyle: LyricsStyle;
  hiragana: HiraganaSettings;
  viewMode: ViewMode;

  // Theme
  themeId: string;

  // Offsets
  trackOffsets: Record<string, number>;
  globalOffset: number;

  // Actions - Translation
  setTranslationEnabled: (enabled: boolean) => void;
  setAutoTranslate: (auto: boolean) => void;
  updateTranslationConfig: (config: Partial<TranslationConfig>) => void;
  updateTranslationDisplay: (display: Partial<TranslationDisplayConfig>) => void;

  // Actions - Display
  updateLyricsStyle: (style: Partial<LyricsStyle>) => void;
  updateHiragana: (settings: Partial<HiraganaSettings>) => void;
  setViewMode: (mode: ViewMode) => void;
  applyStylePreset: (presetName: string) => void;

  // Actions - Theme
  setTheme: (themeId: string) => void;
  getTheme: () => Theme;

  // Actions - Offsets
  setTrackOffset: (artist: string, title: string, offset: number) => void;
  getTrackOffset: (artist: string, title: string) => number;
  setGlobalOffset: (offset: number) => void;
  getTotalOffset: (artist: string, title: string) => number;
  clearAllOffsets: () => void;

  // Actions - Reset
  resetAll: () => void;
}

// =============================================================================
// Style Presets
// =============================================================================

const STYLE_PRESETS: Record<string, Partial<LyricsStyle>> = {
  'Clean & Simple': {
    fontSize: 2.0,
    fontWeight: 'normal',
    textShadow: false,
    animationIntensity: 20,
  },
  'Bold & Dynamic': {
    fontSize: 3.0,
    fontWeight: 'bold',
    textShadow: true,
    textShadowBlur: 30,
    animationIntensity: 80,
    textGlow: true,
    textGlowColor: '#3b82f6',
  },
  'Karaoke Style': {
    fontSize: 2.8,
    fontWeight: 'semibold',
    highlightColor: '#ffd700',
    pastWordColor: '#ffeb3b',
    textShadow: true,
    textShadowBlur: 15,
    animationIntensity: 70,
    backgroundColor: '#1a1a1a',
    backgroundOpacity: 30,
    backgroundBlur: 5,
  },
  'Neon Glow': {
    fontSize: 2.2,
    fontFamily: 'Orbitron, monospace',
    fontWeight: 'medium',
    textColor: '#00f5ff',
    highlightColor: '#39ff14',
    pastWordColor: '#ff0080',
    textGlow: true,
    textGlowColor: '#00f5ff',
    textShadow: false,
    animationIntensity: 90,
    backgroundColor: '#0a0a0a',
    backgroundOpacity: 50,
  },
  'Minimal Focus': {
    fontSize: 1.8,
    fontWeight: 'normal',
    textColor: '#e5e7eb',
    highlightColor: '#ffffff',
    pastWordColor: '#9ca3af',
    futureWordColor: '#6b7280',
    textShadow: false,
    animationIntensity: 10,
    backgroundColor: '#1f2937',
    backgroundOpacity: 20,
    backgroundBlur: 10,
  },
  'Ultra Compact': {
    fontSize: 0.8,
    fontWeight: 'normal',
    textShadow: true,
    textShadowBlur: 10,
    animationIntensity: 20,
    lineHeight: 1.2,
    letterSpacing: 0.02,
  },
};

// =============================================================================
// Helper Functions
// =============================================================================

const createTrackKey = (artist: string, title: string): string => {
  return `${artist.toLowerCase().trim()} - ${title.toLowerCase().trim()}`;
};

// =============================================================================
// Store Implementation
// =============================================================================

export const useConfigStore = create<ConfigState>()(
  persist(
    (set, get) => ({
      // Initial State
      translation: {
        enabled: false,
        autoTranslate: false,
        config: DEFAULT_TRANSLATION_CONFIG,
        display: DEFAULT_TRANSLATION_DISPLAY,
        showLanguageIndicator: true,
        showConfidenceScore: false,
      },
      lyricsStyle: DEFAULT_LYRICS_STYLE,
      hiragana: DEFAULT_HIRAGANA,
      viewMode: 'highlighted',
      themeId: 'dark',
      trackOffsets: {},
      globalOffset: 0,

      // Translation Actions
      setTranslationEnabled: (enabled) =>
        set((state) => ({
          translation: { ...state.translation, enabled },
        })),

      setAutoTranslate: (auto) =>
        set((state) => ({
          translation: { ...state.translation, autoTranslate: auto },
        })),

      updateTranslationConfig: (config) =>
        set((state) => ({
          translation: {
            ...state.translation,
            config: { ...state.translation.config, ...config },
          },
        })),

      updateTranslationDisplay: (display) =>
        set((state) => ({
          translation: {
            ...state.translation,
            display: { ...state.translation.display, ...display },
          },
        })),

      // Display Actions
      updateLyricsStyle: (style) =>
        set((state) => ({
          lyricsStyle: { ...state.lyricsStyle, ...style },
        })),

      updateHiragana: (settings) =>
        set((state) => ({
          hiragana: { ...state.hiragana, ...settings },
        })),

      setViewMode: (mode) => set({ viewMode: mode }),

      applyStylePreset: (presetName) => {
        const preset = STYLE_PRESETS[presetName];
        if (preset) {
          set((state) => ({
            lyricsStyle: { ...DEFAULT_LYRICS_STYLE, ...preset },
          }));
        }
      },

      // Theme Actions
      setTheme: (themeId) => {
        const theme = DEFAULT_THEMES.find((t) => t.id === themeId);
        if (theme) {
          set({ themeId });
          if (typeof window !== 'undefined') {
            applyThemeToDocument(theme);
          }
        }
      },

      getTheme: () => {
        const { themeId } = get();
        return DEFAULT_THEMES.find((t) => t.id === themeId) || DEFAULT_THEMES[1];
      },

      // Offset Actions
      setTrackOffset: (artist, title, offset) => {
        const key = createTrackKey(artist, title);
        set((state) => ({
          trackOffsets: { ...state.trackOffsets, [key]: offset },
        }));
      },

      getTrackOffset: (artist, title) => {
        const key = createTrackKey(artist, title);
        return get().trackOffsets[key] || 0;
      },

      setGlobalOffset: (offset) => set({ globalOffset: offset }),

      getTotalOffset: (artist, title) => {
        const trackOffset = get().getTrackOffset(artist, title);
        return trackOffset + get().globalOffset;
      },

      clearAllOffsets: () => set({ trackOffsets: {}, globalOffset: 0 }),

      // Reset
      resetAll: () =>
        set({
          translation: {
            enabled: false,
            autoTranslate: false,
            config: DEFAULT_TRANSLATION_CONFIG,
            display: DEFAULT_TRANSLATION_DISPLAY,
            showLanguageIndicator: true,
            showConfidenceScore: false,
          },
          lyricsStyle: DEFAULT_LYRICS_STYLE,
          hiragana: DEFAULT_HIRAGANA,
          viewMode: 'highlighted',
          themeId: 'dark',
          trackOffsets: {},
          globalOffset: 0,
        }),
    }),
    {
      name: 'lyryc-config',
      version: 2,
    }
  )
);

// =============================================================================
// Cross-Window Sync
// =============================================================================

if (typeof window !== 'undefined') {
  window.addEventListener('storage', (e) => {
    if (e.key === 'lyryc-config' && e.newValue) {
      try {
        const data = JSON.parse(e.newValue);
        if (data.state) {
          useConfigStore.setState(data.state);
        }
      } catch (error) {
        console.error('[ConfigStore] Failed to parse storage event:', error);
      }
    }
  });

  // Apply theme on load
  const state = useConfigStore.getState();
  const theme = state.getTheme();
  applyThemeToDocument(theme);
}

// =============================================================================
// Selector Hooks (for focused component usage)
// =============================================================================

export const useTranslationConfig = () =>
  useConfigStore((state) => ({
    ...state.translation,
    updateConfig: state.updateTranslationConfig,
    updateDisplay: state.updateTranslationDisplay,
    setEnabled: state.setTranslationEnabled,
    setAutoTranslate: state.setAutoTranslate,
  }));

export const useLyricsStyle = () =>
  useConfigStore((state) => ({
    style: state.lyricsStyle,
    updateStyle: state.updateLyricsStyle,
    applyPreset: state.applyStylePreset,
  }));

export const useHiraganaConfig = () =>
  useConfigStore((state) => ({
    settings: state.hiragana,
    updateSettings: state.updateHiragana,
  }));

export const useThemeConfig = () =>
  useConfigStore((state) => ({
    themeId: state.themeId,
    theme: state.getTheme(),
    setTheme: state.setTheme,
  }));

export const useOffsetConfig = () =>
  useConfigStore((state) => ({
    globalOffset: state.globalOffset,
    setGlobalOffset: state.setGlobalOffset,
    setTrackOffset: state.setTrackOffset,
    getTrackOffset: state.getTrackOffset,
    getTotalOffset: state.getTotalOffset,
    clearAll: state.clearAllOffsets,
  }));
