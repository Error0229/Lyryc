/**
 * AppState - Runtime application state (NOT persisted)
 *
 * Contains transient state that changes during app lifecycle:
 * - Current track and lyrics
 * - Playback state
 * - Loading/error states
 * - Translation status
 */

import { create } from 'zustand';
import type { TrackInfo, LyricLine, TranslatedLyricLine } from './types';

// =============================================================================
// Store Interface
// =============================================================================

interface AppState {
  // Playback
  currentTrack: TrackInfo | null;
  lyrics: LyricLine[];
  translatedLyrics: TranslatedLyricLine[] | null;
  currentTime: number;
  isPlaying: boolean;

  // Operation Status
  isLoading: boolean;
  isTranslating: boolean;
  error: string | null;

  // UI State
  isClickThrough: boolean;
  isConnected: boolean;

  // Actions - Playback
  setCurrentTrack: (track: TrackInfo | null) => void;
  setLyrics: (lyrics: LyricLine[]) => void;
  setTranslatedLyrics: (lyrics: TranslatedLyricLine[] | null) => void;
  setCurrentTime: (time: number) => void;
  setIsPlaying: (playing: boolean) => void;

  // Actions - Status
  setIsLoading: (loading: boolean) => void;
  setIsTranslating: (translating: boolean) => void;
  setError: (error: string | null) => void;
  clearError: () => void;

  // Actions - UI
  setClickThrough: (enabled: boolean) => void;
  setConnected: (connected: boolean) => void;

  // Actions - Bulk
  clearTrack: () => void;
  reset: () => void;
}

// =============================================================================
// Store Implementation
// =============================================================================

export const useAppState = create<AppState>((set) => ({
  // Initial State
  currentTrack: null,
  lyrics: [],
  translatedLyrics: null,
  currentTime: 0,
  isPlaying: false,
  isLoading: false,
  isTranslating: false,
  error: null,
  isClickThrough: false,
  isConnected: false,

  // Playback Actions
  setCurrentTrack: (track) => set({ currentTrack: track }),
  setLyrics: (lyrics) => set({ lyrics }),
  setTranslatedLyrics: (lyrics) => set({ translatedLyrics: lyrics }),
  setCurrentTime: (time) => set({ currentTime: time }),
  setIsPlaying: (playing) => set({ isPlaying: playing }),

  // Status Actions
  setIsLoading: (loading) => set({ isLoading: loading }),
  setIsTranslating: (translating) => set({ isTranslating: translating }),
  setError: (error) => set({ error }),
  clearError: () => set({ error: null }),

  // UI Actions
  setClickThrough: (enabled) => set({ isClickThrough: enabled }),
  setConnected: (connected) => set({ isConnected: connected }),

  // Bulk Actions
  clearTrack: () =>
    set({
      currentTrack: null,
      lyrics: [],
      translatedLyrics: null,
      currentTime: 0,
      isPlaying: false,
      isLoading: false,
      isTranslating: false,
      error: null,
    }),

  reset: () =>
    set({
      currentTrack: null,
      lyrics: [],
      translatedLyrics: null,
      currentTime: 0,
      isPlaying: false,
      isLoading: false,
      isTranslating: false,
      error: null,
      isClickThrough: false,
      isConnected: false,
    }),
}));

// =============================================================================
// Selector Hooks (for focused component usage)
// =============================================================================

export const usePlaybackState = () =>
  useAppState((state) => ({
    track: state.currentTrack,
    lyrics: state.lyrics,
    translatedLyrics: state.translatedLyrics,
    time: state.currentTime,
    isPlaying: state.isPlaying,
    setTrack: state.setCurrentTrack,
    setLyrics: state.setLyrics,
    setTranslatedLyrics: state.setTranslatedLyrics,
    setTime: state.setCurrentTime,
    setIsPlaying: state.setIsPlaying,
    clear: state.clearTrack,
  }));

export const useOperationStatus = () =>
  useAppState((state) => ({
    isLoading: state.isLoading,
    isTranslating: state.isTranslating,
    error: state.error,
    setLoading: state.setIsLoading,
    setTranslating: state.setIsTranslating,
    setError: state.setError,
    clearError: state.clearError,
  }));

export const useUIState = () =>
  useAppState((state) => ({
    isClickThrough: state.isClickThrough,
    isConnected: state.isConnected,
    setClickThrough: state.setClickThrough,
    setConnected: state.setConnected,
  }));
