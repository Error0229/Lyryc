/**
 * Sync offsets, in seconds. Positive = lyrics appear earlier (the offset is
 * added to playback time). Same semantics and track key as the v2 config so
 * persisted values keep meaning the same thing.
 */

export const OFFSET_LIMIT = 10;
export const NUDGE_STEP = 0.1;

export function trackKey(artist: string, title: string): string {
  return `${artist.toLowerCase().trim()} - ${title.toLowerCase().trim()}`;
}

/** Clamp and round to 10 ms so float noise never accumulates. */
export function normalizeOffset(v: number): number {
  if (!Number.isFinite(v)) return 0;
  const clamped = Math.min(OFFSET_LIMIT, Math.max(-OFFSET_LIMIT, v));
  return Math.round(clamped * 100) / 100;
}

export function totalOffset(global: number, tracks: Record<string, number>, key: string | null): number {
  return global + (key ? tracks[key] ?? 0 : 0);
}

/** Set a track offset; zero removes the entry instead of storing noise. */
export function withTrackOffset(tracks: Record<string, number>, key: string, value: number): Record<string, number> {
  const next = { ...tracks };
  const v = normalizeOffset(value);
  if (v === 0) delete next[key];
  else next[key] = v;
  return next;
}

export function formatOffset(v: number): string {
  const n = normalizeOffset(v);
  return `${n > 0 ? "+" : n < 0 ? "−" : "±"}${Math.abs(n).toFixed(2)} s`;
}

/**
 * What someone typed into an offset field, in seconds: "0.35", "-1.2",
 * "+0.5 s", "−1,2" (a typographic minus or a decimal comma). Null when it
 * isn't a number, so the field keeps its old value.
 */
export function parseOffset(text: string): number | null {
  const t = text.trim().replace(/−/g, "-").replace(",", ".").replace(/\s*s(ec)?$/i, "").replace(/^±/, "");
  if (!/^[+-]?(\d+\.?\d*|\.\d+)$/.test(t)) return null;
  return normalizeOffset(Number(t));
}

/** Step for keys in an offset field or slider: 0.05 s, 0.5 s with Shift. */
export const OFFSET_KEY_STEP = 0.05;
