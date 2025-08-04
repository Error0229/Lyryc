import { create } from 'zustand';

export interface TrackInfo {
  title: string;
  artist: string;
  duration?: number;
  thumbnail?: string;
}

export interface WordTiming {
  start: number; // milliseconds
  end: number; // milliseconds
  word: string;
}

export interface LyricLine {
  time: number; // milliseconds
  text: string;
  duration?: number;
  words?: WordTiming[]; // For word-level synchronization
}

interface LyricsState {
  currentTrack: TrackInfo | null;
  lyrics: LyricLine[];
  currentTime: number;
  isPlaying: boolean;
  
  // Actions
  setCurrentTrack: (track: TrackInfo | null) => void;
  setLyrics: (lyrics: LyricLine[]) => void;
  setCurrentTime: (time: number) => void;
  setIsPlaying: (playing: boolean) => void;
  clearAll: () => void;
}

export const useLyricsStore = create<LyricsState>((set) => ({
  currentTrack: null,
  lyrics: [],
  currentTime: 0,
  isPlaying: false,

  setCurrentTrack: (track) => set({ currentTrack: track }),
  setLyrics: (lyrics) => set({ lyrics }),
  setCurrentTime: (time) => set({ currentTime: time }),
  setIsPlaying: (playing) => set({ isPlaying: playing }),
  clearAll: () => set({ 
    currentTrack: null, 
    lyrics: [], 
    currentTime: 0, 
    isPlaying: false 
  }),
}));