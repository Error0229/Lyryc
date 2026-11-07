/** LRC text ("[01:02.50]words") to lyric lines and back, for editing saved lyrics. */
import type { LyricLine } from "./types";

const TAG = /\[(\d+):(\d{1,2}(?:[.:]\d+)?)\]/g;

/** "1:02.50" / "62.5" → seconds; null when it isn't a time. */
export function parseTime(s: string): number | null {
  const m = /^\s*(?:(\d+):)?(\d+(?:\.\d+)?)\s*$/.exec(s);
  if (!m) return null;
  return Number(m[1] ?? 0) * 60 + Number(m[2]);
}

/** Seconds → "1:02.50". */
export function formatTime(t: number): string {
  const cs = Math.round(Math.max(0, t) * 100);
  const m = Math.floor(cs / 6000);
  const s = ((cs % 6000) / 100).toFixed(2).padStart(5, "0");
  return `${m}:${s}`;
}

/**
 * Every timed line, sorted. A line with several tags ("[00:10][01:20]chorus")
 * is sung at each; metadata tags ("[ar:...]") and untimed lines are skipped.
 */
export function parseLrc(text: string): LyricLine[] {
  const lines: LyricLine[] = [];
  for (const raw of text.split(/\r?\n/)) {
    const times = [...raw.matchAll(TAG)].map((m) => Number(m[1]) * 60 + Number(m[2].replace(":", ".")));
    const words = raw.replace(TAG, "").trim();
    for (const time of times) lines.push({ time, text: words, duration: null });
  }
  return lines.sort((a, b) => a.time - b.time);
}

/** Lines → LRC text, one "[mm:ss.cc]words" per line. */
export function formatLrc(lines: readonly LyricLine[]): string {
  return lines.map((l) => `[${formatTime(l.time).padStart(8, "0")}]${l.text}`).join("\n");
}
