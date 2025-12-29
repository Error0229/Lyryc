/**
 * LEGACY COMPATIBILITY FILE
 */

import { useConfigStore } from './configStore';
import type { ViewMode } from './types';

export type { ViewMode };

export const useViewModeStore = () => {
  const viewMode = useConfigStore((s) => s.viewMode);
  const setViewMode = useConfigStore((s) => s.setViewMode);
  return { viewMode, setViewMode };
};
