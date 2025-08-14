import { describe, it, expect } from 'vitest';
import { LyricLine, WordTiming } from '../src/stores/lyricsStore';

// Simulate the improved getCurrentWordIndex logic from LyricsViewer.tsx
function getCurrentWordIndex(line: LyricLine, currentTime: number): number {
  if (!line.words || !line.words.length) return -1;

  // If before the first word, return -1
  if (currentTime < line.words[0].start) return -1;
  
  // If after the last word, show the last word as completed
  if (currentTime >= line.words[line.words.length - 1].end) return line.words.length - 1;

  // Find the appropriate word index
  for (let i = 0; i < line.words.length; i++) {
    const word = line.words[i];
    const nextWord = line.words[i + 1];
    
    if (currentTime >= word.start && currentTime < word.end) {
      // Currently in this word
      return i;
    } else if (currentTime >= word.end && (!nextWord || currentTime < nextWord.start)) {
      // Between this word and the next - show next word as starting
      return nextWord ? i + 1 : i;
    }
  }
  
  // Fallback: find the closest word based on timing
  let closestIndex = 0;
  let closestDistance = Math.abs(currentTime - line.words[0].start);
  
  for (let i = 1; i < line.words.length; i++) {
    const word = line.words[i];
    const distanceToStart = Math.abs(currentTime - word.start);
    const distanceToMid = Math.abs(currentTime - (word.start + word.end) / 2);
    const distance = Math.min(distanceToStart, distanceToMid);
    
    if (distance < closestDistance) {
      closestDistance = distance;
      closestIndex = i;
    }
  }
  
  return closestIndex;
}

