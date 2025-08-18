import { LyricLine } from "../stores/lyricsStore";

export const getLineDuration = (line: LyricLine): number => {
  if (line.words && line.words.length > 0) {
    const lastWordEnd = line.words[line.words.length - 1].end;
    if (lastWordEnd > line.time) {
      return lastWordEnd - line.time;
    }
  }
  return line.duration ?? 3;
};
