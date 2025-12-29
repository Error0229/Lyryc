/**
 * App.tsx - Main Application (Refactored)
 *
 * Slim composition file that:
 * 1. Initializes hooks
 * 2. Renders UI components
 *
 * All logic is delegated to focused hooks:
 * - useAppEvents: Event listeners
 * - useLyricsFetcher: Lyrics fetching
 * - useTranslation: Translation orchestration
 * - usePlaybackTime: Time management
 */

import { useState, useEffect } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { motion, AnimatePresence } from 'framer-motion';

// Stores
import { useAppState, useConfigStore, useThemeConfig, useOffsetConfig } from './stores';
import { DEFAULT_THEMES } from './stores/defaults/themes';

// Hooks
import { useAppEvents } from './hooks/useAppEvents';
import { useLyricsFetcher } from './hooks/useLyricsFetcher';
import { useTranslation } from './hooks/useTranslation';
import { usePlaybackTime } from './hooks/usePlaybackTime';
import { useAutoWindowHeight } from './hooks/useAutoWindowHeight';

// Components
import CleanLyricDisplay from './components/CleanLyricDisplay';
import MediaControls from './components/MediaControls';

// Services
import { TauriBridge } from './services/ipc/tauri-bridge';

// =============================================================================
// Constants
// =============================================================================

const ERROR_DISPLAY_DURATION_MS = 5000;

// =============================================================================
// App Component
// =============================================================================

