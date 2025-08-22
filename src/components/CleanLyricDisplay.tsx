import React from 'react';
import { motion } from 'framer-motion';
import { LyricLine, WordTiming } from '../stores/lyricsStore';

interface CleanLyricDisplayProps {
  line: LyricLine;
  currentTime: number;
  adjustedTime: number;
  fontFamily: string;
}

const CleanLyricDisplay: React.FC<CleanLyricDisplayProps> = ({
  line,
  currentTime,
  adjustedTime,
  fontFamily
}) => {
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
    
    return (
      <div className="inline-flex flex-wrap justify-center gap-2">
        {line.words.map((wordTiming, wordIndex) => {
          const isPastWord = wordIndex < currentWordIndex;
          const isCurrentWord = wordIndex === currentWordIndex;
          const wordProgress = isCurrentWord ? calculateWordProgress(wordTiming, adjustedTime) : 0;

          return (
            <motion.span
              key={wordIndex}
              className={`
                relative transition-all duration-200
                ${isPastWord ? "text-blue-300" : isCurrentWord ? "text-white" : "text-white/60"}
              `}
              animate={{
                scale: isCurrentWord ? 1.05 : 1,
                y: isCurrentWord ? -4 : 0,
              }}
              transition={{ duration: 0.2, ease: "easeOut" }}
              style={{ fontFamily }}
            >
              {isCurrentWord ? (
                <span className="relative">
                  {/* Background word */}
                  <span className="text-white/30">{wordTiming.word}</span>
                  {/* Progressive highlight */}
                  <span
                    className="absolute inset-0 text-white overflow-hidden"
                    style={{
                      clipPath: `inset(0 ${Math.max(0, (1 - wordProgress) * 100)}% 0 0)`,
                      transition: wordProgress > 0.05 ? 'clip-path 0.1s ease-out' : 'none'
                    }}
                  >
                    {wordTiming.word}
                  </span>
                </span>
              ) : isPastWord ? (
                <span className="text-blue-300 drop-shadow-sm">{wordTiming.word}</span>
              ) : (
                wordTiming.word
              )}
            </motion.span>
          );
        })}
      </div>
    );
  }

  // Fallback to simple line highlighting
  const lineProgress = Math.min((adjustedTime - line.time) / 3, 1); // Assume 3s duration

  return (
    <div className="relative" style={{ fontFamily }}>
      {/* Background text */}
      <div className="text-white/30">{line.text}</div>
      {/* Progressive highlight */}
      <div
        className="absolute inset-0 text-white overflow-hidden transition-all duration-200"
        style={{
          clipPath: `inset(0 ${(1 - lineProgress) * 100}% 0 0)`,
        }}
      >
        {line.text}
      </div>
    </div>
  );
};

export default CleanLyricDisplay;