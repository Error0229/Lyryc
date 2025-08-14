import { describe, it, expect } from 'vitest';
import { LyricLine, WordTiming } from '../src/stores/lyricsStore';

// Test data based on "Never Gonna Give You Up" by Rick Astley
const neverGonnaGiveYouUpLyrics: LyricLine[] = [
  {
    time: 24.0,
    text: "We're no strangers to love",
    duration: 3.2,
  },
  {
    time: 27.7,
    text: "You know the rules and so do I",
    duration: 3.6,
  },
  {
    time: 31.8,
    text: "A full commitment's what I'm thinking of",
    duration: 4.1,
  },
  {
    time: 36.4,
    text: "You wouldn't get this from any other guy",
    duration: 4.0,
  },
  {
    time: 41.1,
    text: "I just wanna tell you how I'm feeling",
    duration: 3.8,
  },
  {
    time: 45.4,
    text: "Gotta make you understand",
    duration: 2.7,
  },
  {
    time: 48.6,
    text: "Never gonna give you up",
    duration: 2.8,
  },
  {
    time: 51.7,
    text: "Never gonna let you down",
    duration: 2.9,
  },
  {
    time: 54.9,
    text: "Never gonna run around and desert you",
    duration: 3.4,
  },
  {
    time: 58.7,
    text: "Never gonna make you cry",
    duration: 2.8,
  },
  {
    time: 61.8,
    text: "Never gonna say goodbye",
    duration: 2.9,
  },
  {
    time: 65.0,
    text: "Never gonna tell a lie and hurt you",
    duration: 3.2,
  }
];

// Function to generate word timings (replicating the logic from lyricsProcessor.ts)
function generateWordTimings(line: LyricLine): WordTiming[] {
  const words = line.text.split(/\s+/).filter(w => w.length > 0);
  const wordTimings: WordTiming[] = [];

  if (words.length === 0) return wordTimings;

  const lineDuration = line.duration ?? 3;

  // Calculate relative durations based on word characteristics
  const wordWeights = words.map(word => calculateWordWeight(word));
  const totalWeight = wordWeights.reduce((sum, weight) => sum + weight, 0);

  let currentTime = line.time;

  // First pass: compute base segments
  const baseSegments: Array<{ start: number; end: number; word: string }> = [];
  for (let i = 0; i < words.length; i++) {
    const wordDuration = (wordWeights[i] / totalWeight) * lineDuration;
    const endTime = currentTime + wordDuration;
    baseSegments.push({ start: currentTime, end: endTime, word: words[i] });
    currentTime = endTime;
  }

  // Normalize to exactly fit line duration
  if (baseSegments.length > 0) {
    const total = baseSegments[baseSegments.length - 1].end - baseSegments[0].start;
    const scale = total > 0 ? lineDuration / total : 1;
    const start0 = baseSegments[0].start;
    let prevEnd = line.time;
    
    for (let i = 0; i < baseSegments.length; i++) {
      const relStart = baseSegments[i].start - start0;
      const relEnd = baseSegments[i].end - start0;
      let s = line.time + relStart * scale;
      let e = line.time + relEnd * scale;
      s = Math.max(prevEnd, s);
      e = Math.max(s, Math.min(e, line.time + lineDuration));
      wordTimings.push({ start: s, end: e, word: words[i] });
      prevEnd = e;
    }
    // Ensure last word ends exactly at line end
    wordTimings[wordTimings.length - 1].end = line.time + lineDuration;
  }

  return wordTimings;
}

