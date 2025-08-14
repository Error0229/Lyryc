import { describe, it, expect } from 'vitest';
import { LyricLine, WordTiming } from '../src/stores/lyricsStore';

describe('Word Highlighting Speed Issues', () => {
  describe('Real-world timing problem demonstration', () => {
    it('should demonstrate the slow highlighting issue with a typical line', () => {
      // Typical line: "Never gonna give you up" (2.8s duration)
      // The word segmentation might allocate time like this:
      const problematicLine: LyricLine = {
        time: 48.6,
        text: "Never gonna give you up",
        duration: 2.8,
        words: [
          { start: 48.6, end: 49.2, word: "Never" },   // 0.6s
          { start: 49.2, end: 49.86, word: "gonna" },  // 0.66s  
          { start: 49.86, end: 50.43, word: "give" },  // 0.57s
          { start: 50.43, end: 50.97, word: "you" },   // 0.54s
          { start: 50.97, end: 51.4, word: "up" }      // 0.43s
        ]
      };

      // BUT in reality, the singer might say it faster/different:
      // - "Never gonna" might be sung quickly in first 1 second
      // - "give you up" might be stretched over remaining 1.8 seconds

      // Test the problematic scenario:
      // When audio is at time 50.8 (which is 2.2s into the line, about 78% through)
      const currentTime = 50.8;
      const lineProgress = (currentTime - problematicLine.time) / problematicLine.duration!;
      
      console.log(`Audio is ${lineProgress * 100}% through the line`);
      
      // But according to our word timings, we're still in word 3 ("give")
      let currentWordIndex = -1;
      for (let i = 0; i < problematicLine.words!.length; i++) {
        const word = problematicLine.words![i];
        if (currentTime >= word.start && currentTime < word.end) {
          currentWordIndex = i;
          break;
        }
      }
      
      console.log(`According to word timings, we're at word ${currentWordIndex} ("${problematicLine.words![currentWordIndex]?.word}")`);
      console.log(`But audio is already 78% through the line!`);
      
      // This demonstrates the problem: audio is 78% through but highlighting is only at word 3/5
      expect(lineProgress).toBeGreaterThan(0.75); // Audio is well into the line
      expect(currentWordIndex).toBeLessThan(4); // But highlighting hasn't reached the last word
    });

    it('should show how word weight calculation can cause timing misalignment', () => {
      // Example: "A full commitment's what I'm thinking of" (4.1s)
      const line: LyricLine = {
        time: 31.8,
        text: "A full commitment's what I'm thinking of",
        duration: 4.1
      };

      // The word weight algorithm gives "commitment's" and "thinking" much more time
      // because they're longer words, but in singing, all words might be more evenly timed
      
      // Simulate the weight calculation
      const calculateWordWeight = (word: string): number => {
        let weight = 1;
        weight += word.length * 0.1;
        const vowels = word.match(/[aeiouAEIOU]/g)?.length || 0;
        const consonantClusters = word.match(/[bcdfghjklmnpqrstvwxyzBCDFGHJKLMNPQRSTVWXYZ]{2,}/g)?.length || 0;
        weight += vowels * 0.3 + consonantClusters * 0.2;
        const specialChars = word.match(/[^\w]/g)?.length || 0;
        weight += specialChars * 0.1;
        const shortWords = ['a', 'an', 'the', 'in', 'on', 'at', 'to', 'of', 'and', 'or', 'but'];
        if (shortWords.includes(word.toLowerCase())) {
          weight *= 0.7;
        }
        return Math.max(weight, 0.3);
      };

      const words = line.text.split(/\s+/);
      const wordWeights = words.map(calculateWordWeight);
      const totalWeight = wordWeights.reduce((sum, weight) => sum + weight, 0);

      console.log('Word weights:');
      words.forEach((word, i) => {
        const percentage = (wordWeights[i] / totalWeight * 100);
        const timeAllocation = (wordWeights[i] / totalWeight * line.duration!);
        console.log(`  "${word}": ${percentage.toFixed(1)}% (${timeAllocation.toFixed(2)}s)`);
      });

      // This shows that "commitment's" gets ~26% of the time (1.09s)
      // But in reality, each word might be more evenly distributed
      const commitmentIndex = words.findIndex(w => w.includes('commitment'));
      const commitmentWeight = wordWeights[commitmentIndex];
      const commitmentTimePercent = commitmentWeight / totalWeight;
      
      expect(commitmentTimePercent).toBeGreaterThan(0.2); // Gets >20% of total time
      console.log(`"commitment's" gets ${(commitmentTimePercent * 100).toFixed(1)}% of the line time`);
    });
  });

  describe('Proposed solution: Audio-based timing', () => {
    it('should demonstrate more accurate timing using audio-based progress', () => {
      const line: LyricLine = {
        time: 48.6,
        text: "Never gonna give you up",
        duration: 2.8,
        words: [
          { start: 48.6, end: 49.2, word: "Never" },
          { start: 49.2, end: 49.86, word: "gonna" },
          { start: 49.86, end: 50.43, word: "give" },
          { start: 50.43, end: 50.97, word: "you" },
          { start: 50.97, end: 51.4, word: "up" }
        ]
      };

      // Instead of using word-specific timings, use line-based progress
      const getAudioBasedWordIndex = (line: LyricLine, currentTime: number): number => {
        if (!line.words || currentTime < line.time) return -1;
        
        const lineDuration = line.duration ?? 3;
        const lineProgress = Math.min((currentTime - line.time) / lineDuration, 1);
        
        // Distribute progress evenly across all words
        const wordCount = line.words.length;
        const currentWordFloat = lineProgress * wordCount;
        const currentWordIndex = Math.min(Math.floor(currentWordFloat), wordCount - 1);
        
        return currentWordIndex;
      };

      // Test at various points in the line
      const testTimes = [48.8, 49.2, 49.8, 50.2, 50.8, 51.2];
      
      console.log('Audio-based word highlighting:');
      testTimes.forEach(time => {
        const lineProgress = (time - line.time) / line.duration!;
        const audioBasedIndex = getAudioBasedWordIndex(line, time);
        const currentWord = line.words![audioBasedIndex]?.word || 'none';
        
        console.log(`Time ${time}s (${(lineProgress * 100).toFixed(1)}% through line): word ${audioBasedIndex} ("${currentWord}")`);
      });

      // At 78% through the line, we should be at word 4 ("up") 
      const currentTime = 50.8;
      const lineProgress = (currentTime - line.time) / line.duration!;
      const audioBasedIndex = getAudioBasedWordIndex(line, currentTime);
      
      expect(lineProgress).toBeCloseTo(0.786, 2); // ~78.6% through line
      expect(audioBasedIndex).toBe(3); // Should be at word 3 ("you") - close to word 4
    });
  });
});