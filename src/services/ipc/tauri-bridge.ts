/**
 * TauriBridge - Centralized IPC abstraction layer
 *
 * All Tauri invoke calls go through here. Benefits:
 * 1. Single point of contact with backend
 * 2. Easy to mock for testing
 * 3. Type-safe API surface
 * 4. Centralized error handling
 */

import { invoke } from '@tauri-apps/api/core';
import type {
  LyricLine,
  TranslationConfig,
  TranslationResult,
} from '../../stores/types';

// =============================================================================
// Backend Types (matching Rust structs)
// =============================================================================

interface BackendLyricLine {
  time: number;
  text: string;
  duration?: number;
}

interface FuriganaResult {
  segments: Array<{ text: string; reading?: string }>;
  html: string;
}

interface CleanedNames {
  artist: string;
  track: string;
}

// =============================================================================
// Lyrics Operations
// =============================================================================

export const LyricsBridge = {
  async fetch(trackName: string, artistName: string): Promise<LyricLine[]> {
    return invoke<LyricLine[]>('fetch_lyrics', { trackName, artistName });
  },

  async cleanNames(artistName: string, trackName: string): Promise<CleanedNames> {
    return invoke<CleanedNames>('clean_artist_and_track_names_command', {
      artistName,
      trackName,
    });
  },
};

// =============================================================================
// Translation Operations
// =============================================================================

export const TranslationBridge = {
  async translate(lyrics: LyricLine[], config: TranslationConfig): Promise<TranslationResult> {
    const backendLyrics: BackendLyricLine[] = lyrics.map((line) => ({
      time: line.time,
      text: line.text,
      duration: line.duration,
    }));

    return invoke<TranslationResult>('translate_lyrics_command', {
      lyrics: backendLyrics,
      config,
    });
  },

  async detectLanguage(text: string): Promise<string | null> {
    try {
      const result = await invoke<{ detected_language: string }>('detect_language', { text });
      return result.detected_language;
    } catch {
      return null;
    }
  },

  async getSupportedLanguages(): Promise<[string, string][]> {
    try {
      return await invoke<[string, string][]>('get_supported_languages');
    } catch {
      return [
        ['auto', 'Auto-detect'],
        ['en', 'English'],
        ['es', 'Spanish'],
        ['fr', 'French'],
        ['de', 'German'],
      ];
    }
  },
};

// =============================================================================
// Furigana Operations
// =============================================================================

export const FuriganaBridge = {
  async generate(text: string): Promise<FuriganaResult> {
    return invoke<FuriganaResult>('generate_furigana_command', { text });
  },
};

// =============================================================================
// Window Operations
// =============================================================================

export const WindowBridge = {
  async openSettings(): Promise<void> {
    return invoke('open_settings_window');
  },

  async minimizeToTray(): Promise<void> {
    return invoke('minimize_to_tray');
  },

  async setClickThrough(enabled: boolean): Promise<void> {
    return invoke('set_click_through', { enabled });
  },

  async startDrag(): Promise<void> {
    return invoke('start_drag');
  },
};

// =============================================================================
// Playback Control Operations
// =============================================================================

export const PlaybackBridge = {
  async play(): Promise<void> {
    return invoke('send_play_command');
  },

  async pause(): Promise<void> {
    return invoke('send_pause_command');
  },

  async next(): Promise<void> {
    return invoke('send_next_command');
  },

  async previous(): Promise<void> {
    return invoke('send_previous_command');
  },

  async seek(position: number): Promise<void> {
    return invoke('send_seek_command', { position });
  },
};

// =============================================================================
// Unified Export
// =============================================================================

export const TauriBridge = {
  lyrics: LyricsBridge,
  translation: TranslationBridge,
  furigana: FuriganaBridge,
  window: WindowBridge,
  playback: PlaybackBridge,
};

export default TauriBridge;
