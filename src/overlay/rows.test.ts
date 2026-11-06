import { describe, expect, it } from "vitest";
import { defaultSettings } from "../state/schema";
import { NO_ROWS, rowsEm, songRows, stripHeight, wantsTranslation } from "./rows";

const ja = [{ time: 0, text: "君の声を辿って" }, { time: 4, text: "遠回りしてきた" }, { time: 8, text: "夜明けのプリズム" }];
const en = [{ time: 0, text: "I followed your voice" }, { time: 4, text: "all the long way round" }];
const fur = { ...defaultSettings.furigana, enabled: true };
const tr = { ...defaultSettings.translation, enabled: true, targetLanguage: "zh-tw" };

describe("songRows", () => {
  it("reserves nothing until there are lyrics", () => {
    expect(songRows([], fur, tr, true)).toEqual(NO_ROWS);
  });

  it("reserves the next-line row only when it is on", () => {
    expect(songRows(en, fur, tr).next).toBe(false);
    expect(songRows(en, fur, tr, true).next).toBe(true);
  });

  it("reserves furigana for Japanese songs only", () => {
    expect(songRows(ja, fur, tr).furigana).toBe(true);
    expect(songRows(en, fur, tr).furigana).toBe(false);
    expect(songRows(ja, { ...fur, enabled: false }, tr).furigana).toBe(false);
    // "replace" swaps the text for its reading: no row above.
    expect(songRows(ja, { ...fur, mode: "replace" }, tr).furigana).toBe(false);
  });

  it("reserves the translation row before the translation arrives", () => {
    expect(songRows(ja, fur, tr).translation).toBe(true);
    expect(songRows(ja, fur, { ...tr, enabled: false }).translation).toBe(false);
    // Only "bilingual" stacks a second line.
    expect(songRows(ja, fur, { ...tr, display: "translated" }).translation).toBe(false);
    expect(songRows(ja, fur, { ...tr, display: "side-by-side" }).translation).toBe(false);
  });

  it("skips the translation row when the song won't be translated", () => {
    expect(songRows(en, fur, { ...tr, targetLanguage: "en" }).translation).toBe(false);
    expect(songRows(ja, fur, { ...tr, filter: { mode: "except", languages: ["ja"] } }).translation).toBe(false);
    expect(wantsTranslation(ja, { ...tr, filter: { mode: "only", languages: ["ja"] } })).toBe(true);
  });
});

describe("stripHeight", () => {
  const lyric = defaultSettings.lyric; // 2.5rem, line height 1.4

  it("adds each reserved row to the line", () => {
    expect(rowsEm(NO_ROWS, 1.4, 0.5, 0.6)).toBeCloseTo(1.4);
    expect(rowsEm({ furigana: true, translation: true, next: false }, 1.4, 0.5, 0.6)).toBeCloseTo(1.4 + 0.55 + 0.93);
    expect(rowsEm({ ...NO_ROWS, next: true }, 1.4, 0.5, 0.6)).toBeCloseTo(1.4 + 0.62 * 1.3);
  });

  it("is the stage plus padding, in px", () => {
    expect(stripHeight(NO_ROWS, lyric, fur, tr, 16)).toBe(72); // 1.4 × 40 + 16
    expect(stripHeight({ furigana: true, translation: true, next: false }, lyric, fur, tr, 16)).toBe(132); // (1.4 + 0.55 + 0.93) × 40 + 16 = 131.2, up to even
    expect(stripHeight(NO_ROWS, { ...lyric, lineHeight: 1.45 }, fur, tr, 16) % 2).toBe(0); // 74 from 73.6, never odd
  });

  it("does not depend on what is showing, only on the rows", () => {
    const a = stripHeight(songRows(ja, fur, tr), lyric, fur, tr, 16);
    const b = stripHeight(songRows(ja.slice(1), fur, tr), lyric, fur, tr, 16);
    expect(a).toBe(b);
  });
});
