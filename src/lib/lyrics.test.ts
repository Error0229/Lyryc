import { describe, expect, it } from "vitest";
import { lineKeys, normalizeLines, textSignature } from "./lyrics";
import { hasKanji, isJapaneseSong } from "./japanese";
import { splitAlpha, toHex, withAlpha } from "./color";

describe("normalizeLines", () => {
  it("drops malformed lines, trims text and sorts by time", () => {
    const raw = [{ time: 5, text: " b " }, { time: "x", text: "bad" }, null, { time: 1, text: "a" }, { time: Infinity, text: "inf" }];
    expect(normalizeLines(raw)).toEqual([
      { time: 1, text: "a" },
      { time: 5, text: "b" },
    ]);
    expect(normalizeLines("nope")).toEqual([]);
  });
});

describe("lineKeys", () => {
  it("numbers repeated lines so a chorus repeat is a new line", () => {
    const keys = lineKeys([{ time: 0, text: "La la" }, { time: 1, text: "Hey" }, { time: 2, text: "la  LA" }]);
    expect(keys).toEqual(["la la#0", "hey#0", "la la#1"]);
  });

  it("is unchanged by retiming, so a timing swap keeps line identity", () => {
    const a = [{ time: 1, text: "x" }, { time: 2, text: "y" }];
    const b = [{ time: 1.3, text: "x" }, { time: 2.4, text: "y" }];
    expect(lineKeys(b)).toEqual(lineKeys(a));
    expect(textSignature(b)).toBe(textSignature(a));
  });
});

describe("japanese", () => {
  it("needs kana somewhere in the song", () => {
    expect(isJapaneseSong([{ text: "我爱你" }])).toBe(false);
    expect(isJapaneseSong([{ text: "東京" }, { text: "君を待つ" }])).toBe(true);
    expect(hasKanji("ひらがな")).toBe(false);
    expect(hasKanji("駅")).toBe(true);
  });
});

describe("color", () => {
  it("normalises css colours to hex", () => {
    expect(toHex("#ABC")).toBe("#aabbcc");
    expect(toHex("rgb(200, 220, 255)")).toBe("#c8dcff");
    expect(toHex("rgba(0,0,0,0.5)")).toBe("#00000080");
    expect(toHex("#ffffff60")).toBe("#ffffff60");
    expect(toHex("blue")).toBeNull();
  });

  it("splits and joins alpha", () => {
    expect(splitAlpha("#ffffff80")).toEqual({ rgb: "#ffffff", alpha: 128 / 255 });
    expect(withAlpha("#123456", 1)).toBe("#123456");
    expect(withAlpha("#123456ff", 0.5)).toBe("#12345680");
  });
});
