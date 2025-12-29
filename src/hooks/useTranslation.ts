/**
 * useTranslation - Translation orchestration
 *
 * Handles automatic translation when:
 * - Lyrics change
 * - Translation is enabled
 * - Auto-translate is on
 */

import { useEffect } from 'react';
import { useAppState, useConfigStore } from '../stores';
import { TauriBridge } from '../services/ipc/tauri-bridge';
import type { TranslatedLyricLine, LyricLine } from '../stores';

export function useTranslation(): void {
  const { lyrics, setTranslatedLyrics, setIsTranslating, setError } = useAppState();
  const translation = useConfigStore((s) => s.translation);
  const { enabled, autoTranslate, config } = translation;

  useEffect(() => {
    const handleTranslation = async () => {
      // Guard: Skip if disabled or no lyrics
      if (!enabled || !autoTranslate || lyrics.length === 0) {
        setTranslatedLyrics(null);
        return;
      }

      setIsTranslating(true);

      try {
        const result = await TauriBridge.translation.translate(lyrics, config);

        if (!result.success) {
          console.error('[useTranslation] Failed:', result.error);
          setError(result.error || 'Translation failed');
          setTranslatedLyrics(null);
          return;
        }

        // Skip if same language
        if (result.is_same_language) {
          setTranslatedLyrics(null);
          return;
        }

        if (result.detected_language === config.target_language) {
          setTranslatedLyrics(null);
          return;
        }

        // Convert to frontend format
        const converted: TranslatedLyricLine[] = result.translated_lines.map((line: any) => ({
          time: line.time,
          text: line.translated_text || line.text,
          duration: line.duration,
          words: [],
          originalText: line.original_text,
          translatedText: line.translated_text,
          translationConfidence: line.translation_confidence,
          detectedLanguage: line.detected_language,
        }));

        setTranslatedLyrics(converted);
      } catch (error) {
        console.error('[useTranslation] Error:', error);
        setError(error instanceof Error ? error.message : 'Translation error');
        setTranslatedLyrics(null);
      } finally {
        setIsTranslating(false);
      }
    };

    handleTranslation();
  }, [lyrics, enabled, autoTranslate, config, setTranslatedLyrics, setIsTranslating, setError]);
}
