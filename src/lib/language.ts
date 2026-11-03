/** Lyric language detection and the "which songs get translated" rule. */
import type { LanguageFilter } from "../state/schema";

/** Unknown: no letters, or Latin text with no recognisable common words. */
export const UNKNOWN = "und";

const count = (re: RegExp, s: string) => s.match(re)?.length ?? 0;

/** Characters that only Traditional Chinese uses (frequent ones). */
const TRADITIONAL = /[這們個來時會過說對於國學應開關電話語見間樣愛讓還妳嗎當為與從後裡麼]/;

/** Non-Latin scripts, judged per line. Kana comes first: Japanese lines mix in kanji. */
const SCRIPTS: Array<[code: string, re: RegExp]> = [
  ["ja", /[぀-ヿ]/g],
  ["ko", /[가-힯ᄀ-ᇿ]/g],
  ["zh", /[㐀-䶿一-鿿]/g],
  ["ru", /[Ѐ-ӿ]/g],
  ["el", /[Ͱ-Ͽ]/g],
  ["he", /[֐-׿]/g],
  ["ar", /[؀-ۿ]/g],
  ["hi", /[ऀ-ॿ]/g],
  ["th", /[฀-๿]/g],
];

/** Common short words: Latin-script languages are told apart by these. */
const WORDS: Array<[code: string, words: Set<string>]> = [
  ["en", new Set("the and you to is it me my of in that your on for with what this i'm we be".split(" "))],
  ["es", new Set("el la de que y en los las un una mi tu no me te se por con es yo".split(" "))],
  ["fr", new Set("le la les de et je tu un une que pas est des mon ma dans pour qui ne moi".split(" "))],
  ["de", new Set("der die das und ich du ist nicht ein eine mich dich mit zu auf wir es sie den dem".split(" "))],
  ["pt", new Set("de que não um uma eu você meu minha do da em com se por os as é tu".split(" "))],
  ["it", new Set("il di che non un una io tu mi ti la le per con sono è del della ma se".split(" "))],
];

/** A line's script: the non-Latin script with kana/kanji taking priority, else "latin". */
function lineScript(line: string): string {
  if (count(SCRIPTS[0][1], line) > 0) return "ja";
  let best = "latin";
  let most = 0;
  for (const [code, re] of SCRIPTS.slice(1)) {
    const n = count(re, line);
    if (n > most) [best, most] = [code, n];
  }
  // Two or three CJK characters outweigh a line of Latin letters; anything else needs a majority.
  return most > 0 && (best === "ko" || best === "zh" || most >= count(/\p{L}/gu, line) / 2) ? best : "latin";
}

function latinLanguage(text: string): string {
  const words = text.toLowerCase().split(/[^\p{L}']+/u).filter(Boolean);
  let best = UNKNOWN;
  let most = 0;
  for (const [code, set] of WORDS) {
    const n = words.filter((w) => set.has(w)).length;
    if (n > most) [best, most] = [code, n];
  }
  return best;
}

/**
 * The song's language from its lines. Each line votes for its script, so a few
 * English hooks in a J-pop or K-pop song don't make it English. Kana in 15% of
 * lines is enough for Japanese (its other lines are often kanji-only).
 */
export function detectLanguage(lines: readonly string[]): string {
  const sample = lines.map((l) => l.trim()).filter((l) => /\p{L}/u.test(l)).slice(0, 60);
  if (sample.length === 0) return UNKNOWN;
  const votes = new Map<string, number>();
  for (const line of sample) {
    const script = lineScript(line);
    votes.set(script, (votes.get(script) ?? 0) + 1);
  }
  const share = (code: string) => (votes.get(code) ?? 0) / sample.length;

  if (share("ja") >= 0.15) return "ja";
  const [script, n] = [...votes].filter(([c]) => c !== "latin").sort((a, b) => b[1] - a[1])[0] ?? ["latin", 0];
  if (n / sample.length >= 0.25) {
    return script === "zh" && TRADITIONAL.test(sample.join("")) ? "zh-tw" : script;
  }
  return latinLanguage(sample.filter((l) => lineScript(l) === "latin").join(" "));
}

/**
 * Codes as families: lyrics show Simplified vs Traditional Chinese, never
 * Mainland vs Taiwan vs Hong Kong.
 */
export function languageFamily(code: string): string {
  const c = code.trim().toLowerCase();
  if (c === "zh-cn" || c === "zh-hans") return "zh";
  if (c === "zh-hk" || c === "zh-hant") return "zh-tw";
  return c;
}

/**
 * Whether a song in `lang` gets translated. A song already in the target
 * language never does; otherwise the user's filter decides.
 */
export function shouldTranslate(lang: string, target: string, filter: LanguageFilter): boolean {
  const family = languageFamily(lang);
  if (family !== UNKNOWN && family === languageFamily(target)) return false;
  const listed = filter.languages.some((l) => languageFamily(l) === family);
  if (filter.mode === "only") return listed;
  if (filter.mode === "except") return !listed;
  return true;
}