function App() {
  // Initialize hooks
  const { isConnected } = useAppEvents();
  useLyricsFetcher();
  useTranslation();
  const { currentTime, duration } = usePlaybackTime();

  // App state
  const {
    currentTrack,
    lyrics,
    translatedLyrics,
    isPlaying,
    isLoading,
    isClickThrough,
    error,
    setIsPlaying,
    setClickThrough,
    clearError,
  } = useAppState();

  // Config state
  const translation = useConfigStore((s) => s.translation);
  const hiragana = useConfigStore((s) => s.hiragana);
  const viewMode = useConfigStore((s) => s.viewMode);
  const { theme, setTheme } = useThemeConfig();
  const { getTotalOffset } = useOffsetConfig();

  // Local UI state
  const [showControls, setShowControls] = useState(false);
  const [browserTime, setBrowserTime] = useState(0);

  // Auto-hide errors
  useEffect(() => {
    if (error) {
      const timer = setTimeout(clearError, ERROR_DISPLAY_DURATION_MS);
      return () => clearTimeout(timer);
    }
  }, [error, clearError]);

  // Auto window height
  const detectedLanguage = translatedLyrics?.[0]?.detectedLanguage;
  const { containerRef } = useAutoWindowHeight({
    enabled: true,
    hasTranslation: translation.enabled && (translatedLyrics?.length ?? 0) > 0,
    hasFurigana: hiragana.enabled && hiragana.displayMode !== 'replace',
    songId: currentTrack ? `${currentTrack.artist}-${currentTrack.title}` : undefined,
    detectedLanguage,
  });

  // Initialize click-through state on mount
  // Start in interactive mode (click-through disabled) so users can interact with controls
  useEffect(() => {
    const init = async () => {
      try {
        const appWindow = getCurrentWindow();
        // Start in interactive mode (click-through disabled)
        await appWindow.setIgnoreCursorEvents(false);
        setClickThrough(false);
      } catch (error) {
        console.error('[App] Failed to initialize click-through:', error);
      }
    };
    init();
  }, [setClickThrough]);

  // Get current lyric line
  const getCurrentLine = () => {
    const displayLyrics =
      translation.enabled && translatedLyrics && translatedLyrics.length > 0
        ? translatedLyrics
        : lyrics;

    if (!displayLyrics.length) return null;

    const offset = getTotalOffset(currentTrack?.artist || '', currentTrack?.title || '');
    const adjustedTime = currentTime + offset;

    for (let i = 0; i < displayLyrics.length; i++) {
      const line = displayLyrics[i];
      const next = displayLyrics[i + 1];

      if (adjustedTime >= line.time && (!next || adjustedTime < next.time)) {
        return { line, index: i };
      }
    }
    return null;
  };

  const currentLineData = getCurrentLine();
  const offset = getTotalOffset(currentTrack?.artist || '', currentTrack?.title || '');

  // Drag handler
  const handleDrag = async (e: React.MouseEvent) => {
    const target = e.target as HTMLElement;
    const isClickable = target.closest('button, input, select, .no-drag');

    if (!isClickable && e.button === 0) {
      e.preventDefault();
      try {
        const appWindow = getCurrentWindow();
        await appWindow.startDragging();
      } catch (error) {
        console.error('[App] Drag failed:', error);
      }
    }
  };

  return (
    <div
      className="h-screen relative flex items-center justify-center p-1 select-none"
      style={{
        background: 'transparent',
        cursor: isClickThrough ? 'default' : 'move',
      }}
      onMouseEnter={() => setShowControls(true)}
      onMouseLeave={() => setShowControls(false)}
      onMouseDown={isClickThrough ? undefined : handleDrag}
    >
      {/* Lyrics Display */}
      <div ref={containerRef} className="text-center max-w-full mx-auto px-2">
        {currentLineData ? (
          <motion.div
            key={currentLineData.index}
            initial={{ opacity: 0, y: 20 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -20 }}
            className="text-xl md:text-3xl font-light leading-tight tracking-wide drop-shadow-2xl"
            style={{
              textShadow: '0 4px 20px rgba(0,0,0,0.8), 0 0 40px rgba(255,255,255,0.1)',
            }}
          >
            <CleanLyricDisplay
              line={currentLineData.line}
              currentTime={currentTime}
              adjustedTime={currentTime + offset}
              fontFamily={theme.typography.fontFamily}
              viewMode={viewMode}
            />
          </motion.div>
        ) : isLoading ? (
          <div className="text-white/50 text-base animate-pulse">Searching for lyrics...</div>
        ) : !currentTrack ? (
          <div className="text-center">
            <div className="text-white/50 text-base">Play music to see lyrics</div>
            <div className="text-white/30 text-xs mt-2">
              Press Ctrl+Shift+D to toggle {isClickThrough ? 'drag mode' : 'click-through'}
            </div>
          </div>
        ) : lyrics.length === 0 ? (
          <div className="text-white/50 text-base">No lyrics found</div>
        ) : (
          <div className="text-white/50 text-base">Waiting for lyrics...</div>
        )}
      </div>

      {/* Media Controls */}
      <AnimatePresence>
        {currentTrack && duration > 0 && showControls && !isClickThrough && (
          <div className="fixed bottom-6 left-1/2 transform -translate-x-1/2 z-50">
            <div className="no-drag">
              <MediaControls
                currentTime={currentTime}
                duration={duration}
                isPlaying={isPlaying}
                onPlayPause={() => setIsPlaying(!isPlaying)}
                onSeek={(time) => setBrowserTime(time)}
                onOpenStyleControls={() => TauriBridge.window.openSettings()}
              />
            </div>
          </div>
        )}
      </AnimatePresence>

      {/* Status Bar */}
      <AnimatePresence>
        {!isClickThrough && (
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            className="fixed top-4 right-4 z-40 flex items-center space-x-2"
          >
            {/* Minimize Button */}
            <button
              onClick={() => TauriBridge.window.minimizeToTray()}
              className="bg-black/20 backdrop-blur-xl border border-white/10 rounded-full p-1 hover:bg-white/10 transition-all"
              title="Minimize to Tray"
            >
              <svg
                className="w-3 h-3 text-white/70"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M20 12H4" />
              </svg>
            </button>

            {/* Connection Status */}
            <div className="bg-black/20 backdrop-blur-xl border border-white/10 rounded-full px-2 py-1 flex items-center space-x-1">
              <div
                className={`w-1.5 h-1.5 rounded-full ${isConnected ? 'bg-green-400' : 'bg-red-400'}`}
              />
              <span className="text-xs text-white/60">{isConnected ? 'Connected' : 'Offline'}</span>
            </div>

            {/* Theme Selector */}
            <div className="bg-black/20 backdrop-blur-xl border border-white/10 rounded-full px-2 py-1 flex items-center space-x-1">
              {DEFAULT_THEMES.map((t) => (
                <button
                  key={t.id}
                  onClick={() => setTheme(t.id)}
                  className={`w-3 h-3 rounded-full transition-all border ${
                    theme.id === t.id
                      ? 'border-white scale-110'
                      : 'border-white/30 hover:border-white/60'
                  }`}
                  style={{ backgroundColor: t.colors.primary }}
                  title={t.name}
                />
              ))}
            </div>
          </motion.div>
        )}
      </AnimatePresence>

      {/* Interactive Mode Indicator */}
      <AnimatePresence>
        {!isClickThrough && (
          <motion.div
            initial={{ opacity: 0, y: -20 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -20 }}
            className="fixed top-2 left-1/2 transform -translate-x-1/2 z-40"
          >
            <div className="bg-green-500/90 backdrop-blur-lg rounded-lg px-3 py-1 border border-green-400/50">
              <div className="text-green-100 text-xs font-medium">Interactive Mode</div>
            </div>
          </motion.div>
        )}
      </AnimatePresence>

      {/* Error Notification */}
      <AnimatePresence>
        {error && (
          <motion.div
            initial={{ opacity: 0, x: 100 }}
            animate={{ opacity: 1, x: 0 }}
            exit={{ opacity: 0, x: 100 }}
            className="fixed top-4 right-4 z-40"
          >
            <div className="bg-red-500/20 backdrop-blur-lg rounded-lg p-3 border border-red-500/30 max-w-xs">
              <div className="text-red-200 text-xs">{error}</div>
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

export default App;
