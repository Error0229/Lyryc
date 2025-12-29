/**
 * useLyricsFetcher - Lyrics fetching with abort handling
 *
 * Manages the entire lyrics fetching flow:
 * - Abort previous requests when track changes
 * - Try LyricsProcessor first, fallback to backend
 * - Handle errors gracefully
 */

import { useEffect, useRef, useCallback } from 'react';
import { useAppState } from '../stores';
import { TauriBridge } from '../services/ipc/tauri-bridge';
import { LyricsProcessor } from '../services/lyricsProcessor';
import type { LyricLine } from '../stores';

const lyricsProcessor = new LyricsProcessor({
  enableAIAlignment: true,
  enableWordLevel: true,
  language: 'auto',
  confidenceThreshold: 0.6,
  fallbackToOriginal: true,
});

export function useLyricsFetcher(): void {
  const { currentTrack, setLyrics, setTranslatedLyrics, setIsLoading, setError } = useAppState();

  const abortControllerRef = useRef<AbortController | null>(null);
  const requestSequenceRef = useRef(0);

  const fetchLyrics = useCallback(
    async (title: string, artist: string) => {
      // Cancel any existing request
      if (abortControllerRef.current) {
        abortControllerRef.current.abort();
      }

      const abortController = new AbortController();
      abortControllerRef.current = abortController;
      const currentSequence = ++requestSequenceRef.current;

      // Clear previous state
      setLyrics([]);
      setTranslatedLyrics(null);
      setIsLoading(true);
      setError(null);

      try {
        // Try LyricsProcessor first
        const result = await lyricsProcessor.processTrackLyrics(
          title,
          artist,
          undefined,
          abortController.signal
        );

        // Check if cancelled
        if (abortController.signal.aborted || currentSequence !== requestSequenceRef.current) {
          return;
        }

        if (result.lyrics.length > 0) {
          setLyrics(result.lyrics);
          return;
        }

        // Fallback to backend
        const backendLyrics = await TauriBridge.lyrics.fetch(title, artist);

        if (abortController.signal.aborted || currentSequence !== requestSequenceRef.current) {
          return;
        }

        setLyrics(backendLyrics as LyricLine[]);
      } catch (error) {
        if (!abortController.signal.aborted && currentSequence === requestSequenceRef.current) {
          console.error('[useLyricsFetcher] Failed:', error);
          setError(`No lyrics found for "${title}" by ${artist}`);
          setLyrics([]);
        }
      } finally {
        if (currentSequence === requestSequenceRef.current) {
          setIsLoading(false);
        }
      }
    },
    [setLyrics, setTranslatedLyrics, setIsLoading, setError]
  );

  // Fetch when track changes
  useEffect(() => {
    if (currentTrack) {
      fetchLyrics(currentTrack.title, currentTrack.artist);
    }
  }, [currentTrack, fetchLyrics]);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      if (abortControllerRef.current) {
        abortControllerRef.current.abort();
      }
    };
  }, []);
}
