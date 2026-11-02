import { describe, expect, it } from "vitest";
import { activeLineIndex, lineEnd, timeAt, tokenize, wordKnots } from "./timeline";

const lines = [
  { time: 1, text: "a" },
  { time: 3, text: "b" },
  { time: 3, text: "c" },
  { time: 7, text: "d", duration: 2 },
];

describe("activeLineIndex", () => {
  it("is -1 before the first line and with no lines", () => {
    expect(activeLineIndex(lines, 0.99)).toBe(-1);
    expect(activeLineIndex([], 10)).toBe(-1);
  });
  it("picks the last line that has started", () => {
    expect(activeLineIndex(lines, 1)).toBe(0);
    expect(activeLineIndex(lines, 2.9)).toBe(0);
    expect(activeLineIndex(lines, 3)).toBe(2); // same timestamp: the later one wins
    expect(activeLineIndex(lines, 100)).toBe(3);
  });
});

describe("lineEnd", () => {
  it("uses duration, then the next line, then a fallback", () => {
    expect(lineEnd(lines, 0)).toBe(3);
    expect(lineEnd(lines, 3)).toBe(9);
    expect(lineEnd([{ time: 4, text: "x" }], 0)).toBe(9);
  });
});

describe("timeAt", () => {
  const knots = [
    { pos: 0, time: 10 },
    { pos: 4, time: 12 },
    { pos: 5, time: 13 },
  ];
  it("clamps and interpolates linearly", () => {
    expect(timeAt(knots, -1)).toBe(10);
    expect(timeAt(knots, 2)).toBe(11);
    expect(timeAt(knots, 4.5)).toBe(12.5);
    expect(timeAt(knots, 9)).toBe(13);
  });
});

describe("wordKnots", () => {
  it("locates words in order and skips ones not in the text", () => {
    const k = wordKnots("one two one", [
      { time: 0, text: "one", duration: 1 },
      { time: 1, text: "zzz", duration: 1 },
      { time: 2, text: "one" },
    ], 4);
    expect(k).toEqual([
      { pos: 0, time: 0 },
      { pos: 3, time: 1 },
      { pos: 8, time: 2 },
      { pos: 11, time: 4 },
    ]);
  });
});

describe("tokenize", () => {
  it("without words: one untimed token per span that fills at start, settles at end", () => {
    expect(tokenize("hello", 2, 5)).toEqual([{ text: "hello", reading: undefined, start: 2, end: 2, done: 5 }]);
  });

  it("with words: tokens follow word edges and carry their times", () => {
    const t = tokenize("Hi there", 0, 4, [
      { time: 0, text: "Hi", duration: 1 },
      { time: 2, text: "there", duration: 1 },
    ]);
    expect(t.map((x) => x.text)).toEqual(["Hi", " ", "there"]);
    expect(t[0]).toMatchObject({ start: 0, end: 1, done: 1 });
    expect(t[1]).toMatchObject({ start: 1, end: 2 }); // the gap sweeps between words
    expect(t[2]).toMatchObject({ start: 2, end: 3 });
  });

  it("keeps ruby spans whole even when a word edge falls inside", () => {
    const spans = [{ text: "夜明", reading: "よあ" }, { text: "けの駅" }];
    const words = [
      { time: 0, text: "夜", duration: 1 },
      { time: 1, text: "明けの駅", duration: 3 },
    ];
    const t = tokenize("夜明けの駅", 0, 4, words, spans);
    expect(t[0]).toMatchObject({ text: "夜明", reading: "よあ", start: 0 });
    expect(t.map((x) => x.text).join("")).toBe("夜明けの駅");
  });

  it("ignores spans that don't spell the text", () => {
    const t = tokenize("abc", 0, 1, undefined, [{ text: "xyz", reading: "q" }]);
    expect(t).toHaveLength(1);
    expect(t[0].reading).toBeUndefined();
  });
});
