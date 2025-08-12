import { describe, it, expect } from 'vitest';

// Mock the actual component calculations
interface WordTiming {
  start: number;
  end: number;
  word: string;
}

interface LyricLine {
  time: number;
  text: string;
  duration?: number;
  words?: WordTiming[];
}

// Replicate the getWordProgress function from the component
const getWordProgress = (line: LyricLine, adjustedTime: number) => {
  const lineProgress = Math.min(
    (adjustedTime - line.time) / (line.duration || 3000),
    1
  );
  return Math.max(0, lineProgress);
};

// Replicate the getCurrentWordIndex function
const getCurrentWordIndex = (line: LyricLine, adjustedTime: number) => {
  if (!line.words || !line.words.length) return -1;

  for (let i = 0; i < line.words.length; i++) {
    const word = line.words[i];
    if (adjustedTime >= word.start && adjustedTime < word.end) {
      return i;
    }
  }
  return -1;
};

// Replicate the word progress calculation from the component
const calculateWordProgress = (
  line: LyricLine,
  wordTiming: WordTiming,
  wordIndex: number,
  currentWordIndex: number,
  lineProgress: number
) => {
  const isPastWord = wordIndex < currentWordIndex;
  const isCurrentWord = wordIndex === currentWordIndex;

  let wordProgress = 0;
  if (isPastWord) {
    wordProgress = 1;
  } else if (isCurrentWord) {
    const lineDuration = line.duration || 3000;
    const wordStartRatio = (wordTiming.start - line.time) / lineDuration;
    const wordEndRatio = (wordTiming.end - line.time) / lineDuration;
    const wordDurationRatio = wordEndRatio - wordStartRatio;
    
    if (lineProgress >= wordStartRatio) {
      if (wordDurationRatio > 0) {
        wordProgress = Math.min(1, (lineProgress - wordStartRatio) / wordDurationRatio);
      } else {
        wordProgress = 1;
      }
    }
  }
  return wordProgress;
};

describe('Progress Synchronization Tests', () => {
  it('should synchronize word progress with line progress bar', () => {
    const line: LyricLine = {
      time: 10000, // 10s
      duration: 4000, // 4s duration
      text: 'Hello beautiful world',
      words: [
        { start: 10000, end: 11000, word: 'Hello' },     // 0-1s (25% of line)
        { start: 11000, end: 13000, word: 'beautiful' }, // 1-3s (50% of line)
        { start: 13000, end: 14000, word: 'world' }      // 3-4s (25% of line)
      ]
    };

    // Test at different time points
    const testPoints = [
      { time: 10000, expectedLineProgress: 0, expectedWordIndex: 0 },    // Start
      { time: 10500, expectedLineProgress: 0.125, expectedWordIndex: 0 }, // 12.5% - middle of first word
      { time: 11000, expectedLineProgress: 0.25, expectedWordIndex: 1 },  // 25% - start of second word
      { time: 12000, expectedLineProgress: 0.5, expectedWordIndex: 1 },   // 50% - middle of second word
      { time: 13000, expectedLineProgress: 0.75, expectedWordIndex: 2 },  // 75% - start of third word
      { time: 13500, expectedLineProgress: 0.875, expectedWordIndex: 2 }, // 87.5% - middle of third word
      { time: 14000, expectedLineProgress: 1, expectedWordIndex: -1 }     // 100% - end of line
    ];

    testPoints.forEach(({ time, expectedLineProgress, expectedWordIndex }) => {
      const lineProgress = getWordProgress(line, time);
      const currentWordIndex = getCurrentWordIndex(line, time);
      
      // Line progress should match expected
      expect(lineProgress).toBeCloseTo(expectedLineProgress, 3);
      
      // Word index should match (handle end-of-line case)
      if (expectedWordIndex >= 0) {
        expect(currentWordIndex).toBe(expectedWordIndex);
      }
      
      // Test word progress calculation for each word
      line.words.forEach((wordTiming, wordIndex) => {
        const wordProgress = calculateWordProgress(
          line, 
          wordTiming, 
          wordIndex, 
          currentWordIndex, 
          lineProgress
        );
        
        // Word progress should be valid (0-1)
        expect(wordProgress).toBeGreaterThanOrEqual(0);
        expect(wordProgress).toBeLessThanOrEqual(1);
        
        if (wordIndex < currentWordIndex) {
          // Past words should be fully highlighted
          expect(wordProgress).toBe(1);
        } else if (wordIndex === currentWordIndex) {
          // Current word progress should correlate with line progress
          const wordStartRatio = (wordTiming.start - line.time) / line.duration;
          const wordEndRatio = (wordTiming.end - line.time) / line.duration;
          
          if (lineProgress >= wordStartRatio) {
            // Word progress should be >= 0, and > 0 only if we're actually into the word
            expect(wordProgress).toBeGreaterThanOrEqual(0);
            
            if (lineProgress > wordStartRatio) {
              // Only expect progress > 0 if we're past the word start
              expect(wordProgress).toBeGreaterThan(0);
            }
            
            if (lineProgress >= wordEndRatio) {
              expect(wordProgress).toBe(1);
            }
          } else {
            expect(wordProgress).toBe(0);
          }
        } else {
          // Future words should not be highlighted
          expect(wordProgress).toBe(0);
        }
      });
    });
  });

  it('should handle edge cases in word timing', () => {
    const line: LyricLine = {
      time: 10000,
      duration: 2000,
      text: 'Quick test',
      words: [
        { start: 10000, end: 10000, word: 'Quick' }, // Zero duration word
        { start: 10000, end: 12000, word: 'test' }   // Full duration word
      ]
    };

    const time = 11000; // 50% through line
    const lineProgress = getWordProgress(line, time);
    const currentWordIndex = getCurrentWordIndex(line, time);

    // Should handle zero-duration word correctly
    const zeroWordProgress = calculateWordProgress(
      line,
      line.words[0],
      0,
      currentWordIndex,
      lineProgress
    );

    expect(zeroWordProgress).toBe(1); // Zero duration word should be fully highlighted when past it
    expect(isNaN(zeroWordProgress)).toBe(false);
  });

  it('should maintain consistent progress when words have gaps', () => {
    const line: LyricLine = {
      time: 10000,
      duration: 4000,
      text: 'Word with gaps',
      words: [
        { start: 10000, end: 11000, word: 'Word' },  // 0-1s
        // Gap from 1-2s
        { start: 12000, end: 13000, word: 'with' },  // 2-3s
        // Gap from 3-3.5s
        { start: 13500, end: 14000, word: 'gaps' }   // 3.5-4s
      ]
    };

    // Test during gap periods
    const timeInGap1 = 11500; // 1.5s - between first and second word
    const timeInGap2 = 13250; // 3.25s - between second and third word

    [timeInGap1, timeInGap2].forEach(time => {
      const lineProgress = getWordProgress(line, time);
      const currentWordIndex = getCurrentWordIndex(line, time);
      
      // Should still calculate valid line progress
      expect(lineProgress).toBeGreaterThan(0);
      expect(lineProgress).toBeLessThan(1);
      
      // Current word index should be -1 (no active word)
      expect(currentWordIndex).toBe(-1);
      
      // Word progress calculations should still be valid
      line.words.forEach((wordTiming, wordIndex) => {
        const wordProgress = calculateWordProgress(
          line,
          wordTiming,
          wordIndex,
          currentWordIndex,
          lineProgress
        );
        
        expect(wordProgress).toBeGreaterThanOrEqual(0);
        expect(wordProgress).toBeLessThanOrEqual(1);
      });
    });
  });
});