import { describe, expect, it, vi } from "vitest";
import {
  findLegacyKeys,
  fromLegacy,
  importLegacyKeys,
  LEGACY_CONFIG_KEY,
  LEGACY_OFFSETS_KEY,
  LEGACY_TRANSLATION_KEY,
  seedFromLegacy,
  SETTINGS_KEY,
  type KV,
  upgradeSettings,
} from "./migrate";
import { defaultSettings } from "./schema";

/** In-memory localStorage. */
function memory(init: Record<string, unknown> = {}): KV & { dump: () => Record<string, unknown> } {
  const m = new Map(Object.entries(init).map(([k, v]) => [k, JSON.stringify(v)]));
  return {
    getItem: (k) => m.get(k) ?? null,
    setItem: (k, v) => void m.set(k, v),
    dump: () => Object.fromEntries([...m].map(([k, v]) => [k, JSON.parse(v)])),
  };
}

/** What the v2 store (src/stores/configStore.ts, persist version 2) wrote. */
const v2State = {
  translation: {
    enabled: true,
    autoTranslate: true,
    config: {
      source_language: "ja",
      target_language: "zh-TW",
      style: "poetic",
      preserve_rhyme: false,
      preserve_meaning: true,
      maintain_structure: true,
      provider: "gemini",
      github_api_key: "ghp_secret",
      gemini_api_key: "AIza_secret",
    },
    display: {
      mode: "bilingual",
      fontSize: { original: 1, translated: 0.85 },
      opacity: { original: 1, translated: 0.9 },
      spacing: 8,
      colors: { original: "rgb(255, 255, 255)", translated: "rgb(200, 220, 255)" },
      animation: { enabled: true, type: "fade", duration: 300 },
    },
    showLanguageIndicator: true,
    showConfidenceScore: false,
  },
  lyricsStyle: {
    fontFamily: "Orbitron, monospace",
    fontSize: 2.2,
    fontWeight: "semibold",
    textColor: "#00f5ff",
    highlightColor: "#39ff14",
    pastWordColor: "#ff0080",
    futureWordColor: "#ffffff60",
    backgroundColor: "#0a0a0a",
    backgroundOpacity: 50,
    backgroundBlur: 0,
    textShadow: false,
    textShadowColor: "#000000",
    textShadowBlur: 20,
    textGlow: true,
    textGlowColor: "#00f5ff",
    lineHeight: 1.5,
    letterSpacing: 0.05,
    textAlign: "left",
    animationIntensity: 90,
  },
  hiragana: { enabled: true, displayMode: "ruby", rubyPosition: "bottom", opacity: 0.8, fontSize: 0.7, color: "#a0a0a0", spacing: 4 },
  viewMode: "plain",
  themeId: "classic",
  trackOffsets: { "artist - song": 0.5, "other - track": -1.25, "zero - entry": 0 },
  globalOffset: 0.3,
};

const envelope = (state: unknown, version = 2) => ({ state, version });

describe("fromLegacy", () => {
  const s = fromLegacy(v2State, null);

  it("maps theme, view mode and lyric style", () => {
    expect(s.theme).toBe("light");
    expect(s.wordHighlight).toBe(false);
    expect(s.lyric).toMatchObject({
      fontFamily: "Orbitron, monospace",
      fontSize: 2.2,
      fontWeight: 600,
      align: "left",
      textColor: "#00f5ff",
      activeColor: "#39ff14",
      sungColor: "#ff0080",
      upcomingColor: "#ffffff60",
      shadow: false,
      glow: true,
      glowColor: "#00f5ff",
      background: "#0a0a0a",
      backgroundOpacity: 50,
      lineHeight: 1.5,
      letterSpacing: 0.05,
    });
  });

  it("maps translation prefs, converting rgb() colours", () => {
    expect(s.translation).toMatchObject({
      enabled: true,
      sourceLanguage: "ja",
      targetLanguage: "zh-TW",
      style: "poetic",
      preserveRhyme: false,
      display: "bilingual",
      scale: 0.85,
      color: "#c8dcff",
      opacity: 0.9,
    });
  });

  it("only enables translation when the old pipeline would have translated", () => {
    const off = fromLegacy({ ...v2State, translation: { ...v2State.translation, autoTranslate: false } }, null);
    expect(off.translation.enabled).toBe(false);
    const original = fromLegacy({ ...v2State, translation: { ...v2State.translation, display: { mode: "original" } } }, null);
    expect(original.translation.enabled).toBe(false);
  });

  it("maps furigana prefs (ruby at the bottom → under)", () => {
    expect(s.furigana).toEqual({ enabled: true, mode: "under", scale: 0.7, opacity: 0.8, color: "#a0a0a0" });
    expect(fromLegacy({ hiragana: { displayMode: "replace" } }, null).furigana.mode).toBe("replace");
    expect(fromLegacy({ hiragana: { displayMode: "above" } }, null).furigana.mode).toBe("over");
  });

  it("keeps every non-zero track offset and the global offset", () => {
    expect(s.sync).toEqual({ globalOffset: 0.3, trackOffsets: { "artist - song": 0.5, "other - track": -1.25 } });
  });

  it("merges pre-v2 offsets under v2 ones", () => {
    const pre = { trackOffsets: { "artist - song": 9, "old - only": 0.2 }, globalOffset: 0.7 };
    const merged = fromLegacy({ ...v2State, globalOffset: 0 }, pre);
    expect(merged.sync.trackOffsets).toEqual({ "artist - song": 0.5, "other - track": -1.25, "old - only": 0.2 });
    expect(merged.sync.globalOffset).toBe(0.7);
  });

  it("falls back to defaults for garbage", () => {
    expect(fromLegacy({ lyricsStyle: { fontSize: "big", textColor: 42 }, trackOffsets: { a: "x" } }, null)).toEqual({
      ...defaultSettings,
      theme: defaultSettings.theme,
      // The old app had no backdrop: an upgraded setup keeps its look.
      lyric: { ...defaultSettings.lyric, scrim: 0 },
    });
  });
});

