/**
 * useAppEvents - Centralized event listener setup
 *
 * Handles all Tauri events from the backend/extension:
 * - track-updated: New track detected
 * - playback-state: Play/pause state
 * - track-time-update: Current playback time
 * - click-through-enabled/disabled: Global shortcut toggle
 */

import { useEffect, useCallback } from 'react';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { useAppState } from '../stores';
import type { TrackInfo } from '../stores';

interface TimeUpdatePayload {
  currentTime: number;
  duration: number;
  isPlaying: boolean;
}

interface TrackPayload {
  title: string;
  originalTitle?: string;
  artist: string;
  thumbnail?: string;
}

interface UseAppEventsReturn {
  isConnected: boolean;
}

export function useAppEvents(): UseAppEventsReturn {
  const {
    setCurrentTrack,
    setIsPlaying,
    setClickThrough,
    setConnected,
    isConnected,
    clearError,
  } = useAppState();

  // Initialize connection
  const initializeConnection = useCallback(async () => {
    try {
      await invoke('init_extension_connection');
      const wsStatus = await invoke<boolean>('get_websocket_status');
      setConnected(wsStatus);
    } catch (error) {
      console.error('[useAppEvents] Failed to connect:', error);
      setConnected(false);
    }
  }, [setConnected]);

  // Initialize window sizing
  const initializeWindow = useCallback(async () => {
    try {
      await invoke('initialize_window_sizing');
    } catch (error) {
      console.error('[useAppEvents] Failed to initialize window:', error);
    }
  }, []);

  useEffect(() => {
    initializeConnection();
    initializeWindow();
  }, [initializeConnection, initializeWindow]);

  // Event listeners
  useEffect(() => {
    const unlisteners: (() => void)[] = [];

    const setup = async () => {
      // Track updates
      const unlistenTrack = await listen<TrackPayload>('track-updated', (event) => {
        const data = event.payload;
        const track: TrackInfo = {
          title: data.title,
          originalTitle: data.originalTitle || data.title,
          artist: data.artist,
          thumbnail: data.thumbnail,
        };
        setCurrentTrack(track);
        clearError();
      });
      unlisteners.push(unlistenTrack);

      // Playback state
      const unlistenPlayback = await listen<boolean>('playback-state', (event) => {
        setIsPlaying(event.payload);
      });
      unlisteners.push(unlistenPlayback);

      // Time updates (handled separately to avoid re-renders here)
      // Time state is managed in useSmoothTime hook

      // Click-through toggle
      const unlistenClickEnabled = await listen('click-through-enabled', () => {
        setClickThrough(true);
      });
      unlisteners.push(unlistenClickEnabled);

      const unlistenClickDisabled = await listen('click-through-disabled', () => {
        setClickThrough(false);
      });
      unlisteners.push(unlistenClickDisabled);
    };

    setup();

    return () => {
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, [setCurrentTrack, setIsPlaying, setClickThrough, clearError]);

  return { isConnected };
}
