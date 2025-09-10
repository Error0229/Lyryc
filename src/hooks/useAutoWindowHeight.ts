import { useEffect, useRef, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';

// Simple debounce utility
function debounce<T extends (...args: any[]) => any>(func: T, delay: number): T {
  let timeoutId: NodeJS.Timeout;
  return ((...args: any[]) => {
    clearTimeout(timeoutId);
    timeoutId = setTimeout(() => func.apply(null, args), delay);
  }) as T;
}

interface UseAutoWindowHeightOptions {
  enabled?: boolean;
  debounceMs?: number;
  hasTranslation?: boolean;
  hasFurigana?: boolean;
  songId?: string; // Track current song to detect changes
  detectedLanguage?: string; // Track detected language
}

/**
 * Hook to automatically adjust window height based on content
 * Measures the rendered content and calls the Tauri backend to resize the window
 */
export const useAutoWindowHeight = ({
  enabled = true,
  debounceMs = 200,
  hasTranslation = false,
  hasFurigana = false,
  songId,
  detectedLanguage
}: UseAutoWindowHeightOptions = {}) => {
  const containerRef = useRef<HTMLDivElement>(null);
  const lastHeightRef = useRef<number>(0);
  const lastConfigRef = useRef<{
    songId?: string;
    hasTranslation: boolean;
    hasFurigana: boolean;
    detectedLanguage?: string;
  }>({
    hasTranslation: false,
    hasFurigana: false,
  });
  
  const adjustWindowHeight = useCallback(
    debounce(async (contentHeight: number, hasTranslation: boolean, hasFurigana: boolean) => {
      if (!enabled) return;
      
      // Only adjust if height changed significantly
      const heightDiff = Math.abs(contentHeight - lastHeightRef.current);
      if (heightDiff < 5) return;
      
      try {
        await invoke('adjust_window_height', {
          contentHeight,
          hasTranslation,
          hasFurigana
        });
        lastHeightRef.current = contentHeight;
      } catch (error) {
        console.error('Failed to adjust window height:', error);
      }
    }, debounceMs),
    [enabled, debounceMs]
  );

  const measureAndAdjust = useCallback(() => {
    if (!enabled || !containerRef.current) return;
    
    // Check if song or config has changed
    const currentConfig = {
      songId,
      hasTranslation,
      hasFurigana,
      detectedLanguage,
    };
    
    const hasConfigChanged = 
      lastConfigRef.current.songId !== currentConfig.songId ||
      lastConfigRef.current.hasTranslation !== currentConfig.hasTranslation ||
      lastConfigRef.current.hasFurigana !== currentConfig.hasFurigana ||
      lastConfigRef.current.detectedLanguage !== currentConfig.detectedLanguage;
    
    // Only measure and adjust if config changed
    if (!hasConfigChanged) {
      return;
    }
    
    console.log('[useAutoWindowHeight] Config changed, adjusting window height:', {
      oldConfig: lastConfigRef.current,
      newConfig: currentConfig,
    });
    
    const element = containerRef.current;
    
    // Use getBoundingClientRect for accurate measurement including margins/padding
    const rect = element.getBoundingClientRect();
    const contentHeight = rect.height;
    
    // Also consider scrollHeight in case content is clipped
    const scrollHeight = element.scrollHeight;
    const actualHeight = Math.max(contentHeight, scrollHeight);
    
    adjustWindowHeight(actualHeight, hasTranslation, hasFurigana);
    
    // Update the last config
    lastConfigRef.current = currentConfig;
  }, [enabled, hasTranslation, hasFurigana, songId, detectedLanguage, adjustWindowHeight]);

  // Only measure when song or config changes
  useEffect(() => {
    if (enabled) {
      // Delay measurement to allow for rendering
      setTimeout(measureAndAdjust, 100);
    }
  }, [enabled, songId, hasTranslation, hasFurigana, detectedLanguage, measureAndAdjust]);

  return {
    containerRef,
    measureAndAdjust
  };
};