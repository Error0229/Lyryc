import React from 'react';
import { motion } from 'framer-motion';
import { LyricLine, WordTiming } from '../stores/lyricsStore';
import { useLyricsStyleStore } from '../stores/lyricsStyleStore';

interface CleanLyricDisplayProps {
  line: LyricLine;
  currentTime: number;
  adjustedTime: number;
  fontFamily: string;
  viewMode?: 'highlighted' | 'plain';
}

const CleanLyricDisplay: React.FC<CleanLyricDisplayProps> = ({
  line,
  currentTime,
  adjustedTime,
  fontFamily, // Keep for backward compatibility, but will be overridden by style store
  viewMode = 'highlighted'
}) => {
  const { style } = useLyricsStyleStore();
  
  // Create dynamic styles based on user preferences
  const baseTextStyle = {
    fontFamily: style.fontFamily,
    fontSize: `${style.fontSize}rem`,
    fontWeight: style.fontWeight,
    color: style.textColor,
    lineHeight: style.lineHeight,
    letterSpacing: `${style.letterSpacing}em`,
    textAlign: style.textAlign as 'left' | 'center' | 'right',
    textShadow: style.textShadow 
      ? `0 0 ${style.textShadowBlur}px ${style.textShadowColor}` 
      : 'none',
    filter: style.textGlow 
      ? `drop-shadow(0 0 10px ${style.textGlowColor})` 
      : 'none',
  };

  const backgroundStyle = style.backgroundOpacity > 0 ? {
    backgroundColor: `${style.backgroundColor}${Math.round(style.backgroundOpacity * 2.55).toString(16).padStart(2, '0')}`,
    backdropFilter: style.backgroundBlur > 0 ? `blur(${style.backgroundBlur}px)` : 'none',
    borderRadius: '12px',
    padding: '16px 24px',
  } : {};

  // If plain mode, just render the text without any highlighting
  if (viewMode === 'plain') {
    return (
      <div style={{ ...backgroundStyle }}>
        <div style={baseTextStyle}>
          {line.text}
        </div>
      </div>
    );
  }
  // Calculate word progress for clean word-by-word highlighting
  const calculateWordProgress = (word: WordTiming, currentTime: number): number => {
    if (currentTime < word.start) return 0;
    if (currentTime >= word.end) return 1;
    
    const wordDuration = word.end - word.start;
    if (wordDuration <= 0) return 1;
    
    return (currentTime - word.start) / wordDuration;
  };

  // Get current word index based on timing
  const getCurrentWordIndex = (words: WordTiming[], currentTime: number): number => {
    for (let i = 0; i < words.length; i++) {
      const word = words[i];
      if (currentTime >= word.start && currentTime < word.end) {
        return i;
      }
    }
    
    // If we're past all words, highlight the last one
    if (currentTime >= words[words.length - 1].end) {
      return words.length - 1;
    }
    
    return -1;
  };

  // Render with word-level highlighting if available
  if (line.words && line.words.length > 0) {
    const currentWordIndex = getCurrentWordIndex(line.words, adjustedTime);
    
    // Debug offset application
    if (Math.abs(currentTime - adjustedTime) > 0.1) {
      console.log(`[CleanLyricDisplay] Offset active: currentTime=${currentTime.toFixed(2)}s, adjustedTime=${adjustedTime.toFixed(2)}s, offset=${(adjustedTime - currentTime).toFixed(2)}s`);
    }
    
    return (
      <div style={{ ...backgroundStyle }}>
        <div 
          className="inline-flex flex-wrap gap-2"
          style={{ justifyContent: style.textAlign }}
        >
          {line.words.map((wordTiming, wordIndex) => {
            const isPastWord = wordIndex < currentWordIndex;
            const isCurrentWord = wordIndex === currentWordIndex;
            const wordProgress = isCurrentWord ? calculateWordProgress(wordTiming, adjustedTime) : 0;
            
            const animationScale = isCurrentWord ? 1 + (style.animationIntensity / 1000) : 1;
            const animationY = isCurrentWord ? -(style.animationIntensity / 25) : 0;

            return (
              <motion.span
                key={wordIndex}
                className="relative transition-all duration-200"
                animate={{
                  scale: animationScale,
                  y: animationY,
                }}
                transition={{ duration: 0.2, ease: "easeOut" }}
                style={baseTextStyle}
              >
                {isCurrentWord ? (
                  <span className="relative">
                    {/* Background word */}
                    <span style={{ color: `${style.futureWordColor}60` }}>{wordTiming.word}</span>
                    {/* Progressive highlight */}
                    <span
                      className="absolute inset-0 overflow-hidden"
                      style={{
                        color: style.highlightColor,
                        clipPath: `inset(0 ${Math.max(0, (1 - wordProgress) * 100)}% 0 0)`,
                        transition: wordProgress > 0.05 ? 'clip-path 0.1s ease-out' : 'none',
                        filter: style.textGlow 
                          ? `drop-shadow(0 0 10px ${style.textGlowColor})` 
                          : 'none',
                      }}
                    >
                      {wordTiming.word}
                    </span>
                  </span>
                ) : isPastWord ? (
                  <span style={{ color: style.pastWordColor }}>{wordTiming.word}</span>
                ) : (
                  <span style={{ color: style.futureWordColor }}>{wordTiming.word}</span>
                )}
              </motion.span>
            );
          })}
        </div>
      </div>
    );
  }

  // Fallback to simple line highlighting
  const lineProgress = Math.min((adjustedTime - line.time) / 3, 1); // Assume 3s duration

  return (
    <div style={{ ...backgroundStyle }}>
      <div className="relative" style={baseTextStyle}>
        {/* Background text */}
        <div style={{ color: `${style.futureWordColor}60` }}>{line.text}</div>
        {/* Progressive highlight */}
        <div
          className="absolute inset-0 overflow-hidden transition-all duration-200"
          style={{
            color: style.highlightColor,
            clipPath: `inset(0 ${(1 - lineProgress) * 100}% 0 0)`,
            filter: style.textGlow 
              ? `drop-shadow(0 0 10px ${style.textGlowColor})` 
              : 'none',
          }}
        >
          {line.text}
        </div>
      </div>
    </div>
  );
};

export default CleanLyricDisplay;