// =============================================================================
// Core Domain Types
// =============================================================================

export interface TrackInfo {
  title: string;
  originalTitle?: string;
  artist: string;
  duration?: number;
  thumbnail?: string;
}

export interface WordTiming {
  start: number;
  end: number;
  word: string;
}

export interface LyricLine {
  time: number;
  text: string;
  duration?: number;
  words?: WordTiming[];
}

// =============================================================================
// Translation Types
// =============================================================================

export type TranslationStyle = 'natural' | 'literal' | 'poetic' | 'rhyming' | 'custom';
export type TranslationProvider = 'github' | 'gemini';
export type TranslationDisplayMode = 'original' | 'translated' | 'bilingual' | 'side-by-side';

export interface TranslationConfig {
  source_language: string;
  target_language: string;
  style: TranslationStyle;
  custom_prompt?: string;
  preserve_rhyme: boolean;
  preserve_meaning: boolean;
  maintain_structure: boolean;
  provider: TranslationProvider;
  github_api_key?: string;
  gemini_api_key?: string;
}

export interface TranslationDisplayConfig {
  mode: TranslationDisplayMode;
  fontSize: { original: number; translated: number };
  opacity: { original: number; translated: number };
  spacing: number;
  colors: { original: string; translated: string };
  animation: {
    enabled: boolean;
    type: 'fade' | 'slide' | 'scale';
    duration: number;
  };
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
  is_same_language: boolean;
}

// =============================================================================
// Display Types
// =============================================================================

export type FontWeight = 'normal' | 'medium' | 'semibold' | 'bold';
export type TextAlign = 'left' | 'center' | 'right';
export type ViewMode = 'highlighted' | 'plain';
export type HiraganaDisplayMode = 'above' | 'below' | 'replace' | 'ruby';

export interface LyricsStyle {
  fontFamily: string;
  fontSize: number;
  fontWeight: FontWeight;
  textColor: string;
  highlightColor: string;
  pastWordColor: string;
  futureWordColor: string;
  backgroundColor: string;
  backgroundOpacity: number;
  backgroundBlur: number;
  textShadow: boolean;
  textShadowColor: string;
  textShadowBlur: number;
  textGlow: boolean;
  textGlowColor: string;
  lineHeight: number;
  letterSpacing: number;
  textAlign: TextAlign;
  animationIntensity: number;
}

export interface HiraganaSettings {
  enabled: boolean;
  displayMode: HiraganaDisplayMode;
  rubyPosition: 'top' | 'bottom';
  opacity: number;
  fontSize: number;
  color: string;
  spacing: number;
}

// =============================================================================
// Theme Types
// =============================================================================

export interface ThemeColors {
  primary: string;
  secondary: string;
  accent: string;
  background: string;
  backgroundSecondary: string;
  text: string;
  textSecondary: string;
  textMuted: string;
  border: string;
  highlight: string;
  success: string;
  warning: string;
  error: string;
}

export interface ThemeAnimation {
  duration: number;
  easing: string;
  enableParticles: boolean;
  enableBlur: boolean;
  enableGlow: boolean;
}

export interface ThemeTypography {
  fontFamily: string;
  fontSize: {
    sm: string;
    base: string;
    lg: string;
    xl: string;
    '2xl': string;
    '3xl': string;
  };
  fontWeight: {
    normal: number;
    medium: number;
    semibold: number;
    bold: number;
  };
  lineHeight: {
    tight: number;
    normal: number;
    relaxed: number;
  };
}

export interface Theme {
  id: string;
  name: string;
  description: string;
  colors: ThemeColors;
  animation: ThemeAnimation;
  typography: ThemeTypography;
  custom: boolean;
}
