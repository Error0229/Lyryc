/**
 * One-way migration from the old frontend's localStorage into `lyryc-settings`.
 *
 * Sources, newest wins:
 *   lyryc-config               zustand persist v2 (the consolidated store)
 *   lyrics-offset-storage      pre-v2 offset store (offsets only)
 *   lyryc-translation-settings pre-v2 translation store (API keys only)
 *
 * Legacy entries are never deleted, so an older build still finds its data.
 * Only the API key fields are stripped, and only after the backend confirms
 * it imported them.
 */
import type { LegacyKeys } from "../services/ipc/llm";
import { toHex } from "../lib/color";
import { normalizeOffset } from "../lib/offset";
import { defaultSettings, type FontWeight, type FuriganaMode, type Settings, type TranslationStyle } from "./schema";

export const SETTINGS_KEY = "lyryc-settings";
export const SETTINGS_VERSION = 1;
export const LEGACY_CONFIG_KEY = "lyryc-config";
export const LEGACY_OFFSETS_KEY = "lyrics-offset-storage";
export const LEGACY_TRANSLATION_KEY = "lyryc-translation-settings";

export type KV = Pick<Storage, "getItem" | "setItem">;
type Obj = Record<string, unknown>;

const obj = (v: unknown): Obj => (v && typeof v === "object" && !Array.isArray(v) ? (v as Obj) : {});
const num = (v: unknown, fb: number, min = -Infinity, max = Infinity) =>
  typeof v === "number" && Number.isFinite(v) ? Math.min(max, Math.max(min, v)) : fb;
const str = (v: unknown, fb: string) => (typeof v === "string" && v.trim() ? v : fb);
const bool = (v: unknown, fb: boolean) => (typeof v === "boolean" ? v : fb);
const color = (v: unknown, fb: string) => toHex(v) ?? fb;
const oneOf = <T extends string>(v: unknown, all: readonly T[], fb: T): T => (all.includes(v as T) ? (v as T) : fb);

/** Parse a zustand-persist envelope and return its `state`. */
function persisted(kv: KV, key: string): Obj | null {
  const raw = kv.getItem(key);
  if (!raw) return null;
  try {
    return obj(obj(JSON.parse(raw)).state);
  } catch {
    return null;
  }
}

const WEIGHTS: Record<string, FontWeight> = { normal: 400, medium: 500, semibold: 600, bold: 700 };
const THEMES: Record<string, Settings["theme"]> = { classic: "light", dark: "dark", neon: "dark", karaoke: "dark" };
const STYLES: readonly TranslationStyle[] = ["natural", "literal", "poetic", "rhyming", "custom"];

function furiganaMode(h: Obj): FuriganaMode {
  if (h.displayMode === "replace") return "replace";
  if (h.displayMode === "below") return "under";
  if (h.displayMode === "ruby" && h.rubyPosition === "bottom") return "under";
  return "over";
}

function offsets(v: unknown): Record<string, number> {
  const out: Record<string, number> = {};
  for (const [k, n] of Object.entries(obj(v))) {
    const o = normalizeOffset(num(n, 0));
    if (o !== 0) out[k] = o;
  }
  return out;
}

