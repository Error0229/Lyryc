import { describe, expect, it } from "vitest";
import { defaultSettings } from "./schema";
import { mergeSettings } from "./settings";

describe("mergeSettings (loading lyryc-settings)", () => {
  it("gives settings saved before extraSources existed the default: off", () => {
    const { extraSources: _, ...saved } = { ...defaultSettings, theme: "dark" as const, wordHighlight: false };
    const s = mergeSettings(saved);
    expect(s.extraSources).toBe(false);
    expect(s.theme).toBe("dark");
    expect(s.wordHighlight).toBe(false);
  });

  it("keeps the user's choice once saved", () => {
    expect(mergeSettings({ ...defaultSettings, extraSources: true }).extraSources).toBe(true);
  });

  it("is off for a fresh install", () => {
    expect(mergeSettings(undefined).extraSources).toBe(false);
  });

  it("gives translation prefs saved before the language filter existed: translate all", () => {
    const { filter: _, ...translation } = { ...defaultSettings.translation, targetLanguage: "zh-tw" };
    const s = mergeSettings({ ...defaultSettings, translation });
    expect(s.translation.filter).toEqual({ mode: "all", languages: [] });
    expect(s.translation.targetLanguage).toBe("zh-tw");
  });
});
