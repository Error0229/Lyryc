import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import LyricsViewer from "./components/LyricsViewer";
import MediaControls from "./components/MediaControls";
import OffsetControls from "./components/OffsetControls";
import { useLyricsStore } from "./stores/lyricsStore";
import { useThemeStore } from "./stores/themeStore";
import { LyricsProcessor } from "./services/lyricsProcessor";
import { useCurrentTime } from "./hooks/useCurrentTime";
import { useIndependentTimer } from "./hooks/useSmoothTime";
import AlignmentTester from "./components/AlignmentTester";

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

  return (
    <div
      className="min-h-screen transition-all duration-500"
      style={{
        background: currentTheme.colors.background,
        fontFamily: currentTheme.typography.fontFamily,
      }}
    >
      <div className="container mx-auto px-4 py-8">
        {/* Header with Theme Selector */}
        <header className="text-center mb-8">
          <div className="flex justify-between items-start mb-4">
            {/* Theme Selector */}
            <div className="flex gap-2">
              {themes.map((theme) => (
                <button
                  key={theme.id}
                  onClick={() => setTheme(theme.id)}
                  className={`
                    px-3 py-1 rounded-full text-xs font-medium transition-all duration-300
                    ${
                      currentTheme.id === theme.id
                        ? "ring-2 ring-white/50 shadow-lg transform scale-105"
                        : "hover:scale-105 opacity-70 hover:opacity-100"
                    }
                  `}
                  style={{
                    background: theme.colors.primary,
                    color: theme.colors.text,
                  }}
                  title={theme.description}
                >
                  {theme.name}
                </button>
              ))}
            </div>

            {/* Connection Status */}
            <div>
              <span
                className="inline-flex items-center px-3 py-1 rounded-full text-sm transition-all duration-300"
                style={{
                  backgroundColor: isConnected
                    ? `${currentTheme.colors.success}20`
                    : `${currentTheme.colors.error}20`,
                  color: isConnected
                    ? currentTheme.colors.success
                    : currentTheme.colors.error,
                }}
              >
                {isConnected ? "🟢 Connected" : "🔴 Disconnected"}
              </span>
            </div>
          </div>

          <h1
            className="text-4xl font-bold mb-2 transition-colors duration-300"
            style={{
              color: currentTheme.colors.text,
              fontWeight: currentTheme.typography.fontWeight.bold,
            }}
          >
            🎵 Lyryc
          </h1>
          <p
            className="transition-colors duration-300"
            style={{ color: currentTheme.colors.textSecondary }}
          >
            Real-time lyrics sync for your music
          </p>
        </header>

        {/* Current Track Info */}
        {currentTrack && (
          <div
            className="backdrop-blur-md rounded-xl p-6 mb-8 transition-all duration-300"
            style={{
              backgroundColor: `${currentTheme.colors.backgroundSecondary}80`,
              border: `1px solid ${currentTheme.colors.border}40`,
            }}
          >
            <div className="text-center">
              <h2
                className="text-2xl mb-2 transition-colors duration-300"
                style={{
                  color: currentTheme.colors.text,
                  fontWeight: currentTheme.typography.fontWeight.semibold,
                }}
              >
                {currentTrack.originalTitle || currentTrack.title}
              </h2>
              <p
                className="text-lg transition-colors duration-300"
                style={{ color: currentTheme.colors.textSecondary }}
              >
                by {currentTrack.artist}
              </p>
            </div>
          </div>
        )}

        {/* Lyrics Display */}
        {lyrics.length > 0 && (
          <div className="space-y-6">
            <LyricsViewer
              lyrics={lyrics}
              currentTime={currentTime}
              isPlaying={isPlaying}
              className="mb-4"
              artist={currentTrack?.artist || ""}
              title={currentTrack?.title || ""}
            />

            {/* Offset Controls */}
            {currentTrack && (
              <div className="flex justify-center">
                <OffsetControls
                  artist={currentTrack.artist}
                  title={currentTrack.title}
                />
              </div>
            )}
          </div>
        )}

        {/* Media Controls */}
        {currentTrack && duration > 0 && (
          <MediaControls
            currentTime={currentTime}
            duration={duration}
            isPlaying={isPlaying}
            onPlayPause={() => {
              // This should rarely be called - only as fallback
              console.log("⚠️ Fallback play/pause called");
              setIsPlaying(!isPlaying);
            }}
            onSeek={(time) => {
              // This should rarely be called - only as fallback
              console.log("⚠️ Fallback seek called:", time);
              setBrowserTime(time);
            }}
          />
        )}

        {/* Developer Tools - Alignment Tester */}
        {currentTrack && duration > 0 && (
          <div className="mt-8">
            <AlignmentTester totalDurationSec={duration} />
          </div>
        )}

        {/* Loading State */}
        {isLoadingLyrics && (
          <div className="text-center text-white/70 mt-8">
            <div className="animate-pulse">
              <p className="text-lg mb-4">🔍 Searching for lyrics...</p>
            </div>
          </div>
        )}

        {/* Error Messages */}
        {connectionError && (
          <div
            className="backdrop-blur-md rounded-xl p-6 mb-8 border"
            style={{
              backgroundColor: `${currentTheme.colors.error}20`,
              borderColor: `${currentTheme.colors.error}40`,
            }}
          >
            <div className="text-center">
              <p
                className="text-lg font-medium mb-2"
                style={{ color: currentTheme.colors.error }}
              >
                ⚠️ Connection Error
              </p>
              <p
                className="text-sm"
                style={{ color: currentTheme.colors.textSecondary }}
              >
                {connectionError}
              </p>
            </div>
          </div>
        )}

        {lyricsError && (
          <div
            className="backdrop-blur-md rounded-xl p-6 mb-8 border"
            style={{
              backgroundColor: `${currentTheme.colors.error}20`,
              borderColor: `${currentTheme.colors.error}40`,
            }}
          >
            <div className="text-center">
              <p
                className="text-lg font-medium mb-2"
                style={{ color: currentTheme.colors.error }}
              >
                📝 Lyrics Error
              </p>
              <p
                className="text-sm"
                style={{ color: currentTheme.colors.textSecondary }}
              >
                {lyricsError}
              </p>
            </div>
          </div>
        )}

        {/* Instructions */}
        {!currentTrack && !isLoadingLyrics && !connectionError && (
          <div className="text-center text-white/70 mt-12">
            <p className="text-lg mb-4">
              🎧 Play music in your browser to see lyrics here
            </p>
            <p className="text-sm">
              Supported: Spotify Web Player, YouTube Music, Apple Music,
              SoundCloud
            </p>
          </div>
        )}
      </div>
    </div>
  );
}

export default App;
