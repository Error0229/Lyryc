/** Lyric list helpers: cleaning backend input and stable line identity. */
import type { LyricLine } from "./types";

/** Drop malformed lines and sort by time. Never trust the wire. */
export function normalizeLines(raw: unknown): LyricLine[] {
  if (!Array.isArray(raw)) return [];
  return raw
    .filter((l): l is LyricLine => !!l && typeof l.time === "number" && Number.isFinite(l.time) && typeof l.text === "string")
    .map((l) => ({ ...l, text: l.text.trim() }))
    .sort((a, b) => a.time - b.time);
}

const norm = (s: string) => s.trim().toLowerCase().replace(/\s+/g, " ");

/**
 * Identity of each line = its text plus which occurrence of that text it is.
 * When the backend swaps in better timing for the same song, the active line
 * keeps its key, so React keeps its DOM and only the timing numbers change.
 */
export function lineKeys(lines: readonly LyricLine[]): string[] {
  const seen = new Map<string, number>();
  return lines.map((l) => {
    const t = norm(l.text);
    const n = seen.get(t) ?? 0;
    seen.set(t, n + 1);
    return `${t}#${n}`;
  });
}

/** Translations are keyed by original text so they survive a timing swap. */
export function translationKey(text: string): string {
  return norm(text);
}

/** The cache's name for a song (`song_key` in lyrics_cache.rs): lowercase, spaces squashed. */
export function songCacheKey(artist: string, title: string): string {
  return [artist, title].map((s) => s.split(/\s+/).filter(Boolean).join(" ").toLowerCase()).join(" - ");
}

/** Stable signature of a lyric list's text; unchanged by retiming. */
export function textSignature(lines: readonly LyricLine[]): string {
  return lines.map((l) => norm(l.text)).join("\n");
}
