/**
 * The rows a song's strip reserves, and the window height they make. Decided
 * once per song (and on a settings change), never per line or per async
 * arrival: furigana that lands with the lyrics, or a translation that comes
 * seconds later, fills a row that is already there. Pure.
 */
import { isJapaneseSong } from "../lib/japanese";
import { detectLanguage, shouldTranslate } from "../lib/language";
import type { LyricLine } from "../lib/types";
import type { FuriganaPrefs, LyricStyle, TranslationPrefs } from "../state/schema";

export interface Rows {
  /** Readings over the kanji (not "replace", which swaps the text instead). */
  furigana: boolean;
  /** The translation under the line ("bilingual" only; the others don't add a row). */
  translation: boolean;
  /** A dim preview of the next line, under everything else. */
  next: boolean;
}

export const NO_ROWS: Rows = { furigana: false, translation: false, next: false };

/** Whether these lyrics get translated with these prefs: the same test that decides whether the LLM is asked. */
export function wantsTranslation(lines: readonly LyricLine[], p: TranslationPrefs): boolean {
  if (!p.enabled || lines.length === 0) return false;
  const lang = p.sourceLanguage === "auto" ? detectLanguage(lines.map((l) => l.text)) : p.sourceLanguage;
  return shouldTranslate(lang, p.targetLanguage, p.filter);
}

export function songRows(lines: readonly LyricLine[], furigana: FuriganaPrefs, translation: TranslationPrefs, nextLine = false): Rows {
  if (lines.length === 0) return NO_ROWS;
  return {
    furigana: furigana.enabled && furigana.mode !== "replace" && isJapaneseSong(lines),
    translation: translation.display === "bilingual" && wantsTranslation(lines, translation),
    next: nextLine,
  };
}

/** The next-line preview's size, relative to the lyric (lyric.css `.lyric-next`). */
export const NEXT_SCALE = 0.62;

/** The stage's height in em of the lyric size: the line, plus each reserved row. */
export function rowsEm(rows: Rows, lineHeight: number, furiganaScale: number, translationScale: number): number {
  return lineHeight + (rows.furigana ? furiganaScale * 1.1 : 0) + (rows.translation ? translationScale * 1.3 + 0.15 : 0) + (rows.next ? NEXT_SCALE * 1.3 : 0);
}

/** The strip's padding above and below the stage (py-2). */
export const STRIP_PAD_PX = 16;

/**
 * The window is taller than the strip it shows, by ROOM_PX of transparent
 * room above and below (click-through while locked). The Unlock pill / Move
 * toolbar (PILL_PX tall) sits in the room away from the anchored edge,
 * reaching PILL_OVERLAP_PX into the strip, with its name tip beyond it. The
 * same room on both sides: locking, unlocking or the anchor flipping mid-drag
 * never moves the strip in its window.
 */
export const ROOM_PX = 60;
/** Room at each end of the strip: Move mode's width grips straddle its edge there. */
export const ROOM_X_PX = 4;
export const PILL_PX = 36;
export const PILL_OVERLAP_PX = 6;

/**
 * The strip's height for these rows, in CSS px (the window adds 2 × ROOM_PX).
 * Even, so whatever is centred in it (the grips, the lyric) sits on whole pixels.
 */
export function stripHeight(rows: Rows, lyric: LyricStyle, furigana: FuriganaPrefs, translation: TranslationPrefs, remPx: number): number {
  const em = rowsEm(rows, lyric.lineHeight, furigana.scale, translation.scale);
  return Math.ceil((em * lyric.fontSize * remPx + STRIP_PAD_PX) / 2) * 2;
}
