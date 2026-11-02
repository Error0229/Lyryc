const KANA = /[぀-ヿ]/;
const KANJI = /[㐀-䶿一-鿿]/;

export const hasKanji = (s: string) => KANJI.test(s);

/**
 * Kanji alone could be Chinese; a song counts as Japanese only if some line
 * has kana. Furigana is generated only for those songs.
 */
export const isJapaneseSong = (lines: ReadonlyArray<{ text: string }>) => lines.some((l) => KANA.test(l.text));
