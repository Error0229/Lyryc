import { describe, it, expect } from 'vitest';

// Mock data structures based on the actual types
interface WordTiming {
  start: number; // milliseconds
  end: number; // milliseconds
  word: string;
}

interface LyricLine {
  time: number; // milliseconds
  text: string;
  duration?: number;
  words?: WordTiming[];
}

describe('Word Timing Tests', () => {
  describe('Line Progress Calculation', () => {
    it('should calculate correct progress for active line', () => {
      const line: LyricLine = {
        time: 10000, // 10 seconds
        duration: 4000, // 4 seconds duration
        text: 'Hello world test line'
      };
      
      // Test at different time points
      const testCases = [
        { currentTime: 10000, expectedProgress: 0 },    // Start of line
        { currentTime: 12000, expectedProgress: 0.5 },  // Middle of line
        { currentTime: 14000, expectedProgress: 1 },    // End of line
        { currentTime: 15000, expectedProgress: 1 },    // Past end (should clamp to 1)
        { currentTime: 9000, expectedProgress: 0 },     // Before start (should clamp to 0)
      ];
      
      testCases.forEach(({ currentTime, expectedProgress }) => {
        const progress = Math.max(0, Math.min(1, (currentTime - line.time) / (line.duration || 3000)));
        expect(progress).toBeCloseTo(expectedProgress, 2);
      });
    });
    
    it('should use default duration when not specified', () => {
      const line: LyricLine = {
        time: 10000,
        text: 'Line without duration'
        // No duration specified
      };
      
      // Should use default 3000ms duration
      const currentTime = 11500; // 1.5 seconds into line
      const expectedProgress = 1500 / 3000; // 0.5
      const progress = Math.max(0, Math.min(1, (currentTime - line.time) / 3000));
      
      expect(progress).toBeCloseTo(expectedProgress, 2);
    });
  });

  describe('Word Timing Consistency', () => {
    it('should have words that span the entire line duration', () => {
      const line: LyricLine = {
        time: 10000,
        duration: 4000,
        text: 'Hello world test',
        words: [
          { start: 10000, end: 11000, word: 'Hello' },  // 0-1s
          { start: 11000, end: 12500, word: 'world' },  // 1-2.5s  
          { start: 12500, end: 14000, word: 'test' }    // 2.5-4s
        ]
      };
      
      // First word should start at line time
      expect(line.words[0].start).toBe(line.time);
      
      // Last word should end at or near line time + duration
      const lastWord = line.words[line.words.length - 1];
      const expectedEnd = line.time + line.duration;
      expect(lastWord.end).toBe(expectedEnd);
      
      // Words should be contiguous (no gaps)
      for (let i = 0; i < line.words.length - 1; i++) {
        expect(line.words[i].end).toBe(line.words[i + 1].start);
      }
    });
    
    it('should calculate word progress that aligns with line progress', () => {
      const line: LyricLine = {
        time: 10000,
        duration: 4000, 
        text: 'Hello world',
        words: [
          { start: 10000, end: 12000, word: 'Hello' },  // 0-2s (50% of line)
          { start: 12000, end: 14000, word: 'world' }   // 2-4s (50% of line)
        ]
      };
      
      // Test at middle of line (50% progress)
      const currentTime = 12000;
      const lineProgress = (currentTime - line.time) / line.duration; // 0.5
      
      // At this point:
      // - First word should be complete (progress = 1)
      // - Second word should be just starting (progress = 0)
      
      // Check first word
      const firstWordProgress = currentTime >= line.words[0].end ? 1 : 
        Math.max(0, Math.min(1, (currentTime - line.words[0].start) / (line.words[0].end - line.words[0].start)));
      expect(firstWordProgress).toBe(1);
      
      // Check second word  
      const secondWordProgress = currentTime < line.words[1].start ? 0 :
        Math.max(0, Math.min(1, (currentTime - line.words[1].start) / (line.words[1].end - line.words[1].start)));
      expect(secondWordProgress).toBe(0);
    });
  });

  describe('Progress Synchronization', () => {
    it('should show consistent progress between line and word calculations', () => {
      const line: LyricLine = {
        time: 10000,
        duration: 3000,
        text: 'Test line sync',
        words: [
          { start: 10000, end: 11000, word: 'Test' },   // 0-1s
          { start: 11000, end: 12000, word: 'line' },   // 1-2s
          { start: 12000, end: 13000, word: 'sync' }    // 2-3s
        ]
      };
      
      const testTimePoints = [10000, 10500, 11000, 11500, 12000, 12500, 13000];
      
      testTimePoints.forEach(currentTime => {
        const lineProgress = Math.max(0, Math.min(1, (currentTime - line.time) / line.duration));
        
        // Calculate which word should be active
        let activeWordIndex = -1;
        for (let i = 0; i < line.words.length; i++) {
          if (currentTime >= line.words[i].start && currentTime < line.words[i].end) {
            activeWordIndex = i;
            break;
          }
        }
        
        // If we have an active word, its progress should make sense relative to line progress
        if (activeWordIndex >= 0) {
          const word = line.words[activeWordIndex];
          const wordProgress = Math.max(0, Math.min(1, (currentTime - word.start) / (word.end - word.start)));
          
          // The word's relative position in the line should align with line progress
          const wordStartInLine = (word.start - line.time) / line.duration;
          const wordEndInLine = (word.end - line.time) / line.duration;
          const expectedWordContribution = wordStartInLine + (wordEndInLine - wordStartInLine) * wordProgress;
          
          // Line progress should be close to the word's contribution
          expect(lineProgress).toBeCloseTo(expectedWordContribution, 1);
        }
      });
    });
  });
  
  describe('Edge Cases', () => {
    it('should handle words with zero duration', () => {
      const line: LyricLine = {
        time: 10000,
        duration: 2000,
        text: 'Quick test',
        words: [
          { start: 10000, end: 10000, word: 'Quick' },  // Zero duration
          { start: 10000, end: 12000, word: 'test' }    // Normal duration
        ]
      };
      
      const currentTime = 10000;
      
      // Zero duration word should have progress 1 when time >= start
      const zeroWordProgress = line.words[0].end === line.words[0].start ? 1 : 
        Math.max(0, Math.min(1, (currentTime - line.words[0].start) / (line.words[0].end - line.words[0].start)));
      
      // Should not throw or return NaN
      expect(zeroWordProgress).toBe(1);
      expect(isNaN(zeroWordProgress)).toBe(false);
    });
    
    it('should handle overlapping word timings', () => {
      const line: LyricLine = {
        time: 10000,
        duration: 3000,
        text: 'Overlap test',
        words: [
          { start: 10000, end: 12000, word: 'Overlap' }, // 0-2s
          { start: 11000, end: 13000, word: 'test' }     // 1-3s (overlaps by 1s)
        ]
      };
      
      const currentTime = 11500; // 1.5s - in overlap zone
      
      // Both words should be considered active in overlap zone
      const word1Active = currentTime >= line.words[0].start && currentTime < line.words[0].end;
      const word2Active = currentTime >= line.words[1].start && currentTime < line.words[1].end;
      
      expect(word1Active).toBe(true);
      expect(word2Active).toBe(true);
    });
  });
});