import React, { useState, useEffect, useRef } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { LyricLine } from '../stores/lyricsStore';
import { useThemeStore } from '../stores/themeStore';

interface LyricsViewerProps {
  lyrics: LyricLine[];
  currentTime: number;
  isPlaying: boolean;
  className?: string;
}

const LyricsViewer: React.FC<LyricsViewerProps> = ({ 
  lyrics, 
  currentTime, 
  isPlaying,
  className = '' 
}) => {
  const { currentTheme } = useThemeStore();
  const containerRef = useRef<HTMLDivElement>(null);
  const [autoScroll, setAutoScroll] = useState(true);

  // Find current line index
  const getCurrentLineIndex = () => {
    for (let i = 0; i < lyrics.length; i++) {
      const currentLine = lyrics[i];
      const nextLine = lyrics[i + 1];
      
      if (currentTime >= currentLine.time && 
          (!nextLine || currentTime < nextLine.time)) {
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

    const currentLineElement = container.querySelector(`[data-line-index="${currentLineIndex}"]`);
    if (!currentLineElement) return;

    currentLineElement.scrollIntoView({
      behavior: 'smooth',
      block: 'center',
      inline: 'nearest'
    });
  }, [currentLineIndex, autoScroll]);

  // Calculate word-level progress for current line
  const getWordProgress = (line: LyricLine, lineIndex: number) => {
    if (lineIndex !== currentLineIndex) return 1;
    
    const lineProgress = Math.min(
      (currentTime - line.time) / (line.duration || 3000),
      1
    );
    
    return Math.max(0, lineProgress);
  };

  // Get current word index for word-level highlighting
  const getCurrentWordIndex = (line: LyricLine) => {
    if (!line.words || !line.words.length) return -1;
    
    for (let i = 0; i < line.words.length; i++) {
      const word = line.words[i];
      if (currentTime >= word.start && currentTime < word.end) {
        return i;
      }
    }
    return -1;
  };

  // Render word-by-word highlighting
  const renderLineWithWordTiming = (line: LyricLine, isActive: boolean) => {
    if (!line.words || !line.words.length || !isActive) {
      return line.text;
    }

    const currentWordIndex = getCurrentWordIndex(line);
    
    return (
      <span className="inline-flex flex-wrap justify-center gap-1">
        {line.words.map((wordTiming, wordIndex) => {
          const isPastWord = wordIndex < currentWordIndex;
          const isCurrentWord = wordIndex === currentWordIndex;
          const isFutureWord = wordIndex > currentWordIndex;
          
          // Calculate progress within current word
          let wordProgress = 0;
          if (isCurrentWord) {
            const wordDuration = wordTiming.end - wordTiming.start;
            wordProgress = Math.min(
              (currentTime - wordTiming.start) / wordDuration,
              1
            );
          } else if (isPastWord) {
            wordProgress = 1;
          }

          return (
            <motion.span
              key={wordIndex}
              className={`
                relative transition-all duration-200 
                ${isPastWord 
                  ? 'text-blue-300' 
                  : isCurrentWord 
                    ? 'text-white font-semibold' 
                    : 'text-white/60'
                }
              `}
              animate={{
                scale: isCurrentWord ? 1.05 : 1,
                y: isCurrentWord ? -2 : 0,
              }}
              transition={{ duration: 0.2 }}
            >
              {isCurrentWord && isPlaying ? (
                <span className="relative">
                  {/* Background word */}
                  <span className="text-white/40">{wordTiming.word}</span>
                  {/* Progressive highlight */}
                  <span 
                    className="absolute inset-0 text-white overflow-hidden transition-all duration-100"
                    style={{
                      clipPath: `inset(0 ${(1 - wordProgress) * 100}% 0 0)`
                    }}
                  >
                    {wordTiming.word}
                  </span>
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
      <div className={`flex items-center justify-center min-h-[400px] ${className}`}>
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
      {/* Auto-scroll toggle */}
      <div className="absolute top-4 right-4 z-10">
        <button
          onClick={() => setAutoScroll(!autoScroll)}
          className="px-3 py-1 rounded-full text-xs font-medium transition-all duration-300 hover:scale-105"
          style={{
            backgroundColor: autoScroll 
              ? `${currentTheme.colors.primary}40` 
              : `${currentTheme.colors.backgroundSecondary}60`,
            color: autoScroll 
              ? currentTheme.colors.text 
              : currentTheme.colors.textMuted,
            border: `1px solid ${currentTheme.colors.border}30`
          }}
        >
          {autoScroll ? '🔒 Auto-scroll' : '🔓 Manual'}
        </button>
      </div>

      {/* Lyrics container */}
      <div 
        ref={containerRef}
        className="max-h-[500px] overflow-y-auto scrollbar-thin scrollbar-thumb-white/20 scrollbar-track-transparent"
        onScroll={() => setAutoScroll(false)}
      >
        <div className="space-y-6 py-8 px-4">
          {lyrics.map((line, index) => {
            const isActive = index === currentLineIndex;
            const isPast = index < currentLineIndex;
            const isFuture = index > currentLineIndex;
            const wordProgress = getWordProgress(line, index);

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
                  ease: "easeInOut"
                }}
                className={`
                  relative text-center cursor-pointer transition-all duration-300
                  ${isActive 
                    ? 'text-white text-2xl font-semibold' 
                    : isPast 
                      ? 'text-blue-200 text-lg' 
                      : 'text-white/50 text-lg'
                  }
                  hover:text-white/80
                `}
                onClick={() => {
                  // TODO: Implement seek functionality
                  console.log(`Seek to ${line.time}s`);
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
                {line.words && line.words.length > 0 ? (
                  renderLineWithWordTiming(line, isActive)
                ) : isActive && isPlaying ? (
                  <div className="relative">
                    {/* Background text */}
                    <div className="text-white/30">
                      {line.text}
                    </div>
                    {/* Highlighted text */}
                    <div 
                      className="absolute inset-0 text-white overflow-hidden transition-all duration-100"
                      style={{
                        clipPath: `inset(0 ${(1 - wordProgress) * 100}% 0 0)`
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
                          width: `${wordProgress * 100}%` 
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
              ${index === currentLineIndex 
                ? 'bg-white scale-125 shadow-lg' 
                : index < currentLineIndex 
                  ? 'bg-blue-400' 
                  : 'bg-white/30 hover:bg-white/50'
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