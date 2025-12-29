/**
 * usePlaybackTime - Playback time management
 *
 * Listens for time updates from the browser extension and provides
 * smooth time interpolation for lyric highlighting.
 */

import { useState, useEffect, useRef, useCallback } from 'react';
import { listen } from '@tauri-apps/api/event';
import { useAppState } from '../stores';

interface TimeUpdatePayload {
  currentTime: number;
  duration: number;
  isPlaying: boolean;
}

interface UsePlaybackTimeReturn {
  currentTime: number;
  duration: number;
}

const UPDATE_INTERVAL_MS = 50;
const SYNC_INTERVAL_MS = 1000;

export function usePlaybackTime(): UsePlaybackTimeReturn {
  const { isPlaying, setIsPlaying } = useAppState();

  const [browserTime, setBrowserTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [currentTime, setCurrentTime] = useState(0);

  const lastSyncRef = useRef(Date.now());
  const animationFrameRef = useRef<number | null>(null);

  // Listen for time updates from extension
  useEffect(() => {
    let unlisten: (() => void) | null = null;

    const setup = async () => {
      unlisten = await listen<TimeUpdatePayload>('track-time-update', (event) => {
        const { currentTime: time, duration: dur, isPlaying: playing } = event.payload;
        setBrowserTime(time);
        setDuration(dur);
        setIsPlaying(playing);
        lastSyncRef.current = Date.now();
      });
    };

    setup();

    return () => {
      if (unlisten) unlisten();
    };
  }, [setIsPlaying]);

  // Smooth time interpolation
  useEffect(() => {
    let lastUpdate = Date.now();

    const updateTime = () => {
      const now = Date.now();
      const elapsed = (now - lastUpdate) / 1000;
      lastUpdate = now;

      // Sync with browser time periodically
      const timeSinceSync = now - lastSyncRef.current;
      if (timeSinceSync < SYNC_INTERVAL_MS) {
        // Use browser time directly after recent sync
        setCurrentTime(browserTime + (isPlaying ? timeSinceSync / 1000 : 0));
      } else if (isPlaying) {
        // Interpolate when playing
        setCurrentTime((prev) => prev + elapsed);
      }

      animationFrameRef.current = requestAnimationFrame(updateTime);
    };

    animationFrameRef.current = requestAnimationFrame(updateTime);

    return () => {
      if (animationFrameRef.current) {
        cancelAnimationFrame(animationFrameRef.current);
      }
    };
  }, [browserTime, isPlaying]);

  return { currentTime, duration };
}
