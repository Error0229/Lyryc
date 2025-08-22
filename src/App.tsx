import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import LyricsViewer from "./components/LyricsViewer";
import MediaControls from "./components/MediaControls";
import OffsetControls from "./components/OffsetControls";
import { useLyricsStore } from "./stores/lyricsStore";
import { useThemeStore } from "./stores/themeStore";
import { useOffsetStore } from "./stores/offsetStore";
import { LyricsProcessor } from "./services/lyricsProcessor";
import { useCurrentTime } from "./hooks/useCurrentTime";
import { useIndependentTimer } from "./hooks/useSmoothTime";
import AlignmentTester from "./components/AlignmentTester";
import CleanLyricDisplay from "./components/CleanLyricDisplay";
import { motion } from "framer-motion";

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

    initializeConnection();
    setupEventListeners();
  }, [setCurrentTrack, setIsPlaying]);

  // Get current line for clean display
  const getCurrentLine = () => {
    if (!lyrics.length) return null;
    
    const adjustedTime = currentTime + getTotalOffset(currentTrack?.artist || "", currentTrack?.title || "");
    
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

  return (
    <div 
      className="h-screen relative flex items-center justify-center p-4 select-none"
      style={{ background: 'transparent' }}
      onMouseEnter={() => setShowControls(true)}
      onMouseLeave={() => setShowControls(false)}
    >
      {/* Clean Current Lyric Display */}
      <div className="text-center max-w-4xl mx-auto">
        {currentLineData ? (
          <motion.div
            key={currentLineData.index}
            initial={{ opacity: 0, y: 20 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -20 }}
            className="text-2xl md:text-4xl font-light leading-relaxed tracking-wide drop-shadow-2xl"
            style={{
              textShadow: '0 4px 20px rgba(0,0,0,0.8), 0 0 40px rgba(255,255,255,0.1)'
            }}
          >
            <CleanLyricDisplay
              line={currentLineData.line}
              currentTime={currentTime}
              adjustedTime={currentTime + getTotalOffset(currentTrack?.artist || "", currentTrack?.title || "")}
              fontFamily={currentTheme.typography.fontFamily}
            />
          </motion.div>
        ) : isLoadingLyrics ? (
          <div className="text-white/50 text-lg animate-pulse">
            🔍 Searching for lyrics...
          </div>
        ) : !currentTrack ? (
          <div className="text-white/50 text-lg">
            🎧 Play music to see lyrics
          </div>
        ) : lyrics.length === 0 ? (
          <div className="text-white/50 text-lg">
            📝 No lyrics found
          </div>
        ) : (
          <div className="text-white/50 text-lg">
            ⏸️ Waiting for lyrics...
          </div>
        )}
      </div>

      {/* Hidden Controls Menu */}
      <motion.div
        initial={{ opacity: 0, y: 20 }}
        animate={{ 
          opacity: showControls ? 1 : 0,
          y: showControls ? 0 : 20,
          pointerEvents: showControls ? 'auto' : 'none'
        }}
        transition={{ duration: 0.3 }}
        className="fixed bottom-4 left-1/2 transform -translate-x-1/2 z-50"
      >
        <div className="bg-black/90 backdrop-blur-lg rounded-2xl p-4 border border-white/20 shadow-2xl max-w-md">
          <div className="flex flex-col items-center space-y-3">
            {/* Current Track Info */}
            {currentTrack && (
              <div className="text-center text-white/80 mb-2">
                <div className="text-sm font-medium truncate max-w-xs">{currentTrack.originalTitle || currentTrack.title}</div>
                <div className="text-xs text-white/60">by {currentTrack.artist}</div>
              </div>
            )}

            {/* Controls Row */}
            <div className="flex items-center space-x-4">
              {/* Theme Selector */}
              <div className="flex gap-2">
                {themes.map((theme) => (
                  <button
                    key={theme.id}
                    onClick={() => setTheme(theme.id)}
                    className={`
                      w-5 h-5 rounded-full transition-all duration-300 border-2
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

              {/* Connection Status */}
              <div className="flex items-center">
                <div 
                  className={`w-2 h-2 rounded-full ${isConnected ? 'bg-green-400' : 'bg-red-400'}`}
                  title={isConnected ? 'Connected' : 'Disconnected'}
                />
              </div>

              {/* Offset Controls */}
              {currentTrack && (
                <OffsetControls
                  artist={currentTrack.artist}
                  title={currentTrack.title}
                />
              )}
            </div>

            {/* Media Controls */}
            {currentTrack && duration > 0 && (
              <div className="w-full">
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
                />
              </div>
            )}

            {/* Developer Tools Toggle */}
            {currentTrack && duration > 0 && (
              <details className="w-full">
                <summary className="text-white/60 text-xs cursor-pointer hover:text-white/80 text-center">
                  Dev Tools
                </summary>
                <div className="mt-2">
                  <AlignmentTester totalDurationSec={duration} />
                </div>
              </details>
            )}
          </div>
        </div>
      </motion.div>

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