describe('Word Highlighting Synchronization', () => {
  const testLine: LyricLine = {
    time: 48.6,
    text: "Never gonna give you up",
    duration: 2.8,
    words: [
      { start: 48.6, end: 49.2, word: "Never" },   // 48.6-49.2 (0.6s)
      { start: 49.2, end: 49.86, word: "gonna" },  // 49.2-49.86 (0.66s)  
      { start: 49.86, end: 50.43, word: "give" },  // 49.86-50.43 (0.57s)
      { start: 50.43, end: 50.97, word: "you" },   // 50.43-50.97 (0.54s)
      { start: 50.97, end: 51.4, word: "up" }      // 50.97-51.4 (0.43s)
    ]
  };

  describe('Word Index Detection', () => {
    it('should return -1 for times before the first word', () => {
      expect(getCurrentWordIndex(testLine, 48.0)).toBe(-1);
      expect(getCurrentWordIndex(testLine, 48.5)).toBe(-1);
    });

    it('should return correct index during each word', () => {
      // During "Never" (48.6-49.2)
      expect(getCurrentWordIndex(testLine, 48.6)).toBe(0);
      expect(getCurrentWordIndex(testLine, 48.9)).toBe(0);
      expect(getCurrentWordIndex(testLine, 49.1)).toBe(0);

      // During "gonna" (49.2-49.86)
      expect(getCurrentWordIndex(testLine, 49.2)).toBe(1);
      expect(getCurrentWordIndex(testLine, 49.5)).toBe(1);
      expect(getCurrentWordIndex(testLine, 49.8)).toBe(1);

      // During "give" (49.86-50.43)
      expect(getCurrentWordIndex(testLine, 49.86)).toBe(2);
      expect(getCurrentWordIndex(testLine, 50.1)).toBe(2);
      expect(getCurrentWordIndex(testLine, 50.4)).toBe(2);

      // During "you" (50.43-50.97)
      expect(getCurrentWordIndex(testLine, 50.43)).toBe(3);
      expect(getCurrentWordIndex(testLine, 50.7)).toBe(3);
      expect(getCurrentWordIndex(testLine, 50.9)).toBe(3);

      // During "up" (50.97-51.4)
      expect(getCurrentWordIndex(testLine, 50.97)).toBe(4);
      expect(getCurrentWordIndex(testLine, 51.1)).toBe(4);
      expect(getCurrentWordIndex(testLine, 51.3)).toBe(4);
    });

    it('should handle transitions between words without gaps', () => {
      // Exactly at word boundaries - should show next word
      expect(getCurrentWordIndex(testLine, 49.2)).toBe(1); // End of "Never", start of "gonna"
      expect(getCurrentWordIndex(testLine, 49.86)).toBe(2); // End of "gonna", start of "give"
      expect(getCurrentWordIndex(testLine, 50.43)).toBe(3); // End of "give", start of "you"
      expect(getCurrentWordIndex(testLine, 50.97)).toBe(4); // End of "you", start of "up"
    });

    it('should handle time after the last word', () => {
      expect(getCurrentWordIndex(testLine, 51.4)).toBe(4); // End of "up"
      expect(getCurrentWordIndex(testLine, 51.5)).toBe(4); // After "up"
      expect(getCurrentWordIndex(testLine, 52.0)).toBe(4); // Well after "up"
    });

    it('should handle gaps between words by showing next word', () => {
      // Create a line with gaps between words
      const gappedLine: LyricLine = {
        time: 10.0,
        text: "Hello world",
        duration: 4.0,
        words: [
          { start: 10.0, end: 11.0, word: "Hello" },
          // Gap from 11.0 to 12.0
          { start: 12.0, end: 13.0, word: "world" }
        ]
      };

      // During the gap, should show the next word
      expect(getCurrentWordIndex(gappedLine, 11.5)).toBe(1); // In the gap, should prepare for "world"
    });
  });

  describe('Timing Edge Cases', () => {
    it('should handle very short words correctly', () => {
      const shortWordLine: LyricLine = {
        time: 5.0,
        text: "A big test",
        duration: 3.0,
        words: [
          { start: 5.0, end: 5.1, word: "A" },     // Very short word
          { start: 5.1, end: 6.5, word: "big" },
          { start: 6.5, end: 8.0, word: "test" }
        ]
      };

      expect(getCurrentWordIndex(shortWordLine, 5.05)).toBe(0); // During "A"
      expect(getCurrentWordIndex(shortWordLine, 5.1)).toBe(1);  // Start of "big"
      expect(getCurrentWordIndex(shortWordLine, 6.5)).toBe(2);  // Start of "test"
    });

    it('should handle overlapping word timings', () => {
      const overlappingLine: LyricLine = {
        time: 20.0,
        text: "Over lap",
        duration: 3.0,
        words: [
          { start: 20.0, end: 21.5, word: "Over" },
          { start: 21.0, end: 23.0, word: "lap" }  // Overlaps with "Over"
        ]
      };

      // During overlap period (21.0-21.5), should show first word since it started first
      expect(getCurrentWordIndex(overlappingLine, 21.2)).toBe(0); // Still in "Over"
      expect(getCurrentWordIndex(overlappingLine, 21.6)).toBe(1); // Now in "lap"
    });

    it('should handle zero-duration words', () => {
      const zeroDurationLine: LyricLine = {
        time: 30.0,
        text: "Quick pause",
        duration: 2.0,
        words: [
          { start: 30.0, end: 30.0, word: "Quick" }, // Zero duration
          { start: 30.0, end: 32.0, word: "pause" }
        ]
      };

      expect(getCurrentWordIndex(zeroDurationLine, 30.0)).toBe(1); // Should go to next word immediately
      expect(getCurrentWordIndex(zeroDurationLine, 31.0)).toBe(1); // In "pause"
    });
  });

  describe('Real-world Scenarios', () => {
    it('should handle the common lagging issue described by user', () => {
      // Simulate the scenario where highlighting lags ~3 words behind
      const testTimes = [
        48.5,  // Just before "Never" 
        49.1,  // Early in "Never"
        49.19, // Very end of "Never" 
        49.2,  // Start of "gonna"
        49.3,  // In "gonna"
        49.85, // End of "gonna"
        49.86, // Start of "give"
        50.42, // End of "give"
        50.43, // Start of "you"
        50.96, // End of "you"
        50.97, // Start of "up"
        51.39, // End of "up"
        51.4   // After "up"
      ];

      const expectedIndices = [
        -1, // Before start (48.5 < 48.6)
        0,  // "Never"
        0,  // Still "Never" 
        1,  // "gonna"
        1,  // "gonna"
        1,  // "gonna"
        2,  // "give"
        2,  // "give"
        3,  // "you"
        3,  // "you"
        4,  // "up"
        4,  // "up"
        4   // Still "up"
      ];

      testTimes.forEach((time, i) => {
        const actualIndex = getCurrentWordIndex(testLine, time);
        expect(actualIndex).toBe(expectedIndices[i], 
          `At time ${time}s, expected word index ${expectedIndices[i]} but got ${actualIndex}`);
      });
    });

    it('should be responsive enough to avoid the 3-word lag issue', () => {
      // Test that when we're 90% through a word, we're still highlighting it correctly
      testLine.words!.forEach((word, index) => {
        const wordProgress90 = word.start + (word.end - word.start) * 0.9;
        const wordProgress99 = word.start + (word.end - word.start) * 0.99;
        
        expect(getCurrentWordIndex(testLine, wordProgress90)).toBe(index, 
          `At 90% through word "${word.word}", should still highlight word ${index}`);
        expect(getCurrentWordIndex(testLine, wordProgress99)).toBe(index, 
          `At 99% through word "${word.word}", should still highlight word ${index}`);
      });
    });
  });
});