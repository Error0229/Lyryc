/**
 * LEGACY COMPATIBILITY FILE
 *
 * This file re-exports from the new store architecture.
 * TODO: Update all imports to use '@/stores' directly, then delete this file.
 */

export type { TrackInfo, WordTiming, LyricLine } from './types';
export { useAppState as useLyricsStore } from './appState';
