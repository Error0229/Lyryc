import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import LyricsViewer from "./components/LyricsViewer";
import MediaControls from "./components/MediaControls";
import { useLyricsStore } from "./stores/lyricsStore";
import { useThemeStore } from "./stores/themeStore";
import { useOffsetStore } from "./stores/offsetStore";
import { useViewModeStore } from "./stores/viewModeStore";
import { LyricsProcessor } from "./services/lyricsProcessor";
import { useCurrentTime } from "./hooks/useCurrentTime";
import { useIndependentTimer } from "./hooks/useSmoothTime";
import CleanLyricDisplay from "./components/CleanLyricDisplay";
import StyleControls from "./components/StyleControls";
import { motion, AnimatePresence } from "framer-motion";

function App() {
  const {
    currentTrack,
    lyrics,
    setCurrentTrack,
    setLyrics,
    isPlaying,
    setIsPlaying,
  } = useLyricsStore();
  const { currentTheme, themes, setTheme } = useThemeStore();
  const { getTotalOffset } = useOffsetStore();
  const { viewMode, setViewMode } = useViewModeStore();
  const [isConnected, setIsConnected] = useState(false);
  const [isLoadingLyrics, setIsLoadingLyrics] = useState(false);
  const [connectionError, setConnectionError] = useState<string | null>(null);
  const [lyricsError, setLyricsError] = useState<string | null>(null);
  const [lyricsProcessor] = useState(
    () =>
      new LyricsProcessor({
        enableAIAlignment: true,
        enableWordLevel: true,
        language: "auto",
        confidenceThreshold: 0.6,
        fallbackToOriginal: true,
      })
  );

  const [browserTime, setBrowserTime] = useState(0);
  const [duration, setDuration] = useState(0);

  // Use independent timer with periodic sync
  const { currentTime, syncWithBrowser } = useIndependentTimer(browserTime, {
    isPlaying,
    syncInterval: 1000, // Sync with browser every 1 second (browser updates every 200ms)
    updateInterval: 50, // Update frontend every 50ms for very smooth progress
  });

  // Request cancellation and sequencing to prevent race conditions
  const abortControllerRef = useRef<AbortController | null>(null);
  const requestSequenceRef = useRef(0);
  

  // Use real playback state instead of mock timer
  const { seekTo } = useCurrentTime({
    isPlaying: false, // Disable the mock timer
    startTime: 0,
  });

  // Fetch lyrics when track changes
  useEffect(() => {
    if (currentTrack) {
      fetchLyrics(currentTrack.title, currentTrack.artist);
    }
  }, [currentTrack]);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      // Cancel any pending request on unmount
      if (abortControllerRef.current) {
        abortControllerRef.current.abort();
      }
    };
  }, []);

  const fetchLyrics = async (title: string, artist: string) => {
    // Cancel any existing request
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
    }

    // Create new abort controller for this request
    const abortController = new AbortController();
    abortControllerRef.current = abortController;

    // Increment and capture sequence number for this request
    const currentSequence = ++requestSequenceRef.current;

    console.log(
      `[App] Starting lyrics fetch #${currentSequence} for: "${title}" by "${artist}"`
    );

    setIsLoadingLyrics(true);
    setLyricsError(null);

    try {
      // Use the new lyrics processor for enhanced processing
      const processedResult = await lyricsProcessor.processTrackLyrics(
        title,
        artist,
        undefined, // audioUrl
        abortController.signal // Pass abort signal
      );

      // Check if this request was cancelled or superseded
      if (
        abortController.signal.aborted ||
        currentSequence !== requestSequenceRef.current
      ) {
        console.log(
          `[App] Request #${currentSequence} was cancelled or superseded`
        );
        return;
      }

      if (processedResult.lyrics.length > 0) {
        console.log(
          `[App] Request #${currentSequence} succeeded with ${processedResult.method} method, confidence: ${processedResult.confidence}`
        );
        console.log(
          `Processing time: ${processedResult.processingTime.toFixed(2)}ms`
        );
        console.log(`Has word timings: ${processedResult.hasWordTimings}`);

        setLyrics(processedResult.lyrics);
      } else {
        // Check again before fallback
        if (
          abortController.signal.aborted ||
          currentSequence !== requestSequenceRef.current
        ) {
          console.log(
            `[App] Request #${currentSequence} was cancelled before fallback`
          );
          return;
        }

        // Fallback to Tauri backend
        try {
          const backendLyrics = await invoke("fetch_lyrics", {
            trackName: title,
            artistName: artist,
          });

          // Final check before setting results
          if (
            abortController.signal.aborted ||
            currentSequence !== requestSequenceRef.current
          ) {
            console.log(
              `[App] Request #${currentSequence} was cancelled after backend fetch`
            );
            return;
          }

          console.log(
            `[App] Request #${currentSequence} succeeded with backend fallback`
          );
          setLyrics(backendLyrics as any);
        } catch (backendError) {
          // Check if still valid before showing error
          if (
            !abortController.signal.aborted &&
            currentSequence === requestSequenceRef.current
          ) {
            console.error(
              `[App] Request #${currentSequence} - Backend lyrics fetch also failed:`,
              backendError
            );
            setLyricsError(`No lyrics found for "${title}" by ${artist}`);
            setLyrics([]);
          }
        }
      }
    } catch (error) {
      // Only show error if this request wasn't cancelled
      if (
        !abortController.signal.aborted &&
        currentSequence === requestSequenceRef.current
      ) {
        console.error(`[App] Request #${currentSequence} failed:`, error);
        setLyricsError(
          `Failed to fetch lyrics for "${title}" by ${artist}. Please check your internet connection.`
        );
        setLyrics([]);
      }
    } finally {
      // Only update loading state if this is still the current request
      if (currentSequence === requestSequenceRef.current) {
        setIsLoadingLyrics(false);
      }
    }
  };

  useEffect(() => {
    // Initialize window sizing to fit screen
    const initializeWindowSizing = async () => {
      try {
        await invoke("initialize_window_sizing");
        console.log("Window sizing initialized successfully");
      } catch (error) {
        console.error("Failed to initialize window sizing:", error);
      }
    };


    // Initialize connection with browser extension
    const initializeConnection = async () => {
      try {
        await invoke("init_extension_connection");

        // Check WebSocket status
        const wsStatus = await invoke("get_websocket_status");
        setIsConnected(wsStatus as boolean);
        setConnectionError(null);
      } catch (error) {
        console.error("Failed to connect to extension:", error);
        setConnectionError(
          "Failed to connect to browser extension. Please make sure the extension is installed and active."
        );
        setIsConnected(false);
      }
    };

    // Listen for track updates from extension via WebSocket
    const setupEventListeners = async () => {
      const unlistenTrack = await listen("track-updated", (event) => {
        const trackData = event.payload as any;
        console.log("Track updated from extension:", trackData);

        const track = {
          title: trackData.title,
          originalTitle: trackData.originalTitle || trackData.title,
          artist: trackData.artist,
          thumbnail: trackData.thumbnail || "",
        };

        setCurrentTrack(track);
        // Clear previous errors when new track is detected
        setLyricsError(null);
        setConnectionError(null);
      });

      const unlistenPlayback = await listen("playback-state", (event) => {
        const isPlaying = event.payload as boolean;
        // console.log('🔄 Playback state updated from WebSocket:', isPlaying);
        setIsPlaying(isPlaying);
      });

      const unlistenTimeUpdate = await listen("track-time-update", (event) => {
        const timeData = event.payload as {
          currentTime: number;
          duration: number;
          isPlaying: boolean;
        };
        // console.log("🕒 Time Update Received:", {
        //   currentTime: timeData.currentTime,
        //   duration: timeData.duration,
        //   isPlaying: timeData.isPlaying,
        //   timestamp: new Date().toLocaleTimeString(),
        // });
        setBrowserTime(timeData.currentTime);
        setDuration(timeData.duration);
        setIsPlaying(timeData.isPlaying);
      });

      // Return cleanup function
      return () => {
        unlistenTrack();
        unlistenPlayback();
        unlistenTimeUpdate();
      };
    };

    // Removed demo track - will use real track data from extension

    initializeWindowSizing();
    initializeConnection();
    setupEventListeners();

    // Cleanup on unmount
    return () => {
      // No frontend tray cleanup needed - handled by backend
    };
  }, [setCurrentTrack, setIsPlaying]);

  // Get current line for clean display
  const getCurrentLine = () => {
    if (!lyrics.length) return null;
    
    const totalOffset = getTotalOffset(currentTrack?.artist || "", currentTrack?.title || "");
    const adjustedTime = currentTime + totalOffset;
    
    // Debug offset calculation
    if (Math.abs(totalOffset) > 0.05) {
      console.log(`[App.getCurrentLine] Offset ${totalOffset.toFixed(2)}s applied: ${currentTime.toFixed(2)}s → ${adjustedTime.toFixed(2)}s for "${currentTrack?.title}" by "${currentTrack?.artist}"`);
    }
    
    for (let i = 0; i < lyrics.length; i++) {
      const currentLine = lyrics[i];
      const nextLine = lyrics[i + 1];
      
      if (adjustedTime >= currentLine.time && 
          (!nextLine || adjustedTime < nextLine.time)) {
        return { line: currentLine, index: i };
      }
    }
    return null;
  };

  const currentLineData = getCurrentLine();
  const [showControls, setShowControls] = useState(false);
  const [showStyleControls, setShowStyleControls] = useState(false);
  const [isClickThrough, setIsClickThrough] = useState(true); // Start with click-through enabled
  const [dragModeEnabled, setDragModeEnabled] = useState(false);

  // Simplified drag solution: Click and drag on background areas
  const handleDragArea = async (e: React.MouseEvent) => {
    // Only start dragging if clicking on background areas (not on controls/buttons)
    const target = e.target as HTMLElement;
    const isClickableElement = target.closest('button, input, select, textarea, a, [role="button"], .no-drag');
    
    if (!isClickableElement && e.button === 0) {
      e.preventDefault();
      try {
        const appWindow = getCurrentWindow();
        await appWindow.startDragging();
      } catch (error) {
        console.error('Failed to start window drag:', error);
      }
    }
  };

  const handleMouseEnter = async () => {
    setShowControls(true);
    // Only interact with controls when not in click-through mode
    // Click-through state is now controlled by global shortcut toggle
  };

  const handleMouseLeave = async () => {
    setShowControls(false);
    // Controls visibility only - click-through state managed by global shortcut
  };

  // Note: Click-through state is now managed by global shortcut toggle only
  // Style controls and hover states no longer affect click-through behavior

  // Initialize click-through on mount
  useEffect(() => {
    const initializeClickThrough = async () => {
      try {
        const appWindow = getCurrentWindow();
        await appWindow.setIgnoreCursorEvents(true);
        setIsClickThrough(true);
      } catch (error) {
        console.error('Failed to initialize click-through:', error);
      }
    };
    
    initializeClickThrough();
  }, []);

  // Listen to click-through toggle events from backend (global shortcut handled in Rust)
  useEffect(() => {
    const setupEventListeners = async () => {
      const unlisten1 = await listen('click-through-enabled', () => {
        console.log('Click-through enabled by backend via global shortcut');
        setIsClickThrough(true);
        setDragModeEnabled(false);
      });

      const unlisten2 = await listen('click-through-disabled', () => {
        console.log('Click-through disabled by backend via global shortcut');
        setIsClickThrough(false);
        setDragModeEnabled(true); // In interactive mode, dragging is available
      });

      return () => {
        unlisten1();
        unlisten2();
      };
    };

    const eventListenersCleanup = setupEventListeners();

    return () => {
      eventListenersCleanup.then(cleanup => cleanup());
    };
  }, []);

  return (
    <div 
      className="h-screen relative flex items-center justify-center p-1 select-none"
      style={{ 
        background: 'transparent',
        cursor: isClickThrough ? 'default' : 'move'
      }}
      onMouseEnter={handleMouseEnter}
      onMouseLeave={handleMouseLeave}
      onMouseDown={isClickThrough ? undefined : handleDragArea}
    >
      {/* Clean Current Lyric Display */}
      <div className="text-center max-w-full mx-auto px-2">
        {currentLineData ? (
          <motion.div
            key={currentLineData.index}
            initial={{ opacity: 0, y: 20 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -20 }}
            className="text-xl md:text-3xl font-light leading-tight tracking-wide drop-shadow-2xl"
            style={{
              textShadow: '0 4px 20px rgba(0,0,0,0.8), 0 0 40px rgba(255,255,255,0.1)'
            }}
            data-tauri-drag-region
          >
            <CleanLyricDisplay
              line={currentLineData.line}
              currentTime={currentTime}
              adjustedTime={currentTime + getTotalOffset(currentTrack?.artist || "", currentTrack?.title || "")}
              fontFamily={currentTheme.typography.fontFamily}
              viewMode={viewMode}
            />
          </motion.div>
        ) : isLoadingLyrics ? (
          <div className="text-white/50 text-base animate-pulse" data-tauri-drag-region>
            🔍 Searching for lyrics...
          </div>
        ) : !currentTrack ? (
          <div className="text-center">
            <div className="text-white/50 text-base">
              🎧 Play music to see lyrics
            </div>
            <div className="text-white/30 text-xs mt-2">
              Press Ctrl+Shift+D to toggle {isClickThrough ? "drag mode" : "click-through"}
            </div>
            <div className="text-white/30 text-xs mt-1">
              Press Ctrl+Shift+M to minimize to system tray
            </div>
          </div>
        ) : lyrics.length === 0 ? (
          <div className="text-white/50 text-base" data-tauri-drag-region>
            📝 No lyrics found
          </div>
        ) : (
          <div className="text-white/50 text-base" data-tauri-drag-region>
            ⏸️ Waiting for lyrics...
          </div>
        )}
      </div>

      {/* Compact Liquid Glass Media Controls */}
      <AnimatePresence>
        {currentTrack && duration > 0 && showControls && !isClickThrough && (
          <div className="fixed bottom-6 left-1/2 transform -translate-x-1/2 z-50">
            <div className="no-drag">
              <MediaControls
                currentTime={currentTime}
                duration={duration}
                isPlaying={isPlaying}
                onPlayPause={() => {
                  console.log("⚠️ Fallback play/pause called");
                  setIsPlaying(!isPlaying);
                }}
                onSeek={(time) => {
                  console.log("⚠️ Fallback seek called:", time);
                  setBrowserTime(time);
                }}
                onOpenStyleControls={() => setShowStyleControls(true)}
              />
            </div>
          </div>
        )}
      </AnimatePresence>

      {/* Minimal Connection & Theme Status */}
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
              onClick={async () => {
                try {
                  await invoke("minimize_to_tray");
                  console.log("Window minimized to system tray");
                } catch (error) {
                  console.error("Failed to minimize to system tray:", error);
                }
              }}
              className="
                bg-black/20 backdrop-blur-xl border border-white/10 
                rounded-full p-1 shadow-lg hover:bg-white/10 transition-all
              "
              style={{
                background: 'linear-gradient(135deg, rgba(255,255,255,0.1) 0%, rgba(255,255,255,0.05) 100%)',
                backdropFilter: 'blur(20px) saturate(180%)',
                boxShadow: '0 4px 16px rgba(0,0,0,0.2)'
              }}
              title="Minimize to System Tray"
            >
              <svg className="w-3 h-3 text-white/70 hover:text-white" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M20 12H4" />
              </svg>
            </button>
            {/* Connection Status */}
            <div className="
              bg-black/20 backdrop-blur-xl border border-white/10 
              rounded-full px-2 py-1 shadow-lg flex items-center space-x-1
            " style={{
              background: 'linear-gradient(135deg, rgba(255,255,255,0.1) 0%, rgba(255,255,255,0.05) 100%)',
              backdropFilter: 'blur(20px) saturate(180%)',
              boxShadow: '0 4px 16px rgba(0,0,0,0.2)'
            }}>
              <div 
                className={`w-1.5 h-1.5 rounded-full ${isConnected ? 'bg-green-400' : 'bg-red-400'}`}
                title={isConnected ? 'Connected to browser' : 'Disconnected from browser'}
              />
              <span className="text-xs text-white/60">
                {isConnected ? 'Connected' : 'Offline'}
              </span>
            </div>

            {/* Theme Selector */}
            <div className="
              bg-black/20 backdrop-blur-xl border border-white/10 
              rounded-full px-2 py-1 shadow-lg flex items-center space-x-1
            " style={{
              background: 'linear-gradient(135deg, rgba(255,255,255,0.1) 0%, rgba(255,255,255,0.05) 100%)',
              backdropFilter: 'blur(20px) saturate(180%)',
              boxShadow: '0 4px 16px rgba(0,0,0,0.2)'
            }}>
              {themes.map((theme) => (
                <button
                  key={theme.id}
                  onClick={() => setTheme(theme.id)}
                  className={`
                    w-3 h-3 rounded-full transition-all border
                    ${currentTheme.id === theme.id 
                      ? "border-white scale-110" 
                      : "border-white/30 hover:border-white/60 hover:scale-105"
                    }
                  `}
                  style={{ backgroundColor: theme.colors.primary }}
                  title={theme.name}
                />
              ))}
            </div>
          </motion.div>
        )}
      </AnimatePresence>

      {/* Click-Through State Indicator */}
      <AnimatePresence>
        {!isClickThrough && (
          <motion.div
            initial={{ opacity: 0, y: -20 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -20 }}
            className="fixed top-2 left-1/2 transform -translate-x-1/2 z-40"
          >
            <div className="bg-green-500/90 backdrop-blur-lg rounded-lg px-3 py-1 border border-green-400/50">
              <div className="text-green-100 text-xs font-medium">
                🖱️ Interactive Mode • Draggable
              </div>
            </div>
          </motion.div>
        )}
      </AnimatePresence>

      {/* Style Controls Modal */}
      <AnimatePresence>
        <StyleControls 
          isOpen={showStyleControls} 
          onClose={() => setShowStyleControls(false)} 
        />
      </AnimatePresence>

      {/* Error Notifications (Brief, Auto-hide) */}
      {(connectionError || lyricsError) && (
        <motion.div
          initial={{ opacity: 0, x: 100 }}
          animate={{ opacity: 1, x: 0 }}
          exit={{ opacity: 0, x: 100 }}
          className="fixed top-4 right-4 z-40"
        >
          <div className="bg-red-500/20 backdrop-blur-lg rounded-lg p-3 border border-red-500/30 max-w-xs">
            <div className="text-red-200 text-xs">
              {connectionError || lyricsError}
            </div>
          </div>
        </motion.div>
      )}
    </div>
  );
}

export default App;
