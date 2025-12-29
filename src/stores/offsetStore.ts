/**
 * LEGACY COMPATIBILITY FILE
 */

import { useConfigStore } from './configStore';

export const useOffsetStore = () => {
  const store = useConfigStore();
  return {
    trackOffsets: store.trackOffsets,
    globalOffset: store.globalOffset,
    setTrackOffset: store.setTrackOffset,
    getTrackOffset: store.getTrackOffset,
    setGlobalOffset: store.setGlobalOffset,
    getTotalOffset: store.getTotalOffset,
    clearTrackOffset: (artist: string, title: string) => {
      // Individual track clear - set to 0
      store.setTrackOffset(artist, title, 0);
    },
    clearAllOffsets: store.clearAllOffsets,
  };
};
