/**
 * Pure time math: which line is active, and how a line splits into timed
 * tokens. No DOM, no React. The renderer only ever reads Token numbers.
 */
import type { LyricLine, LyricWord, Span } from "./types";

/** A rendered piece of a line. Sweep runs start→end; colour settles at done. */
export interface Token {
  text: string;
  reading?: string;
  start: number;
  end: number;
  done: number;
}

const FALLBACK_LINE_SECONDS = 5;

/** Index of the last line whose time is <= t, or -1 before the first line. */
export function activeLineIndex(lines: readonly LyricLine[], t: number): number {
  let lo = 0;
  let hi = lines.length - 1;
  let found = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (lines[mid].time <= t) {
      found = mid;
      lo = mid + 1;
    } else {
      hi = mid - 1;
    }
  }
  return found;
}

export function lineEnd(lines: readonly LyricLine[], i: number): number {
  const line = lines[i];
  if (line.duration && line.duration > 0) return line.time + line.duration;
  const next = lines[i + 1];
  return next ? next.time : line.time + FALLBACK_LINE_SECONDS;
}

interface Knot {
  pos: number;
  time: number;
}

/** Piecewise-linear map from character position to time. */
export function timeAt(knots: readonly Knot[], pos: number): number {
  if (pos <= knots[0].pos) return knots[0].time;
  for (let i = 1; i < knots.length; i++) {
    const b = knots[i];
    if (pos > b.pos) continue;
    const a = knots[i - 1];
    if (b.pos === a.pos) return b.time;
    return a.time + ((pos - a.pos) / (b.pos - a.pos)) * (b.time - a.time);
  }
  return knots[knots.length - 1].time;
}

/** Locate each word in the line text and turn it into time knots. */
export function wordKnots(text: string, words: readonly LyricWord[], end: number): Knot[] {
  const knots: Knot[] = [];
  let cursor = 0;
  words.forEach((w, i) => {
    const needle = w.text.trim();
    if (!needle) return;
    const at = text.indexOf(needle, cursor);
    if (at < 0) return;
    const next = words[i + 1];
    const dur = w.duration ?? (next ? next.time - w.time : end - w.time);
    knots.push({ pos: at, time: w.time }, { pos: at + needle.length, time: w.time + Math.max(dur, 0) });
    cursor = at + needle.length;
  });
  return knots;
}

function spansFor(text: string, spans?: readonly Span[]): Span[] {
  if (spans && spans.map((s) => s.text).join("") === text) return [...spans];
  return [{ text }];
}

/** Cut points: span edges plus word edges, but never inside a ruby span. */
function cuts(text: string, spans: readonly Span[], knots: readonly Knot[]): number[] {
  const set = new Set<number>([0, text.length]);
  const rubies: Array<[number, number]> = [];
  let pos = 0;
  for (const s of spans) {
    set.add(pos);
    if (s.reading) rubies.push([pos, pos + s.text.length]);
    pos += s.text.length;
  }
  for (const k of knots) {
    if (!rubies.some(([a, b]) => k.pos > a && k.pos < b)) set.add(k.pos);
  }
  return [...set].sort((a, b) => a - b);
}

/**
 * Split a line into tokens. Without word timings every token fills at the line
 * start and settles at the line end, so the renderer has no special case.
 */
export function tokenize(
  text: string,
  start: number,
  end: number,
  words?: readonly LyricWord[],
  spans?: readonly Span[],
): Token[] {
  const parts = spansFor(text, spans);
  const knots = words?.length ? wordKnots(text, words, end) : [];
  if (knots.length === 0) {
    return parts.map((s) => ({ text: s.text, reading: s.reading ?? undefined, start, end: start, done: end }));
  }

  const readings = new Map<number, Span>();
  let pos = 0;
  for (const s of parts) {
    if (s.reading) readings.set(pos, s);
    pos += s.text.length;
  }

  const points = cuts(text, parts, knots);
  const tokens: Token[] = [];
  for (let i = 0; i < points.length - 1; i++) {
    const a = points[i];
    const b = points[i + 1];
    const ruby = readings.get(a);
    const t0 = timeAt(knots, a);
    const t1 = timeAt(knots, b);
    tokens.push({
      text: text.slice(a, b),
      reading: ruby && ruby.text.length === b - a ? ruby.reading ?? undefined : undefined,
      start: t0,
      end: t1,
      done: t1,
    });
  }
  return tokens;
}
