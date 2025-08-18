import React, { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { motion } from "framer-motion";
import { LyricLine, WordTiming } from "../stores/lyricsStore";
import { useThemeStore } from "../stores/themeStore";
import { useOffsetStore } from "../stores/offsetStore";
import { getLineDuration } from "../services/lyricsTiming";

interface LyricsViewerProps {
  lyrics: LyricLine[];
  currentTime: number;
  isPlaying: boolean;
  className?: string;
  artist?: string;
  title?: string;
}

const LyricsViewer: React.FC<LyricsViewerProps> = ({
  lyrics,
  currentTime,
  isPlaying,
  className = "",
  artist = "",
  title = "",
}) => {
  const { currentTheme } = useThemeStore();
  const { getTotalOffset } = useOffsetStore();
  const containerRef = useRef<HTMLDivElement>(null);
  const [autoScroll, setAutoScroll] = useState(true);
  const [wordHighlightEnabled, setWordHighlightEnabled] = useState(true);
  const ignoreScrollRef = useRef(false);
  const previousTimeRef = useRef(currentTime);
  const timeCheckRef = useRef(Date.now());

  // Apply offset to current time (all in seconds)
  const adjustedTime = currentTime + getTotalOffset(artist, title);

  // Detect if time is progressing (backup for when isPlaying might be incorrectly false)
  const now = Date.now();
  const timeDiff = Math.abs(currentTime - previousTimeRef.current);
  const realTimeDiff = now - timeCheckRef.current;
  const isTimeProgressing =
    timeDiff > 0.1 && realTimeDiff > 100 && realTimeDiff < 2000;
  const effectivelyPlaying = isPlaying || isTimeProgressing;

  // Update refs for next check
  previousTimeRef.current = currentTime;
  timeCheckRef.current = now;

  // Find current line index using adjusted time
  const getCurrentLineIndex = () => {
    for (let i = 0; i < lyrics.length; i++) {
      const currentLine = lyrics[i];
      const nextLine = lyrics[i + 1];

      if (
        adjustedTime >= currentLine.time &&
        (!nextLine || adjustedTime < nextLine.time)
      ) {
        return i;
      }
    }
    return -1;
  };

  const currentLineIndex = getCurrentLineIndex();

  // Auto-scroll to current line
  useEffect(() => {
    if (!autoScroll || currentLineIndex === -1) return;

    const container = containerRef.current;
    if (!container) return;

    const currentLineElement = container.querySelector(
      `[data-line-index="${currentLineIndex}"]`
    );
    if (!currentLineElement) return;

    // Mark programmatic scroll to avoid disabling auto-scroll
    ignoreScrollRef.current = true;
    currentLineElement.scrollIntoView({
      behavior: "smooth",
      block: "center",
      inline: "nearest",
    });
    // Reset the flag after the smooth scroll likely finishes
    const t = setTimeout(() => {
      ignoreScrollRef.current = false;
    }, 350);
    return () => clearTimeout(t);
  }, [currentLineIndex, autoScroll]);

  // Calculate line progress based on line duration
  const calculateLineProgress = (line: LyricLine, currentTime: number): number => {
    if (currentTime < line.time) return 0;

    const lineDuration = getLineDuration(line);
    const elapsed = currentTime - line.time;

    return Math.min(elapsed / lineDuration, 1);
  };

  // Calculate progress for a specific word within its own timing
  const calculateWordProgress = (word: WordTiming, currentTime: number): number => {
    if (currentTime < word.start) return 0;
    if (currentTime >= word.end) return 1;
    
    const wordDuration = word.end - word.start;
    if (wordDuration <= 0) return 1; // Handle zero duration words
    
    return (currentTime - word.start) / wordDuration;
  };

  // Calculate overall line progress when using word timings
  // This ensures word timing progress aligns with the line's duration
  const calculateWordBasedLineProgress = (line: LyricLine, currentTime: number): number => {
    if (!line.words || line.words.length === 0) {
      return calculateLineProgress(line, currentTime);
    }
    
    const lineDuration = getLineDuration(line);
    const lineStart = line.time;
    const lineEnd = lineStart + lineDuration;
    
    // Ensure all words fit within the line duration by normalizing their timings
    const normalizedWords = line.words.map(word => {
      // Calculate each word's relative position in the line (0-1)
      const originalStart = Math.max(word.start - lineStart, 0);
      const originalEnd = Math.min(word.end - lineStart, lineDuration);
      
      return {
        ...word,
        normalizedStart: originalStart / lineDuration,
        normalizedEnd: originalEnd / lineDuration,
        actualStart: lineStart + originalStart,
        actualEnd: lineStart + originalEnd
      };
    });
    
    // Calculate progress based on normalized word positions
    let completedProgress = 0;
    
    for (const word of normalizedWords) {
      if (currentTime < word.actualStart) {
        // Haven't reached this word yet
        break;
      } else if (currentTime >= word.actualEnd) {
        // Word is completely finished - add its full contribution
        completedProgress = word.normalizedEnd;
      } else {
        // Currently in this word - calculate partial progress
        const wordProgress = calculateWordProgress(
          { start: word.actualStart, end: word.actualEnd, word: word.word },
          currentTime
        );
        const wordContribution = word.normalizedStart + 
          (word.normalizedEnd - word.normalizedStart) * wordProgress;
        completedProgress = wordContribution;
        break;
      }
    }
    
    return Math.min(completedProgress, 1);
  };

  // Get current word index based on audio progress through the line
  const getCurrentWordIndex = (line: LyricLine, currentTime: number) => {
    if (!line.words || !line.words.length) return -1;

    // If before the line starts, return -1
    if (currentTime < line.time) return -1;
    
    const lineDuration = getLineDuration(line);
    const lineProgress = Math.min((currentTime - line.time) / lineDuration, 1);
    
    // If past the line end, show last word as completed
    if (lineProgress >= 1) return line.words.length - 1;
    
    // Use audio-based progress to determine word index
    // This ensures highlighting keeps pace with the actual audio
    const wordCount = line.words.length;
    const currentWordFloat = lineProgress * wordCount;
    // Adjust the calculation to be more responsive
    const audioBasedIndex = Math.min(Math.floor(currentWordFloat + 0.01), wordCount - 1);
    
    // Also check the original word timing as a fallback for very precise timing
    let timingBasedIndex = -1;
    for (let i = 0; i < line.words.length; i++) {
      const word = line.words[i];
      if (currentTime >= word.start && currentTime < word.end) {
        timingBasedIndex = i;
        break;
      }
    }
    
    // Use the more advanced index (favor audio-based for better sync)
    // But don't let it get too far ahead of timing-based
    if (timingBasedIndex >= 0 && audioBasedIndex > timingBasedIndex + 2) {
      // If audio-based is way ahead, moderate it slightly
      return Math.min(audioBasedIndex, timingBasedIndex + 2);
    }
    
    // Favor audio-based index, but use timing-based if it's valid and higher
    if (timingBasedIndex >= 0) {
      return Math.max(audioBasedIndex, timingBasedIndex);
    } else {
      return audioBasedIndex;
    }
  };

  // Render word-by-word highlighting
  const renderLineWithWordTiming = (line: LyricLine, isActive: boolean, currentTime: number) => {
    if (!line.words || !line.words.length || !isActive) {
      return line.text;
    }

    const currentWordIndex = getCurrentWordIndex(line, currentTime);

    return (
      <span className="inline-flex flex-wrap justify-center gap-1">
        {line.words.map((wordTiming, wordIndex) => {
          const isPastWord = wordIndex < currentWordIndex;
          const isCurrentWord = wordIndex === currentWordIndex;
          const isFutureWord = wordIndex > currentWordIndex;

          // Calculate progress within current word using audio-based timing
          let wordProgress = 0;
          if (isPastWord) {
            wordProgress = 1;
          } else if (isCurrentWord) {
            // Use audio-based progress for more responsive highlighting
            const lineDuration = getLineDuration(line);
            const lineProgress = Math.min((currentTime - line.time) / lineDuration, 1);
            const wordCount = line.words!.length;
            
            // Calculate what portion of the line this word should represent
            const wordStartPercent = wordIndex / wordCount;
            const wordEndPercent = (wordIndex + 1) / wordCount;
            
            if (lineProgress <= wordStartPercent) {
              wordProgress = 0;
            } else if (lineProgress >= wordEndPercent) {
              wordProgress = 1;
            } else {
              // Interpolate progress within this word based on line progress
              wordProgress = (lineProgress - wordStartPercent) / (wordEndPercent - wordStartPercent);
            }
            
            // Ensure minimum visible progress for very quick transitions
            wordProgress = Math.max(wordProgress, 0.05);
          } else {
            // Future word - check if we're very close to starting based on line progress
            const lineDuration = getLineDuration(line);
            const lineProgress = Math.min((currentTime - line.time) / lineDuration, 1);
            const wordCount = line.words!.length;
            const wordStartPercent = wordIndex / wordCount;
            const timeUntilWordPercent = wordStartPercent - lineProgress;
            
            if (timeUntilWordPercent < 0.05) { // Within 5% of line time to this word
              wordProgress = Math.max(0, 0.1 + (0.05 - timeUntilWordPercent) / 0.05 * 0.2);
            }
          }

          // Calculate if word is upcoming based on line progress
          const lineDuration = getLineDuration(line);
          const lineProgress = Math.min((currentTime - line.time) / lineDuration, 1);
          const wordCount = line.words!.length;
          const wordStartPercent = wordIndex / wordCount;
          const timeUntilWordPercent = wordStartPercent - lineProgress;
          const isUpcoming = isFutureWord && timeUntilWordPercent < 0.05; // Within 5% of line time

          return (
            <motion.span
              key={wordIndex}
              className={`
                relative transition-all duration-150
                ${
                  isPastWord
                    ? "text-blue-300"
                    : isCurrentWord
                    ? "text-white font-semibold"
                    : isUpcoming
                    ? "text-white/80" // Highlight upcoming words
                    : "text-white/60"
                }
              `}
              animate={{
                scale: isCurrentWord ? 1.05 : isUpcoming ? 1.02 : 1,
                y: isCurrentWord ? -2 : 0,
              }}
              transition={{ duration: 0.15, ease: "easeOut" }}
            >
              {isCurrentWord ? (
                <span className="relative">
                  {/* Background word */}
                  <span className="text-white/40">{wordTiming.word}</span>
                  {/* Progressive highlight with smoother animation */}
                  <span
                    className="absolute inset-0 text-white overflow-hidden"
                    style={{
                      clipPath: `inset(0 ${Math.max(0, (1 - wordProgress) * 100)}% 0 0)`,
                      transition: wordProgress > 0.05 ? 'clip-path 0.05s ease-out' : 'none'
                    }}
                  >
                    {wordTiming.word}
                  </span>
                </span>
              ) : isPastWord ? (
                <span className="relative">
                  {/* Completed word with subtle glow effect */}
                  <span className="text-blue-300 drop-shadow-sm">{wordTiming.word}</span>
                </span>
              ) : isUpcoming ? (
                <span className="relative">
                  {/* Upcoming word with subtle preparation effect */}
                  <span className="text-white/80">{wordTiming.word}</span>
                </span>
              ) : (
                wordTiming.word
              )}
            </motion.span>
          );
        })}
      </span>
    );
  };

  if (!lyrics.length) {
    return (
      <div
        className={`flex items-center justify-center min-h-[400px] ${className}`}
      >
        <div className="text-center">
          <div className="text-6xl mb-4">🎵</div>
          <div
            className="text-lg mb-2"
            style={{ color: `${currentTheme.colors.textMuted}` }}
          >
            No lyrics available
          </div>
          <div
            className="text-sm"
            style={{ color: `${currentTheme.colors.textMuted}80` }}
          >
            Searching for synchronized lyrics...
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className={`relative ${className}`}>
      {/* Auto-scroll toggle and time debug */}
      <div className="absolute top-4 right-4 z-10 space-y-2">
        <button
          onClick={() => setAutoScroll(!autoScroll)}
          className="block px-3 py-1 rounded-full text-xs font-medium transition-all duration-300 hover:scale-105"
          style={{
            backgroundColor: autoScroll
              ? `${currentTheme.colors.primary}40`
              : `${currentTheme.colors.backgroundSecondary}60`,
            color: autoScroll
              ? currentTheme.colors.text
              : currentTheme.colors.textMuted,
            border: `1px solid ${currentTheme.colors.border}30`,
          }}
        >
          {autoScroll ? "🔒 Auto-scroll" : "🔓 Manual"}
        </button>

        {/* Toggle word-level alignment/highlight for debug */}
        <button
          onClick={() => setWordHighlightEnabled((v) => !v)}
          className="block px-3 py-1 rounded-full text-xs font-medium transition-all duration-300 hover:scale-105"
          style={{
            backgroundColor: wordHighlightEnabled
              ? `${currentTheme.colors.success}30`
              : `${currentTheme.colors.backgroundSecondary}60`,
            color: wordHighlightEnabled
              ? currentTheme.colors.text
              : currentTheme.colors.textMuted,
            border: `1px solid ${currentTheme.colors.border}30`,
          }}
          title="Enable/disable word alignment highlight"
        >
          {wordHighlightEnabled ? "🟢 Word Align: On" : "⚪ Word Align: Off"}
        </button>

        {/* Debug time display */}
        <div className="text-xs text-white/50 font-mono bg-black/30 px-2 py-1 rounded space-y-1">
          <div>Time: {currentTime.toFixed(1)}s</div>
          <div>Adj: {adjustedTime.toFixed(1)}s</div>
          <div>
            Line: {currentLineIndex + 1}/{lyrics.length}
          </div>
          <div
            className={
              effectivelyPlaying ? "text-green-400" : "text-yellow-400"
            }
          >
            {effectivelyPlaying ? "▶ Playing" : "⏸ Paused"}
            {isTimeProgressing && !isPlaying && (
              <span className="text-xs text-blue-300 ml-1">(auto)</span>
            )}
          </div>
        </div>
      </div>

      {/* Lyrics container */}
      <div
        ref={containerRef}
        className="max-h-[500px] overflow-y-auto scrollbar-thin scrollbar-thumb-white/20 scrollbar-track-transparent"
        onScroll={() => {
          if (!ignoreScrollRef.current) setAutoScroll(false);
        }}
      >
        <div className="space-y-6 py-8 px-4">
          {lyrics.map((line, index) => {
            const isActive = index === currentLineIndex;
            const isPast = index < currentLineIndex;
            const isFuture = index > currentLineIndex;
            // Calculate appropriate progress based on whether word timing is enabled
            const hasWordTiming = !!(line.words && line.words.length > 0 && wordHighlightEnabled);
            const lineProgress = isActive
              ? (hasWordTiming 
                  ? calculateWordBasedLineProgress(line, adjustedTime)
                  : calculateLineProgress(line, adjustedTime))
              : (index < currentLineIndex ? 1 : 0);

            return (
              <motion.div
                key={index}
                data-line-index={index}
                initial={{ opacity: 0, y: 20 }}
                animate={{
                  opacity: isActive ? 1 : isFuture ? 0.4 : 0.7,
                  y: 0,
                  scale: isActive ? 1.02 : 1,
                }}
                transition={{
                  duration: 0.3,
                  ease: "easeInOut",
                }}
                className={`
                  relative text-center cursor-pointer transition-all duration-300
                  ${
                    isActive
                      ? "text-white text-2xl font-semibold"
                      : isPast
                      ? "text-blue-200 text-lg"
                      : "text-white/50 text-lg"
                  }
                  hover:text-white/80
                `}
                onClick={async () => {
                  try {
                    await invoke("send_playback_command", {
                      command: "seek",
                      seekTime: line.time,
                    });
                  } catch (e) {
                    console.error("Failed to seek to line", e);
                  }
                }}
              >
                {/* Background highlight for current line */}
                {isActive && (
                  <motion.div
                    initial={{ opacity: 0, scale: 0.8 }}
                    animate={{ opacity: 1, scale: 1 }}
                    exit={{ opacity: 0, scale: 0.8 }}
                    className="absolute inset-0 bg-white/10 rounded-xl -z-10"
                  />
                )}

                {/* Enhanced word-level or line-level highlighting */}
                {line.words && line.words.length > 0 && wordHighlightEnabled ? (
                  renderLineWithWordTiming(line, isActive, adjustedTime)
                ) : isActive ? (
                  <div className="relative">
                    {/* Background text */}
                    <div className="text-white/30">{line.text}</div>
                    {/* Highlighted text - using line progress for non-word timing */}
                    <div
                      className="absolute inset-0 text-white overflow-hidden transition-all duration-100"
                      style={{
                        clipPath: `inset(0 ${(1 - lineProgress) * 100}% 0 0)`,
                      }}
                    >
                      {line.text}
                    </div>
                  </div>
                ) : (
                  line.text
                )}

                {/* Timing indicator */}
                {isActive && (
                  <div className="mt-2 flex justify-center">
                    <div className="w-32 h-1 bg-white/20 rounded-full overflow-hidden">
                      <motion.div
                        className="h-full bg-gradient-to-r from-blue-400 to-purple-400"
                        initial={{ width: 0 }}
                        animate={{
                          width: `${lineProgress * 100}%`,
                        }}
                        transition={{ duration: 0.1 }}
                      />
                    </div>
                  </div>
                )}
              </motion.div>
            );
          })}
        </div>
      </div>

      {/* Progress indicator dots */}
      <div className="flex justify-center mt-6 space-x-1">
        {lyrics.slice(0, Math.min(lyrics.length, 20)).map((_, index) => (
          <div
            key={index}
            className={`
              w-1.5 h-1.5 rounded-full transition-all duration-300 cursor-pointer
              ${
                index === currentLineIndex
                  ? "bg-white scale-125 shadow-lg"
                  : index < currentLineIndex
                  ? "bg-blue-400"
                  : "bg-white/30 hover:bg-white/50"
              }
            `}
            onClick={() => {
              // TODO: Implement seek to line
              console.log(`Seek to line ${index}`);
            }}
          />
        ))}
        {lyrics.length > 20 && (
          <div className="text-white/50 text-xs ml-2">
            +{lyrics.length - 20} more
          </div>
        )}
      </div>
    </div>
  );
};

export default LyricsViewer;
