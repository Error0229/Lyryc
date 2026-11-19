/** Which text to show for the active line, already cut into timed tokens. Pure. */
import { lineEnd, tokenize, type Token } from "../lib/timeline";
import { lineLanguage, UNKNOWN } from "../lib/language";
import { translationKey } from "../lib/lyrics";
import type { LyricLine, Span } from "../lib/types";
import type { TranslationDisplay } from "../state/schema";

export interface LineModel {
  key: string;
  tokens: Token[];
  /** Secondary text (translation) shown with the line. */
  sub: string | null;
  /** The next line's text, for the preview row. */
  next: string | null;
  /** Each row's language family (UNKNOWN for Latin script), for its font and `lang`. */
  lang: string;
  subLang: string;
  nextLang: string;
}

export interface ModelInput {
  lines: readonly LyricLine[];
  keys: readonly string[];
  index: number;
  wordHighlight: boolean;
  translations: Record<string, string> | null; // null = translation off
  display: TranslationDisplay;
  furigana: Record<string, Span[]> | null; // null = furigana off
  /** The song's language (`detectLanguage`): what a line's script alone can't tell. */
  language: string;
  /** The translation's language family. */
  target: string;
}

const INSTRUMENTAL = "♪";

export function lineModel(m: ModelInput): LineModel | null {
  const line = m.lines[m.index];
  if (!line) return null;
  const start = line.time;
  const end = lineEnd(m.lines, m.index);
  const translated = m.translations?.[translationKey(line.text)] ?? null;

  const next = m.lines[m.index + 1]?.text || null;
  const langs = { nextLang: next ? lineLanguage(next, m.language) : UNKNOWN };

  if (translated && m.display === "translated") {
    return { key: m.keys[m.index], tokens: tokenize(translated, start, end), sub: null, next, lang: m.target, subLang: UNKNOWN, ...langs };
  }

  const text = line.text || INSTRUMENTAL;
  const words = m.wordHighlight ? line.words : undefined;
  const spans = m.furigana?.[line.text];
  const lang = lineLanguage(line.text, m.language);
  return { key: m.keys[m.index], tokens: tokenize(text, start, end, words, spans), sub: translated, next, lang, subLang: m.target, ...langs };
}