function calculateWordWeight(word: string): number {
  let weight = 1; // Base weight

  // Length factor
  weight += word.length * 0.1;

  // Syllable estimation (rough)
  const vowels = word.match(/[aeiouAEIOU]/g)?.length || 0;
  const consonantClusters = word.match(/[bcdfghjklmnpqrstvwxyzBCDFGHJKLMNPQRSTVWXYZ]{2,}/g)?.length || 0;
  weight += vowels * 0.3 + consonantClusters * 0.2;

  // Special characters (indicate complex words)
  const specialChars = word.match(/[^\w]/g)?.length || 0;
  weight += specialChars * 0.1;

  // Common short words get less time
  const shortWords = ['a', 'an', 'the', 'in', 'on', 'at', 'to', 'of', 'and', 'or', 'but'];
  if (shortWords.includes(word.toLowerCase())) {
    weight *= 0.7;
  }

  return Math.max(weight, 0.3); // Minimum weight
}

describe('Word Segmentation Tests - Never Gonna Give You Up', () => {
  describe('Duration Sum Validation', () => {
    it('should ensure word durations sum exactly to line duration for each lyric line', () => {
      neverGonnaGiveYouUpLyrics.forEach((line, index) => {
        const words = generateWordTimings(line);
        
        // Calculate total word duration by summing individual word durations
        const totalWordDuration = words.reduce((sum, word) => sum + (word.end - word.start), 0);
        
        // Should match line duration exactly (within floating point precision)
        expect(totalWordDuration).toBeCloseTo(line.duration!, 3);
        
        console.log(`Line ${index + 1}: "${line.text}"`);
        console.log(`  Line duration: ${line.duration}s`);
        console.log(`  Word durations sum: ${totalWordDuration.toFixed(3)}s`);
        console.log(`  Words: ${words.map(w => `${w.word}(${(w.end - w.start).toFixed(2)}s)`).join(', ')}`);
      });
    });

    it('should have contiguous word timings with no gaps', () => {
      neverGonnaGiveYouUpLyrics.forEach((line) => {
        const words = generateWordTimings(line);
        
        if (words.length > 1) {
          for (let i = 0; i < words.length - 1; i++) {
            // Next word should start exactly when current word ends
            expect(words[i].end).toBeCloseTo(words[i + 1].start, 6);
          }
        }
      });
    });

    it('should start first word at line start time and end last word at line end time', () => {
      neverGonnaGiveYouUpLyrics.forEach((line) => {
        const words = generateWordTimings(line);
        
        if (words.length > 0) {
          // First word should start at line time
          expect(words[0].start).toBeCloseTo(line.time, 6);
          
          // Last word should end at line time + duration
          const expectedEndTime = line.time + line.duration!;
          expect(words[words.length - 1].end).toBeCloseTo(expectedEndTime, 6);
        }
      });
    });
  });

  describe('Word Timing Logic', () => {
    it('should generate appropriate number of words for each line', () => {
      const testCases = [
        { text: "We're no strangers to love", expectedCount: 5 },
        { text: "You know the rules and so do I", expectedCount: 8 },
        { text: "A full commitment's what I'm thinking of", expectedCount: 7 },
        { text: "Never gonna give you up", expectedCount: 5 },
        { text: "Never gonna run around and desert you", expectedCount: 7 }
      ];

      testCases.forEach(({ text, expectedCount }) => {
        const mockLine: LyricLine = { time: 0, text, duration: 3 };
        const words = generateWordTimings(mockLine);
        
        expect(words).toHaveLength(expectedCount);
      });
    });

    it('should assign longer durations to longer/more complex words', () => {
      const line: LyricLine = {
        time: 31.8,
        text: "A full commitment's what I'm thinking of",
        duration: 4.1
      };
      
      const words = generateWordTimings(line);
      const commitmentWord = words.find(w => w.word.includes('commitment'));
      const aWord = words.find(w => w.word === 'A');
      
      if (commitmentWord && aWord) {
        const commitmentDuration = commitmentWord.end - commitmentWord.start;
        const aDuration = aWord.end - aWord.start;
        
        // "commitment's" should get more time than "A"
        expect(commitmentDuration).toBeGreaterThan(aDuration);
      }
    });

    it('should handle contractions and punctuation correctly', () => {
      const line: LyricLine = {
        time: 24.0,
        text: "We're no strangers to love",
        duration: 3.2
      };
      
      const words = generateWordTimings(line);
      const contractedWord = words.find(w => w.word === "We're");
      
      expect(contractedWord).toBeDefined();
      expect(contractedWord!.word).toBe("We're");
    });
  });

  describe('Edge Cases', () => {
    it('should handle single word lines correctly', () => {
      const singleWordLine: LyricLine = {
        time: 10.0,
        text: "Never",
        duration: 2.0
      };
      
      const words = generateWordTimings(singleWordLine);
      
      expect(words).toHaveLength(1);
      expect(words[0].start).toBe(10.0);
      expect(words[0].end).toBe(12.0);
      expect(words[0].word).toBe("Never");
    });

    it('should handle lines with multiple spaces correctly', () => {
      const spacedLine: LyricLine = {
        time: 20.0,
        text: "Never  gonna   give    you     up",
        duration: 3.0
      };
      
      const words = generateWordTimings(spacedLine);
      
      expect(words).toHaveLength(5);
      expect(words.map(w => w.word)).toEqual(['Never', 'gonna', 'give', 'you', 'up']);
      
      // Duration should still sum correctly
      const totalDuration = words.reduce((sum, word) => sum + (word.end - word.start), 0);
      expect(totalDuration).toBeCloseTo(3.0, 3);
    });

    it('should handle empty lines gracefully', () => {
      const emptyLine: LyricLine = {
        time: 15.0,
        text: "",
        duration: 1.0
      };
      
      const words = generateWordTimings(emptyLine);
      
      expect(words).toHaveLength(0);
    });

    it('should handle very short durations', () => {
      const shortLine: LyricLine = {
        time: 5.0,
        text: "Up",
        duration: 0.1
      };
      
      const words = generateWordTimings(shortLine);
      
      expect(words).toHaveLength(1);
      expect(words[0].end - words[0].start).toBeCloseTo(0.1, 3);
    });
  });

  describe('Consistency Across Full Song', () => {
    it('should maintain timing consistency across the entire Never Gonna Give You Up chorus', () => {
      const chorusLines = neverGonnaGiveYouUpLyrics.slice(6, 12); // The main "Never gonna" chorus
      
      chorusLines.forEach((line, index) => {
        const words = generateWordTimings(line);
        
        // Each chorus line should have reasonable word count
        expect(words.length).toBeGreaterThan(3);
        expect(words.length).toBeLessThanOrEqual(8);
        
        // All words should have positive duration
        words.forEach(word => {
          expect(word.end - word.start).toBeGreaterThan(0);
        });
        
        // Line timing should be sequential
        if (index > 0) {
          const prevLine = chorusLines[index - 1];
          const prevWords = generateWordTimings(prevLine);
          
          if (prevWords.length > 0) {
            // Current line should start after previous line
            expect(line.time).toBeGreaterThanOrEqual(
              prevLine.time + prevLine.duration! - 0.1 // Allow small overlap
            );
          }
        }
      });
    });

    it('should generate realistic word timing distributions', () => {
      neverGonnaGiveYouUpLyrics.forEach((line) => {
        const words = generateWordTimings(line);
        
        if (words.length > 0) {
          const durations = words.map(w => w.end - w.start);
          const avgDuration = durations.reduce((sum, d) => sum + d, 0) / durations.length;
          
          // Average word duration should be reasonable (between 0.1s and 1s typically)
          expect(avgDuration).toBeGreaterThan(0.05);
          expect(avgDuration).toBeLessThan(2.0);
          
          // No word should be extremely short or long
          durations.forEach(duration => {
            expect(duration).toBeGreaterThan(0.01); // At least 10ms
            expect(duration).toBeLessThan(line.duration! * 0.8); // No word takes more than 80% of line
          });
        }
      });
    });
  });
});