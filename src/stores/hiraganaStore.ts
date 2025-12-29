/**
 * LEGACY COMPATIBILITY FILE
 */

import { useConfigStore } from './configStore';

export const useHiraganaStore = () => {
  const settings = useConfigStore((s) => s.hiragana);
  const updateSettings = useConfigStore((s) => s.updateHiragana);

  return {
    settings,
    updateSettings,
    toggleEnabled: () => updateSettings({ enabled: !settings.enabled }),
    resetToDefaults: () => useConfigStore.getState().resetAll(),
  };
};
