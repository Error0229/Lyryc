import { describe, it, expect } from 'vitest';
import { LyricLine, WordTiming } from '../src/stores/lyricsStore';

// Simulate the improved getCurrentWordIndex logic
function getCurrentWordIndex(line: LyricLine, currentTime: number): number {
  if (!line.words || !line.words.length) return -1;

  // If before the line starts, return -1
  if (currentTime < line.time) return -1;
  
  const lineDuration = line.duration ?? 3;
  const lineProgress = Math.min((currentTime - line.time) / lineDuration, 1);
  
  // If past the line end, show last word as completed
  if (lineProgress >= 1) return line.words.length - 1;
  
  // Use audio-based progress to determine word index
  const wordCount = line.words.length;
  const currentWordFloat = lineProgress * wordCount;
  // Adjust the calculation to be more responsive
  const audioBasedIndex = Math.min(Math.floor(currentWordFloat + 0.01), wordCount - 1);
  
  // Also check the original word timing as a fallback
  let timingBasedIndex = -1;
  for (let i = 0; i < line.words.length; i++) {
    const word = line.words[i];
    if (currentTime >= word.start && currentTime < word.end) {
      timingBasedIndex = i;
      break;
    }
  }
  
  // Use the more advanced index (favor audio-based for better sync)
  if (timingBasedIndex >= 0 && audioBasedIndex > timingBasedIndex + 2) {
    return Math.min(audioBasedIndex, timingBasedIndex + 2);
  }
  
  // Favor audio-based index, but use timing-based if it's valid and higher
  if (timingBasedIndex >= 0) {
    return Math.max(audioBasedIndex, timingBasedIndex);
  } else {
    return audioBasedIndex;
  }
}

describe('Improved Word Highlighting', () => {
  const testLine: LyricLine = {
    time: 48.6,
    text: "Never gonna give you up",
    duration: 2.8,
    words: [
      { start: 48.6, end: 49.2, word: "Never" },   // Original timing allocation
      { start: 49.2, end: 49.86, word: "gonna" },
      { start: 49.86, end: 50.43, word: "give" },
      { start: 50.43, end: 50.97, word: "you" },
      { start: 50.97, end: 51.4, word: "up" }
    ]
  };

  describe('Audio-based progress should solve the lag issue', () => {
    it('should highlight words based on line progress, not just word timings', () => {
      const testCases = [
        // Time, Expected Line Progress %, Expected Word Index, Word Name
        { time: 48.6, progress: 0, expectedIndex: 0, word: "Never" },
        { time: 49.16, progress: 20, expectedIndex: 1, word: "gonna" },  // 20% through = word 1
        { time: 49.72, progress: 40, expectedIndex: 2, word: "give" },   // 40% through = word 2  
        { time: 50.28, progress: 60, expectedIndex: 3, word: "you" },    // 60% through = word 3
        { time: 50.84, progress: 80, expectedIndex: 4, word: "up" },     // 80% through = word 4
        { time: 51.4, progress: 100, expectedIndex: 4, word: "up" },     // 100% = last word
      ];

      testCases.forEach(({ time, progress, expectedIndex, word }) => {
        const actualProgress = (time - testLine.time) / testLine.duration! * 100;
        
        // Debug the calculation
        const lineDuration = testLine.duration!;
        const lineProgress = Math.min((time - testLine.time) / lineDuration, 1);
        const wordCount = testLine.words!.length;
        const currentWordFloat = lineProgress * wordCount;
        const audioBasedIndex = Math.min(Math.floor(currentWordFloat), wordCount - 1);
        
        // Check timing-based index
        let timingBasedIndex = -1;
        for (let i = 0; i < testLine.words!.length; i++) {
          const testWord = testLine.words![i];
          if (time >= testWord.start && time < testWord.end) {
            timingBasedIndex = i;
            break;
          }
        }
        
        const actualIndex = getCurrentWordIndex(testLine, time);
        
        console.log(`Time ${time}s (${actualProgress.toFixed(1)}% through): audio-based=${audioBasedIndex}, timing-based=${timingBasedIndex}, final=${actualIndex}, expected=${expectedIndex} ("${word}")`);
        
        expect(actualProgress).toBeCloseTo(progress, 0);
        expect(actualIndex).toBe(expectedIndex);
      });
    });

    it('should ensure highlighting reaches the end before line ends', () => {
      // Test the problematic scenario from before
      const timeAt78Percent = testLine.time + (testLine.duration! * 0.78); // 50.784s
      const wordIndex = getCurrentWordIndex(testLine, timeAt78Percent);
      
      // At 78% through line, should be at word 3 or 4 (near the end)
      expect(wordIndex).toBeGreaterThanOrEqual(3);
      console.log(`At 78% through line (${timeAt78Percent}s): word ${wordIndex} ("${testLine.words![wordIndex]?.word}")`);
    });

    it('should handle the commitment line properly', () => {
      const commitmentLine: LyricLine = {
        time: 31.8,
        text: "A full commitment's what I'm thinking of",
        duration: 4.1,
        words: [
          { start: 31.8, end: 32.08, word: "A" },
          { start: 32.08, end: 32.62, word: "full" },
          { start: 32.62, end: 33.71, word: "commitment's" }, // Gets 26.5% of time
          { start: 33.71, end: 34.25, word: "what" },
          { start: 34.25, end: 34.74, word: "I'm" },
          { start: 34.74, end: 35.6, word: "thinking" }, // Gets 20.9% of time
          { start: 35.6, end: 35.9, word: "of" }
        ]
      };

      // Test at 50% through the line (should be around word 3)
      const timeAt50Percent = commitmentLine.time + (commitmentLine.duration! * 0.5); // 33.85s
      const wordIndex = getCurrentWordIndex(commitmentLine, timeAt50Percent);
      
      // With audio-based highlighting, 50% should be around word 3 (middle of 7 words)
      expect(wordIndex).toBe(3); // Should be at "what" 
      console.log(`At 50% through commitment line: word ${wordIndex} ("${commitmentLine.words![wordIndex]?.word}")`);
      
      // Test at 85% through the line (should be near the end)
      const timeAt85Percent = commitmentLine.time + (commitmentLine.duration! * 0.85); // 35.285s  
      const wordIndex85 = getCurrentWordIndex(commitmentLine, timeAt85Percent);
      
      expect(wordIndex85).toBeGreaterThanOrEqual(5); // Should be at "thinking" or "of"
      console.log(`At 85% through commitment line: word ${wordIndex85} ("${commitmentLine.words![wordIndex85]?.word}")`);
    });
  });

  describe('Fallback behavior', () => {
    it('should not get too far ahead of word timing-based index', () => {
      // Create a scenario where audio-based might be way ahead
      const line: LyricLine = {
        time: 10.0,
        text: "Quick test here",
        duration: 3.0,
        words: [
          { start: 10.0, end: 10.1, word: "Quick" },    // Very short
          { start: 10.1, end: 12.8, word: "test" },     // Very long (most of line)
          { start: 12.8, end: 13.0, word: "here" }      // Short
        ]
      };

      // At 50% through line, audio-based would suggest word 1, but timing-based is still at word 1
      const timeAt50Percent = 11.5;
      const wordIndex = getCurrentWordIndex(line, timeAt50Percent);
      
      // Should be reasonable, not jumping too far ahead
      expect(wordIndex).toBeLessThanOrEqual(2);
      console.log(`At 50% through line with uneven timing: word ${wordIndex}`);
    });
  });
});