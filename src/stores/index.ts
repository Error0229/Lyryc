/**
 * Unified Store Exports
 *
 * Import from here instead of individual store files.
 *
 * Usage:
 *   import { useAppState, useConfigStore } from '@/stores';
 *   import { usePlaybackState, useLyricsStyle } from '@/stores';
 *   import type { TrackInfo, LyricLine } from '@/stores';
 */

// =============================================================================
// Types
// =============================================================================

export type {
  // Core
  TrackInfo,
  WordTiming,
  LyricLine,
  // Translation
  TranslationStyle,
  TranslationProvider,
  TranslationDisplayMode,
  TranslationConfig,
  TranslationDisplayConfig,
  TranslatedLyricLine,
  TranslationResult,
  // Display
  FontWeight,
  TextAlign,
  ViewMode,
  HiraganaDisplayMode,
  LyricsStyle,
  HiraganaSettings,
  // Theme
  ThemeColors,
  ThemeAnimation,
  ThemeTypography,
  Theme,
} from './types';

// =============================================================================
// Runtime State
// =============================================================================

export {
  useAppState,
  usePlaybackState,
  useOperationStatus,
  useUIState,
} from './appState';

// =============================================================================
// Config State (Persisted)
// =============================================================================

export {
  useConfigStore,
  useTranslationConfig,
  useLyricsStyle,
  useHiraganaConfig,
  useThemeConfig,
  useOffsetConfig,
} from './configStore';

// =============================================================================
// Theme Utilities
// =============================================================================

export { DEFAULT_THEMES, applyThemeToDocument } from './defaults/themes';

// =============================================================================
// Legacy Compatibility Exports
// =============================================================================

// Re-export old store names for gradual migration
// TODO: Remove after migration complete

export { useAppState as useLyricsStore } from './appState';
