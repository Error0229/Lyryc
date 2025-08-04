import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import LyricsViewer from "./components/LyricsViewer";
import MediaControls from "./components/MediaControls";
import { useLyricsStore } from "./stores/lyricsStore";
import { useThemeStore } from "./stores/themeStore";
import { LRCLibService } from "./services/lrclib";
import { LyricsProcessor } from "./services/lyricsProcessor";
import { useCurrentTime } from "./hooks/useCurrentTime";

function App() {
  const { currentTrack, lyrics, setCurrentTrack, setLyrics, isPlaying, setIsPlaying } = useLyricsStore();
  const { currentTheme, themes, setTheme } = useThemeStore();
  const [isConnected, setIsConnected] = useState(false);
  const [isLoadingLyrics, setIsLoadingLyrics] = useState(false);
  const [lyricsProcessor] = useState(() => new LyricsProcessor({
    enableAIAlignment: true,
    enableWordLevel: true,
    language: 'auto',
    confidenceThreshold: 0.6,
    fallbackToOriginal: true
  }));
  
  // Mock playback state for demo
  const { currentTime, seekTo } = useCurrentTime({ 
    isPlaying, 
    startTime: 0 
  });

  // Fetch lyrics when track changes
  useEffect(() => {
    if (currentTrack) {
      fetchLyrics(currentTrack.title, currentTrack.artist);
    }
  }, [currentTrack]);

  const fetchLyrics = async (title: string, artist: string) => {
    setIsLoadingLyrics(true);
    try {
      // Use the new lyrics processor for enhanced processing
      const processedResult = await lyricsProcessor.processTrackLyrics(
        title,
        artist
      );

      if (processedResult.lyrics.length > 0) {
        console.log(`Lyrics processed with ${processedResult.method} method, confidence: ${processedResult.confidence}`);
        console.log(`Processing time: ${processedResult.processingTime.toFixed(2)}ms`);
        console.log(`Has word timings: ${processedResult.hasWordTimings}`);
        
        setLyrics(processedResult.lyrics);
      } else {
        // Fallback to Tauri backend
        try {
          const backendLyrics = await invoke("fetch_lyrics", {
            trackName: title,
            artistName: artist
          });
          setLyrics(backendLyrics as any);
        } catch (backendError) {
          console.error("Backend lyrics fetch also failed:", backendError);
          setLyrics([]);
        }
      }
    } catch (error) {
      console.error("Failed to fetch lyrics:", error);
      setLyrics([]);
    } finally {
      setIsLoadingLyrics(false);
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
      } catch (error) {
        console.error("Failed to connect to extension:", error);
      }
    };

    // Listen for track updates from extension via WebSocket
    const setupEventListeners = async () => {
      const unlistenTrack = await listen('track-updated', (event) => {
        const trackData = event.payload as any;
        console.log('Track updated from extension:', trackData);
        
        const track = {
          title: trackData.title,
          artist: trackData.artist,
          thumbnail: trackData.thumbnail || "",
        };
        
        setCurrentTrack(track);
      });

      const unlistenPlayback = await listen('playback-state', (event) => {
        const isPlaying = event.payload as boolean;
        console.log('Playback state updated:', isPlaying);
        setIsPlaying(isPlaying);
      });

      // Return cleanup function
      return () => {
        unlistenTrack();
        unlistenPlayback();
      };
    };

    // Test with a sample track for demo (remove in production)
    const testTrack = {
      title: "Blinding Lights",
      artist: "The Weeknd",
      thumbnail: "",
    };
    setCurrentTrack(testTrack);

    initializeConnection();
    setupEventListeners();
  }, [setCurrentTrack, setIsPlaying]);

  return (
    <div 
      className="min-h-screen transition-all duration-500"
      style={{ 
        background: currentTheme.colors.background,
        fontFamily: currentTheme.typography.fontFamily 
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
                    ${currentTheme.id === theme.id 
                      ? 'ring-2 ring-white/50 shadow-lg transform scale-105' 
                      : 'hover:scale-105 opacity-70 hover:opacity-100'
                    }
                  `}
                  style={{
                    background: theme.colors.primary,
                    color: theme.colors.text
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
                    : currentTheme.colors.error
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
              fontWeight: currentTheme.typography.fontWeight.bold
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
              border: `1px solid ${currentTheme.colors.border}40`
            }}
          >
            <div className="text-center">
              <h2 
                className="text-2xl mb-2 transition-colors duration-300"
                style={{ 
                  color: currentTheme.colors.text,
                  fontWeight: currentTheme.typography.fontWeight.semibold
                }}
              >
                {currentTrack.title}
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
          <LyricsViewer 
            lyrics={lyrics} 
            currentTime={currentTime}
            isPlaying={isPlaying}
            className="mb-8"
          />
        )}

        {/* Media Controls */}
        {currentTrack && (
          <MediaControls
            currentTime={currentTime}
            duration={180} // 3 minutes demo
            isPlaying={isPlaying}
            onPlayPause={() => setIsPlaying(!isPlaying)}
            onSeek={seekTo}
          />
        )}

        {/* Loading State */}
        {isLoadingLyrics && (
          <div className="text-center text-white/70 mt-8">
            <div className="animate-pulse">
              <p className="text-lg mb-4">
                🔍 Searching for lyrics...
              </p>
            </div>
          </div>
        )}

        {/* Instructions */}
        {!currentTrack && !isLoadingLyrics && (
          <div className="text-center text-white/70 mt-12">
            <p className="text-lg mb-4">
              🎧 Play music in your browser to see lyrics here
            </p>
            <p className="text-sm">
              Supported: Spotify Web Player, YouTube Music
            </p>
          </div>
        )}
      </div>
    </div>
  );
}

export default App;