/** Map the v2 store (plus pre-v2 offsets) onto the new shape. Pure. */
export function fromLegacy(v2: Obj | null, oldOffsets: Obj | null): Settings {
  const d = defaultSettings;
  const s = v2 ?? {};
  const style = obj(s.lyricsStyle);
  const tr = obj(s.translation);
  const cfg = obj(tr.config);
  const disp = obj(tr.display);
  const size = obj(disp.fontSize);
  const op = obj(disp.opacity);
  const cols = obj(disp.colors);
  const hira = obj(s.hiragana);
  const pre = oldOffsets ?? {};

  const v2Global = num(s.globalOffset, 0);
  const translatedScale =
    size.translated === undefined
      ? d.translation.scale
      : num(num(size.translated, 0.85) / num(size.original, 1, 0.1), d.translation.scale, 0.3, 1.2);

  return {
    theme: THEMES[String(s.themeId)] ?? d.theme,
    wordHighlight: s.viewMode !== "plain",
    lyric: {
      fontFamily: str(style.fontFamily, d.lyric.fontFamily),
      fontSize: num(style.fontSize, d.lyric.fontSize, 0.5, 6),
      fontWeight: WEIGHTS[String(style.fontWeight)] ?? d.lyric.fontWeight,
      lineHeight: num(style.lineHeight, d.lyric.lineHeight, 0.8, 3),
      letterSpacing: num(style.letterSpacing, d.lyric.letterSpacing, -0.2, 0.5),
      align: oneOf(style.textAlign, ["left", "center", "right"] as const, d.lyric.align),
      textColor: color(style.textColor, d.lyric.textColor),
      activeColor: color(style.highlightColor, d.lyric.activeColor),
      sungColor: color(style.pastWordColor, d.lyric.sungColor),
      upcomingColor: color(style.futureWordColor, d.lyric.upcomingColor),
      shadow: bool(style.textShadow, d.lyric.shadow),
      shadowColor: color(style.textShadowColor, d.lyric.shadowColor),
      shadowBlur: num(style.textShadowBlur, d.lyric.shadowBlur, 0, 64),
      glow: bool(style.textGlow, d.lyric.glow),
      glowColor: color(style.textGlowColor, d.lyric.glowColor),
      background: color(style.backgroundColor, d.lyric.background),
      backgroundOpacity: num(style.backgroundOpacity, d.lyric.backgroundOpacity, 0, 100),
    },
    translation: {
      // The old pipeline only translated when both flags were on.
      enabled: bool(tr.enabled, false) && bool(tr.autoTranslate, false) && disp.mode !== "original",
      sourceLanguage: str(cfg.source_language, d.translation.sourceLanguage),
      targetLanguage: str(cfg.target_language, d.translation.targetLanguage),
      style: oneOf(cfg.style, STYLES, d.translation.style),
      customPrompt: typeof cfg.custom_prompt === "string" ? cfg.custom_prompt : "",
      preserveRhyme: bool(cfg.preserve_rhyme, d.translation.preserveRhyme),
      preserveMeaning: bool(cfg.preserve_meaning, d.translation.preserveMeaning),
      maintainStructure: bool(cfg.maintain_structure, d.translation.maintainStructure),
      display: oneOf(disp.mode, ["bilingual", "translated", "side-by-side"] as const, d.translation.display),
      scale: translatedScale,
      color: color(cols.translated, d.translation.color),
      opacity: num(op.translated, d.translation.opacity, 0.1, 1),
    },
    furigana: {
      enabled: bool(hira.enabled, d.furigana.enabled),
      mode: furiganaMode(hira),
      scale: num(hira.fontSize, d.furigana.scale, 0.3, 1),
      opacity: num(hira.opacity, d.furigana.opacity, 0.1, 1),
      color: color(hira.color, d.furigana.color),
    },
    sync: {
      // v2 wrote a 0 default even when the user never touched it; a non-zero
      // pre-v2 value is the better signal in that case.
      globalOffset: normalizeOffset(v2Global !== 0 ? v2Global : num(pre.globalOffset, 0)),
      trackOffsets: { ...offsets(pre.trackOffsets), ...offsets(s.trackOffsets) },
    },
  };
}

/**
 * Seed `lyryc-settings` from legacy data the first time the new frontend runs.
 * Returns true when it wrote something.
 */
export function seedFromLegacy(kv: KV): boolean {
  if (kv.getItem(SETTINGS_KEY)) return false;
  const v2 = persisted(kv, LEGACY_CONFIG_KEY);
  const pre = persisted(kv, LEGACY_OFFSETS_KEY);
  if (!v2 && !pre) return false;
  kv.setItem(SETTINGS_KEY, JSON.stringify({ state: fromLegacy(v2, pre), version: SETTINGS_VERSION }));
  return true;
}

// ---- API keys -------------------------------------------------------------

/** Where each legacy store kept its translation config. */
const KEY_SOURCES: Array<{ key: string; path: string[] }> = [
  { key: LEGACY_CONFIG_KEY, path: ["translation", "config"] },
  { key: LEGACY_TRANSLATION_KEY, path: ["config"] },
];

const dig = (o: Obj, path: string[]) => path.reduce<Obj>((acc, k) => obj(acc[k]), o);
const keyOf = (v: unknown) => (typeof v === "string" && v.trim() ? v.trim() : undefined);

export function findLegacyKeys(kv: KV): LegacyKeys | null {
  const keys: LegacyKeys = {};
  for (const src of KEY_SOURCES) {
    const cfg = dig(persisted(kv, src.key) ?? {}, src.path);
    keys.githubApiKey ??= keyOf(cfg.github_api_key);
    keys.geminiApiKey ??= keyOf(cfg.gemini_api_key);
  }
  if (!keys.githubApiKey) delete keys.githubApiKey;
  if (!keys.geminiApiKey) delete keys.geminiApiKey;
  return keys.githubApiKey || keys.geminiApiKey ? keys : null;
}

/** Remove the key fields from every legacy envelope, leaving the rest intact. */
export function stripLegacyKeys(kv: KV): void {
  for (const src of KEY_SOURCES) {
    const raw = kv.getItem(src.key);
    if (!raw) continue;
    try {
      const env = JSON.parse(raw);
      const cfg = dig(obj(env?.state), src.path);
      if (!("github_api_key" in cfg) && !("gemini_api_key" in cfg)) continue;
      delete cfg.github_api_key;
      delete cfg.gemini_api_key;
      kv.setItem(src.key, JSON.stringify(env));
    } catch {
      // Unreadable envelope: leave it alone.
    }
  }
}

export type ImportOutcome = "none" | "imported" | "failed";

/** Hand legacy keys to the backend; strip them only once it accepted them. */
export async function importLegacyKeys(
  kv: KV,
  send: (keys: LegacyKeys) => Promise<unknown>,
): Promise<ImportOutcome> {
  const keys = findLegacyKeys(kv);
  if (!keys) return "none";
  try {
    await send(keys);
  } catch {
    return "failed";
  }
  stripLegacyKeys(kv);
  return "imported";
}
