import { describe, it, expect } from 'vitest';
import { LyricLine } from '../src/stores/lyricsStore';

function getEffectiveLineDuration(line: LyricLine, nextLine?: LyricLine): number {
  const lastWordEnd = line.words?.[line.words.length - 1]?.end;
  let duration = line.duration;
  if (lastWordEnd && (!duration || lastWordEnd - line.time > duration)) {
    duration = lastWordEnd - line.time;
  }
  if (duration === undefined && nextLine) {
    duration = nextLine.time - line.time;
  }
  return duration ?? 3;
}

function calculateWordBasedLineProgress(line: LyricLine, currentTime: number, nextLine?: LyricLine): number {
  if (!line.words || line.words.length === 0) {
    if (currentTime < line.time) return 0;
    const lineDuration = getEffectiveLineDuration(line, nextLine);
    return Math.min((currentTime - line.time) / lineDuration, 1);
  }

  const lineDuration = getEffectiveLineDuration(line, nextLine);
  const lineStart = line.time;
  const normalizedWords = line.words.map(word => {
    const originalStart = word.start - lineStart;
    const originalEnd = word.end - lineStart;
    return {
      normalizedStart: originalStart / lineDuration,
      normalizedEnd: originalEnd / lineDuration,
      actualStart: word.start,
      actualEnd: word.end
    };
  });

  let completedProgress = 0;
  for (const word of normalizedWords) {
    if (currentTime < word.actualStart) {
      break;
    } else if (currentTime >= word.actualEnd) {
      completedProgress = word.normalizedEnd;
    } else {
      const wordDuration = word.actualEnd - word.actualStart;
      const wordProgress = (currentTime - word.actualStart) / wordDuration;
      const contribution = word.normalizedStart + (word.normalizedEnd - word.normalizedStart) * wordProgress;
      completedProgress = contribution;
      break;
    }
  }

  return Math.min(completedProgress, 1);
}

describe('line progress duration', () => {
  it('uses last word end when it exceeds provided duration', () => {
    const line: LyricLine = {
      time: 10,
      text: 'hello world',
      duration: 2,
      words: [
        { start: 10, end: 11, word: 'hello' },
        { start: 11, end: 13, word: 'world' }
      ]
    };
    const progress = calculateWordBasedLineProgress(line, 11.5);
    expect(progress).toBeCloseTo(0.5, 2);
  });
});