describe("seedFromLegacy", () => {
  it("writes lyryc-settings once and leaves the legacy entry in place", () => {
    const kv = memory({ [LEGACY_CONFIG_KEY]: envelope(v2State) });
    expect(seedFromLegacy(kv)).toBe(true);
    const out = kv.dump();
    expect(out[SETTINGS_KEY]).toMatchObject({ version: 2, state: { sync: { globalOffset: 0.3 } } });
    expect(out[LEGACY_CONFIG_KEY]).toEqual(envelope(v2State));
    expect(seedFromLegacy(kv)).toBe(false); // already migrated
  });

  it("does nothing for a fresh install or unreadable data", () => {
    expect(seedFromLegacy(memory())).toBe(false);
    const kv = memory();
    kv.setItem(LEGACY_CONFIG_KEY, "{not json");
    expect(seedFromLegacy(kv)).toBe(false);
  });

  it("leaves extra lyric sources off: the old app never asked", () => {
    const kv = memory({ [LEGACY_CONFIG_KEY]: envelope(v2State) });
    seedFromLegacy(kv);
    expect(kv.dump()[SETTINGS_KEY]).toMatchObject({ state: { extraSources: false } });
  });

  it("migrates pre-v2 offsets even without a v2 store", () => {
    const kv = memory({ [LEGACY_OFFSETS_KEY]: envelope({ trackOffsets: { "a - b": 1 }, globalOffset: 0 }, 1) });
    expect(seedFromLegacy(kv)).toBe(true);
    expect(kv.dump()[SETTINGS_KEY]).toMatchObject({ state: { sync: { trackOffsets: { "a - b": 1 } } } });
  });
});

describe("legacy API keys", () => {
  it("finds keys in the v2 store, falling back to the pre-v2 translation store", () => {
    expect(findLegacyKeys(memory({ [LEGACY_CONFIG_KEY]: envelope(v2State) }))).toEqual({ githubApiKey: "ghp_secret", geminiApiKey: "AIza_secret" });
    const pre = memory({ [LEGACY_TRANSLATION_KEY]: envelope({ config: { gemini_api_key: " old " } }, 1) });
    expect(findLegacyKeys(pre)).toEqual({ geminiApiKey: "old" });
    expect(findLegacyKeys(memory({ [LEGACY_CONFIG_KEY]: envelope({ translation: { config: { github_api_key: "" } } }) }))).toBeNull();
  });

  it("strips keys only after the backend accepted them", async () => {
    const kv = memory({
      [LEGACY_CONFIG_KEY]: envelope(v2State),
      [LEGACY_TRANSLATION_KEY]: envelope({ config: { github_api_key: "ghp_old", target_language: "en" } }, 1),
    });
    const send = vi.fn().mockResolvedValue({ providers: [], routes: {} });
    expect(await importLegacyKeys(kv, send)).toBe("imported");
    expect(send).toHaveBeenCalledWith({ githubApiKey: "ghp_secret", geminiApiKey: "AIza_secret" });

    const out = kv.dump() as Record<string, { state: Record<string, any> }>;
    expect(out[LEGACY_CONFIG_KEY].state.translation.config).not.toHaveProperty("github_api_key");
    expect(out[LEGACY_CONFIG_KEY].state.translation.config).not.toHaveProperty("gemini_api_key");
    expect(out[LEGACY_CONFIG_KEY].state.translation.config.target_language).toBe("zh-TW"); // rest untouched
    expect(out[LEGACY_CONFIG_KEY].state.trackOffsets).toEqual(v2State.trackOffsets);
    expect(out[LEGACY_TRANSLATION_KEY].state.config).toEqual({ target_language: "en" });

    expect(await importLegacyKeys(kv, send)).toBe("none"); // once
    expect(send).toHaveBeenCalledTimes(1);
  });

  it("keeps the keys when the backend call fails (e.g. command missing)", async () => {
    const kv = memory({ [LEGACY_CONFIG_KEY]: envelope(v2State) });
    const send = vi.fn().mockRejectedValue("command llm_import_legacy_keys not found");
    expect(await importLegacyKeys(kv, send)).toBe("failed");
    expect(findLegacyKeys(kv)).toEqual({ githubApiKey: "ghp_secret", geminiApiKey: "AIza_secret" });
  });
});

describe("upgradeSettings", () => {
  it("keeps an existing setup's look: the soft backdrop stays off for v1 settings", () => {
    const v1 = { theme: "dark", lyric: { fontSize: 3, backgroundOpacity: 20 } };
    expect(upgradeSettings(v1, 1)).toEqual({ theme: "dark", lyric: { fontSize: 3, backgroundOpacity: 20, scrim: 0 } });
  });

  it("leaves current settings alone", () => {
    const v2 = { lyric: { scrim: 0.4 } };
    expect(upgradeSettings(v2, 2)).toEqual(v2);
  });
});
