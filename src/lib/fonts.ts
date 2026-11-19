/**
 * Lyric fonts. A font setting is a CSS font-family list: the pickers write one
 * installed family, quoted (`"Yu Gothic UI"`). Some languages can have their
 * own, ahead of the main font, which still covers whatever it lacks.
 */
import type { SystemFont } from "../services/ipc/backend";
import { UNKNOWN } from "./language";

/** One family name as a CSS font-family value. */
export const cssFamily = (name: string) => `"${name.replace(/["\\]/g, "\\$&")}"`;

/**
 * The languages that can have a font of their own: every code `lineLanguage`
 * names a line by, which is also the script code the backend lists each
 * installed font under (src-tauri/src/fonts.rs SCRIPTS).
 */
export const FONT_LANGUAGES: ReadonlyArray<{ code: string; name: string; sample: string }> = [
  { code: "ja", name: "Japanese", sample: "夜明けの駅で君を待つ" },
  { code: "ko", name: "Korean", sample: "새벽 역에서 너를 기다려" },
  { code: "zh", name: "Chinese (Simplified)", sample: "在黎明的车站等你" },
  { code: "zh-tw", name: "Chinese (Traditional)", sample: "在黎明的車站等你" },
  { code: "ru", name: "Cyrillic", sample: "Жду тебя на рассвете" },
  { code: "el", name: "Greek", sample: "Σε περιμένω την αυγή" },
  { code: "he", name: "Hebrew", sample: "מחכה לך עם שחר" },
  { code: "ar", name: "Arabic", sample: "أنتظرك عند الفجر" },
  { code: "hi", name: "Hindi", sample: "भोर में तुम्हारा इंतज़ार" },
  { code: "th", name: "Thai", sample: "รอเธอตอนรุ่งสาง" },
];

/** The installed fonts that write `script` ("latin", or a FONT_LANGUAGES code), as picker choices. */
export function fontsFor(installed: readonly SystemFont[], script: string): Array<{ value: string; label: string }> {
  return installed.filter((f) => f.scripts.includes(script)).map((f) => ({ value: cssFamily(f.name), label: f.name }));
}

const HTML_LANG: Record<string, string> = { zh: "zh-Hans", "zh-tw": "zh-Hant" };

/**
 * The `lang` attribute for text in this language family: it picks the right
 * CJK glyphs (Japanese 直, not Chinese) from fonts that carry both.
 */
export function htmlLang(code: string): string | undefined {
  return code === UNKNOWN ? undefined : (HTML_LANG[code] ?? code);
}

/** The font for text in `code`: its own when set (lyric.css var from `stageVars`), else the main one. */
export const fontVar = (code: string) => `var(--lyric-font-${code}, var(--lyric-font))`;

/** `fonts` with `code`'s font set, or gone (null: the main font). */
export function withFont(fonts: Readonly<Record<string, string>>, code: string, font: string | null): Record<string, string> {
  const { [code]: _, ...rest } = fonts;
  return font ? { ...rest, [code]: font } : rest;
}
