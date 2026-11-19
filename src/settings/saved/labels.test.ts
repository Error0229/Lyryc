import { describe, expect, it } from "vitest";
import type { MadeWith, SavedTranslation } from "../../services/ipc/backend";
import { translationMadeWith, translationNames } from "./labels";

const NAMES: Record<string, string> = { en: "English", "zh-TW": "Chinese Traditional" };
const language = (code: string) => NAMES[code] ?? code;

const base: MadeWith = {
  target_language: "en",
  style: "natural",
  custom_prompt: null,
  preserve_rhyme: true,
  preserve_meaning: true,
  maintain_structure: true,
  provider: "GitHub",
  model: "gpt-4.1",
};
const NOV_3 = Date.UTC(2026, 10, 3, 12) / 1000;
const tr = (m: Partial<MadeWith> | null, cachedAt = NOV_3): SavedTranslation => ({ key: "k", lines: [], madeWith: m && { ...base, ...m }, cachedAt });

describe("translationNames", () => {
  it("names a lone translation by its language", () => {
    expect(translationNames([tr({})], language)).toEqual(["English"]);
  });

  it("names copies by what tells them apart, and nothing else", () => {
    expect(translationNames([tr({ style: "poetic" }), tr({})], language)).toEqual(["Poetic style", "Natural style"]);
    expect(translationNames([tr({ target_language: "zh-TW", model: "o3" }), tr({})], language)).toEqual(["Chinese Traditional · o3 (GitHub)", "English · gpt-4.1 (GitHub)"]);
    expect(translationNames([tr({ style: "custom", custom_prompt: "Keep it playful and short, like a children's song" }), tr({})], language)).toEqual([
      'Prompt "Keep it playful and…"',
      "Natural style",
    ]);
  });

  it("says when a copy differs only by a switch", () => {
    expect(translationNames([tr({ preserve_rhyme: false }), tr({})], language)).toEqual(["Not keeping rhyme", "English"]);
  });

  it("calls one from before settings were recorded what it is", () => {
    expect(translationNames([tr({}), tr(null)], language)).toEqual(["English", "Earlier settings"]);
  });

  it("tells same-looking copies apart by date", () => {
    const names = translationNames([tr(null, NOV_3), tr(null, NOV_3 + 86_400 * 5)], language);
    expect(names[0]).not.toBe(names[1]);
    expect(names[0].startsWith("Earlier settings · ")).toBe(true);
  });
});

describe("translationMadeWith", () => {
  it("says everything that made it", () => {
    const said = translationMadeWith(tr({ style: "poetic", maintain_structure: false }), language);
    expect(said.startsWith("English · poetic style · gpt-4.1 (GitHub) · not keeping structure · ")).toBe(true);
    expect(translationMadeWith(tr(null), language).startsWith("Made before its settings were recorded · ")).toBe(true);
  });
});
