import { create } from 'zustand';
import { persist } from 'zustand/middleware';

export type ViewMode = 'highlighted' | 'plain';

interface ViewModeState {
  viewMode: ViewMode;
  setViewMode: (mode: ViewMode) => void;
}

export const useViewModeStore = create<ViewModeState>()(
  persist(
    (set) => ({
      viewMode: 'highlighted',
      
      setViewMode: (mode) => set({ viewMode: mode }),
    }),
    {
      name: 'lyryc-view-mode-store',
    }
  )
);