import { create } from 'zustand';
import { persist } from 'zustand/middleware';

interface OffsetState {
  // Track-specific offsets (key: "artist - title")
  trackOffsets: Record<string, number>;
  // Global offset applied to all tracks
  globalOffset: number;
  
  // Actions
  setTrackOffset: (artist: string, title: string, offset: number) => void;
  getTrackOffset: (artist: string, title: string) => number;
  setGlobalOffset: (offset: number) => void;
  getTotalOffset: (artist: string, title: string) => number;
  clearTrackOffset: (artist: string, title: string) => void;
  clearAllOffsets: () => void;
}

const createTrackKey = (artist: string, title: string) => {
  return `${artist.toLowerCase().trim()} - ${title.toLowerCase().trim()}`;
};

export const useOffsetStore = create<OffsetState>()(
  persist(
    (set, get) => ({
      trackOffsets: {},
      globalOffset: 0,

      setTrackOffset: (artist: string, title: string, offset: number) => {
        const key = createTrackKey(artist, title);
        set((state) => ({
          trackOffsets: {
            ...state.trackOffsets,
            [key]: offset
          }
        }));
      },

      getTrackOffset: (artist: string, title: string) => {
        const key = createTrackKey(artist, title);
        return get().trackOffsets[key] || 0;
      },

      setGlobalOffset: (offset: number) => {
        set({ globalOffset: offset });
      },

      getTotalOffset: (artist: string, title: string) => {
        const trackOffset = get().getTrackOffset(artist, title);
        const globalOffset = get().globalOffset;
        return trackOffset + globalOffset;
      },

      clearTrackOffset: (artist: string, title: string) => {
        const key = createTrackKey(artist, title);
        set((state) => {
          const newOffsets = { ...state.trackOffsets };
          delete newOffsets[key];
          return { trackOffsets: newOffsets };
        });
      },

      clearAllOffsets: () => {
        set({
          trackOffsets: {},
          globalOffset: 0
        });
      }
    }),
    {
      name: 'lyrics-offset-storage', // localStorage key
      version: 1,
    }
  )
);