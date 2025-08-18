import { describe, it, expect } from 'vitest';
import { getLineDuration } from '../src/services/lyricsTiming';
import { LyricLine } from '../src/stores/lyricsStore';

describe('getLineDuration', () => {
  it('uses last word end when it exceeds line.duration', () => {
    const line: LyricLine = {
      time: 10,
      text: 'hello world',
      duration: 2,
      words: [
        { start: 10, end: 11, word: 'hello' },
        { start: 11, end: 13, word: 'world' }
      ]
    };
    expect(getLineDuration(line)).toBeCloseTo(3);
  });

  it('falls back to line.duration when no words present', () => {
    const line: LyricLine = { time: 0, text: 'test', duration: 4 };
    expect(getLineDuration(line)).toBe(4);
  });
});
