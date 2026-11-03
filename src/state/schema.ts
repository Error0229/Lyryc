/** Persisted settings shape (localStorage `lyryc-settings`, version 1). */

export type UiTheme = "system" | "light" | "dark";
export type FontWeight = 400 | 500 | 600 | 700;
export type TextAlign = "left" | "center" | "right";
export type TranslationDisplay = "bilingual" | "translated" | "side-by-side";
export type TranslationStyle = "natural" | "literal" | "poetic" | "rhyming" | "custom";
export type FuriganaMode = "over" | "under" | "replace";

export interface LyricStyle {
  fontFamily: string;
  fontSize: number; // rem
  fontWeight: FontWeight;
  lineHeight: number;
  letterSpacing: number; // em
  align: TextAlign;
  textColor: string; // plain mode / untimed lines
  activeColor: string; // the sweep
  sungColor: string; // words already sung
  upcomingColor: string; // words not yet sung
  shadow: boolean;
  shadowColor: string;
  shadowBlur: number; // px
  glow: boolean;
  glowColor: string;
  background: string;
  backgroundOpacity: number; // 0..100
}

/** Which songs get translated, by lyric language. "all" ignores `languages`. */
export type LanguageFilterMode = "all" | "only" | "except";
export interface LanguageFilter {
  mode: LanguageFilterMode;
  languages: string[]; // language codes, e.g. "ja", "zh-tw"
}

export interface TranslationPrefs {
  enabled: boolean;
  sourceLanguage: string;
  targetLanguage: string;
  style: TranslationStyle;
  customPrompt: string;
  preserveRhyme: boolean;
  preserveMeaning: boolean;
  maintainStructure: boolean;
  display: TranslationDisplay;
  scale: number; // relative to lyric size
  color: string;
  opacity: number; // 0..1
  filter: LanguageFilter;
}

export interface FuriganaPrefs {
  enabled: boolean;
  mode: FuriganaMode;
  scale: number; // relative to lyric size
  opacity: number; // 0..1
  color: string;
}

export interface SyncPrefs {
  globalOffset: number;
  trackOffsets: Record<string, number>;
}

export interface Settings {
  theme: UiTheme;
  wordHighlight: boolean;
  /** Ask QQ Music / NetEase / Kugou for timing LRCLIB lacks. Opt-in: unofficial endpoints. */
  extraSources: boolean;
  /** When no source finds a song, let the AI provider search the web to identify it. Opt-in: paid. */
  identifyWithSearch: boolean;
  lyric: LyricStyle;
  translation: TranslationPrefs;
  furigana: FuriganaPrefs;
  sync: SyncPrefs;
}

export const defaultLyric: LyricStyle = {
  fontFamily: "Inter, system-ui, sans-serif",
  fontSize: 2.5,
  fontWeight: 500,
  lineHeight: 1.4,
  letterSpacing: 0,
  align: "center",
  textColor: "#ffffff",
  activeColor: "#ffffff",
  sungColor: "#ffffff",
  upcomingColor: "#ffffff73",
  shadow: true,
  shadowColor: "#000000",
  shadowBlur: 16,
  glow: false,
  glowColor: "#ffffff",
  background: "#000000",
  backgroundOpacity: 0,
};

export const defaultSettings: Settings = {
  theme: "system",
  wordHighlight: true,
  extraSources: false,
  identifyWithSearch: false,
  lyric: defaultLyric,
  translation: {
    enabled: false,
    sourceLanguage: "auto",
    targetLanguage: "en",
    style: "natural",
    customPrompt: "",
    preserveRhyme: true,
    preserveMeaning: true,
    maintainStructure: true,
    display: "bilingual",
    scale: 0.6,
    color: "#dbe4ff",
    opacity: 0.9,
    filter: { mode: "all", languages: [] },
  },
  furigana: {
    enabled: false,
    mode: "over",
    scale: 0.5,
    opacity: 0.85,
    color: "#ffffff",
  },
  sync: { globalOffset: 0, trackOffsets: {} },
};
