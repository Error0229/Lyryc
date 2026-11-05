import { describe, expect, it } from "vitest";
import { lineKeys } from "../lib/lyrics";
import { defaultSettings } from "../state/schema";
import { lineModel, type ModelInput } from "./model";
import { NO_ROWS } from "./rows";
import { stageVars, textShadow } from "./style";

const lines = [
  { time: 1, text: "Hello world", words: [{ time: 1, text: "Hello", duration: 0.5 }, { time: 2, text: "world", duration: 0.5 }] },
  { time: 4, text: "" },
  { time: 6, text: "駅で待つ" },
];

const input = (over: Partial<ModelInput> = {}): ModelInput => ({
  lines,
  keys: lineKeys(lines),
  index: 0,
  wordHighlight: true,
  translations: null,
  display: "bilingual",
  furigana: null,
  ...over,
});

describe("lineModel", () => {
  it("is null before the first line", () => {
    expect(lineModel(input({ index: -1 }))).toBeNull();
  });

  it("uses word timing only when highlighting is on", () => {
    expect(lineModel(input())!.tokens).toHaveLength(3);
    expect(lineModel(input({ wordHighlight: false }))!.tokens).toHaveLength(1);
  });

  it("shows ♪ for instrumental lines", () => {
    expect(lineModel(input({ index: 1 }))!.tokens[0].text).toBe("♪");
  });

  it("attaches the translation by text, or swaps it in for translation-only", () => {
    const translations = { "hello world": "Hola mundo" };
    expect(lineModel(input({ translations }))!.sub).toBe("Hola mundo");
    const only = lineModel(input({ translations, display: "translated" }))!;
    expect(only.sub).toBeNull();
    expect(only.tokens.map((t) => t.text).join("")).toBe("Hola mundo");
  });

  it("applies furigana spans for the line text", () => {
    const furigana = { 駅で待つ: [{ text: "駅", reading: "えき" }, { text: "で" }, { text: "待", reading: "ま" }, { text: "つ" }] };
    const m = lineModel(input({ index: 2, furigana }))!;
    expect(m.tokens.map((t) => t.reading ?? null)).toEqual(["えき", null, "ま", null]);
  });
});

describe("stageVars", () => {
  const base = { lyric: defaultSettings.lyric, translation: defaultSettings.translation, furigana: defaultSettings.furigana, rows: NO_ROWS };

  it("makes the stage as tall as the song's rows, whatever is showing", () => {
    const min = (rows: typeof NO_ROWS) => parseFloat(String(stageVars({ ...base, rows })["--stage-min" as never]));
    const plain = min(NO_ROWS);
    expect(plain).toBeCloseTo(defaultSettings.lyric.lineHeight);
    expect(min({ ...NO_ROWS, furigana: true })).toBeGreaterThan(plain);
    expect(min({ ...NO_ROWS, translation: true })).toBeGreaterThan(plain);
    expect(min({ ...NO_ROWS, next: true })).toBeGreaterThan(plain);
  });

  it("maps background opacity to an alpha colour", () => {
    const v = stageVars({ ...base, lyric: { ...base.lyric, background: "#000000", backgroundOpacity: 50 } });
    expect(v["--strip-bg" as never]).toBe("#00000080");
  });

  it("turns the shadow off cleanly", () => {
    expect(textShadow(false, "#000", 10)).toBe("none");
    expect(textShadow(true, "#000000", 0)).toBe("none");
    expect(textShadow(true, "#000000", 16)).toContain("16px");
  });
});
