/**
 * Persisted user settings, shared by both windows through localStorage.
 * State is plain data; mutations are the small functions below.
 */
import { create } from "zustand";
import { persist } from "zustand/middleware";
import { normalizeOffset, withTrackOffset } from "../lib/offset";
import { SETTINGS_KEY, SETTINGS_VERSION, seedFromLegacy } from "./migrate";
import { defaultLyric, defaultSettings, type FuriganaPrefs, type LyricStyle, type Settings, type TranslationPrefs, type UiTheme } from "./schema";

function storage(): Storage | null {
  try {
    return typeof localStorage === "undefined" ? null : localStorage;
  } catch {
    return null;
  }
}

const kv = storage();
if (kv) seedFromLegacy(kv);

/** Section-wise merge so fields added later get their defaults. */
export function mergeSettings(saved: unknown, base: Settings = defaultSettings): Settings {
  const s = (saved ?? {}) as Partial<Settings>;
  return {
    ...base,
    ...s,
    lyric: { ...base.lyric, ...s.lyric },
    translation: { ...base.translation, ...s.translation },
    furigana: { ...base.furigana, ...s.furigana },
    sync: { ...base.sync, ...s.sync, trackOffsets: { ...s.sync?.trackOffsets } },
  };
}

export const useSettings = create<Settings>()(
  persist(() => defaultSettings, {
    name: SETTINGS_KEY,
    version: SETTINGS_VERSION,
    merge: (saved, current) => mergeSettings(saved, current),
  }),
);

const set = useSettings.setState;

export const settings = {
  setTheme: (theme: UiTheme) => set({ theme }),
  setWordHighlight: (wordHighlight: boolean) => set({ wordHighlight }),
  setExtraSources: (extraSources: boolean) => set({ extraSources }),
  setPrealign: (prealign: boolean) => set({ prealign }),
  lyric: (p: Partial<LyricStyle>) => set((s) => ({ lyric: { ...s.lyric, ...p } })),
  resetLyric: () => set({ lyric: defaultLyric }),
  translation: (p: Partial<TranslationPrefs>) => set((s) => ({ translation: { ...s.translation, ...p } })),
  furigana: (p: Partial<FuriganaPrefs>) => set((s) => ({ furigana: { ...s.furigana, ...p } })),
  setGlobalOffset: (v: number) => set((s) => ({ sync: { ...s.sync, globalOffset: normalizeOffset(v) } })),
  setTrackOffset: (key: string, v: number) =>
    set((s) => ({ sync: { ...s.sync, trackOffsets: withTrackOffset(s.sync.trackOffsets, key, v) } })),
  clearOffsets: () => set({ sync: { globalOffset: 0, trackOffsets: {} } }),
  resetAll: () => set(defaultSettings),
};

// The other window wrote; pull its state in.
if (typeof window !== "undefined") {
  window.addEventListener("storage", (e) => {
    if (e.key === SETTINGS_KEY) void useSettings.persist.rehydrate();
  });
}
