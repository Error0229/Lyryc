/**
 * LEGACY COMPATIBILITY FILE
 */

import { useConfigStore } from './configStore';
import { DEFAULT_THEMES, applyThemeToDocument } from './defaults/themes';
import type { Theme, ThemeColors, ThemeAnimation, ThemeTypography } from './types';

export type { Theme, ThemeColors, ThemeAnimation, ThemeTypography };

export const useThemeStore = () => {
  const themeId = useConfigStore((s) => s.themeId);
  const setThemeId = useConfigStore((s) => s.setTheme);
  const currentTheme = DEFAULT_THEMES.find((t) => t.id === themeId) || DEFAULT_THEMES[1];

  return {
    currentTheme,
    themes: DEFAULT_THEMES,
    customTheme: null,
    setTheme: setThemeId,
    updateCustomTheme: () => {}, // No-op, custom themes removed
    resetToDefault: () => setThemeId('dark'),
    exportTheme: (id: string) => JSON.stringify(DEFAULT_THEMES.find((t) => t.id === id)),
    importTheme: () => false, // No-op
  };
};
