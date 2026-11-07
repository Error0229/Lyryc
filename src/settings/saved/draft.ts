/**
 * The saved-lyrics editor's model, kept pure: the rows being edited, and what
 * saving them would change.
 */
import { formatTime, parseTime } from "../../lib/lrc";
import type { LyricLine } from "../../lib/types";
import type { SavedChange, SavedSongDetail } from "../../services/ipc/backend";

/** A line being edited. `tr`: its line in each saved translation. */
export interface EditRow {
  id: number;
  time: string;
  /** The time it was loaded with, kept exact while `time` still shows it. */
  at: number | null;
  text: string;
  duration: number | null;
  tr: string[];
}

let next = 0;

/** Rows for `lines`; `translations[k][i]` is line i's text in translation k. */
export function rowsOf(lines: readonly LyricLine[], translations: readonly (readonly string[])[]): EditRow[] {
  return lines.map((l, i) => ({
    id: ++next,
    time: formatTime(l.time),
    at: l.time,
    text: l.text,
    duration: l.duration ?? null,
    tr: translations.map((t) => t[i] ?? ""),
  }));
}

export function blankRow(time: number, translations: number): EditRow {
  return { id: ++next, time: formatTime(time), at: null, text: "", duration: null, tr: Array(translations).fill("") };
}

/** A row with a new time: its old length no longer fits it. */
export function retimed(row: EditRow, time: string): EditRow {
  return { ...row, time, duration: null };
}

export function timeOf(row: EditRow): number | null {
  return row.at !== null && row.time === formatTime(row.at) ? row.at : parseTime(row.time);
}

export interface Draft {
  /** Rows whose time doesn't read. */
  bad: Set<number>;
  /** The words differ: the translations and the word timing go on save. */
  wordsChanged: boolean;
  /** Words or times differ: the word timing goes on save. */
  linesChanged: boolean;
  /** What Save sends; null when there's nothing (valid) to save. */
  change: SavedChange | null;
}

const words = (lines: readonly LyricLine[]) => lines.map((l) => l.text).join("\n");
const same = (a: LyricLine, b: LyricLine | undefined) => !!b && a.time === b.time && a.text === b.text && a.duration === (b.duration ?? null);

export function draftOf(saved: SavedSongDetail, rows: readonly EditRow[]): Draft {
  const timed = rows.map((r) => ({ r, time: timeOf(r) }));
  const bad = new Set(timed.filter((t) => t.time === null).map((t) => t.r.id));
  // Sorted as the backend stores them (a stable sort, like Rust's).
  const sorted = timed.filter((t) => t.time !== null).sort((a, b) => a.time! - b.time!);
  const lines = sorted.map(({ r, time }) => ({ time: time!, text: r.text, duration: r.duration }));
  const wordsChanged = words(lines) !== words(saved.lines);
  const linesChanged = wordsChanged || lines.some((l, i) => !same(l, saved.lines[i]));
  const translations = wordsChanged ? [] : saved.translations.map((t, k) => ({ key: t.key, lines: sorted.map(({ r }) => r.tr[k]) }));
  const edited = translations.some((t, k) => t.lines.some((l, i) => l !== saved.translations[k].lines[i]));
  const ok = bad.size === 0 && lines.length > 0 && (linesChanged || edited);
  return { bad, wordsChanged, linesChanged, change: ok ? { op: "save", lines, translations } : null };
}
