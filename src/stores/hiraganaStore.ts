import { create } from 'zustand';
import { persist } from 'zustand/middleware';

interface HiraganaSettings {
  enabled: boolean;
  displayMode: 'above' | 'below' | 'replace' | 'ruby';
  rubyPosition: 'top' | 'bottom'; // Position for ruby mode
  opacity: number;
  fontSize: number; // Multiplier of base font size
  color: string;
  spacing: number; // Spacing between original and hiragana text in pixels
}

interface HiraganaStore {
  settings: HiraganaSettings;
  updateSettings: (settings: Partial<HiraganaSettings>) => void;
  toggleEnabled: () => void;
  resetToDefaults: () => void;
}

const defaultSettings: HiraganaSettings = {
  enabled: false,
  displayMode: 'ruby',
  rubyPosition: 'top',
  opacity: 0.8,
  fontSize: 0.7,
  color: '#a0a0a0',
  spacing: 4,
};

export const useHiraganaStore = create<HiraganaStore>()(
  persist(
    (set) => ({
      settings: defaultSettings,

      updateSettings: (newSettings) =>
        set((state) => ({
          settings: { ...state.settings, ...newSettings },
        })),

      toggleEnabled: () =>
        set((state) => ({
          settings: { ...state.settings, enabled: !state.settings.enabled },
        })),

      resetToDefaults: () =>
        set({ settings: defaultSettings }),
    }),
    {
      name: 'hiragana-settings',
      version: 1,
    }
  )
);

// Add storage event listener for real-time cross-window synchronization
if (typeof window !== 'undefined') {
  window.addEventListener('storage', (e) => {
    if (e.key === 'hiragana-settings' && e.newValue) {
      try {
        const data = JSON.parse(e.newValue);
        if (data.state?.settings) {
          useHiraganaStore.setState({ settings: data.state.settings });
        }
      } catch (error) {
        console.error('[HiraganaStore] Failed to parse storage event:', error);
      }
    }
  });
}