import React from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { LyricLine } from '../stores/lyricsStore';

interface LyricsDisplayProps {
  lyrics: LyricLine[];
  currentTime: number;
}

const LyricsDisplay: React.FC<LyricsDisplayProps> = ({ lyrics, currentTime }) => {
  // Find current line based on time
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

  if (!lyrics.length) {
    return (
      <div className="text-center py-12">
        <div className="text-white/50 text-lg">
          🎵 No lyrics available
        </div>
        <div className="text-white/30 text-sm mt-2">
          Searching for synchronized lyrics...
        </div>
      </div>
    );
  }

  return (
    <div className="lyrics-container max-w-4xl mx-auto">
      <div className="bg-black/20 backdrop-blur-sm rounded-2xl p-8 min-h-[400px]">
        <div className="space-y-4">
          {lyrics.map((line, index) => {
            const isActive = index === currentLineIndex;
            const isPast = index < currentLineIndex;
            const isFuture = index > currentLineIndex;

            return (
              <AnimatePresence key={index}>
                <motion.div
                  initial={{ opacity: 0, y: 20 }}
                  animate={{ 
                    opacity: isActive ? 1 : isFuture ? 0.4 : 0.6,
                    y: 0,
                    scale: isActive ? 1.05 : 1,
                  }}
                  exit={{ opacity: 0, y: -20 }}
                  transition={{ 
                    duration: 0.3,
                    ease: "easeInOut"
                  }}
                  className={`
                    text-center transition-all duration-300 py-2 px-4 rounded-lg
                    ${isActive 
                      ? 'text-white text-2xl font-semibold bg-white/10 shadow-lg' 
                      : isPast 
                        ? 'text-blue-200 text-lg' 
                        : 'text-white/50 text-lg'
                    }
                  `}
                >
                  {line.text}
                </motion.div>
              </AnimatePresence>
            );
          })}
        </div>
      </div>

      {/* Progress indicator */}
      {lyrics.length > 0 && (
        <div className="mt-4 flex justify-center">
          <div className="flex space-x-1">
            {lyrics.map((_, index) => (
              <div
                key={index}
                className={`
                  w-2 h-2 rounded-full transition-all duration-300
                  ${index === currentLineIndex 
                    ? 'bg-white scale-125' 
                    : index < currentLineIndex 
                      ? 'bg-blue-400' 
                      : 'bg-white/30'
                  }
                `}
              />
            ))}
          </div>
        </div>
      )}
    </div>
  );
};

export default LyricsDisplay;