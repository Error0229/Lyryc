import { describe, expect, it } from "vitest";
import { formatOffset, normalizeOffset, OFFSET_LIMIT, parseOffset, totalOffset, trackKey, withTrackOffset } from "./offset";

describe("offset", () => {
  it("keeps the v2 track key format", () => {
    expect(trackKey("  Daft Punk ", "One More Time")).toBe("daft punk - one more time");
  });

  it("normalises: rounds to 10 ms, clamps, rejects NaN", () => {
    expect(normalizeOffset(0.1 + 0.2)).toBe(0.3);
    expect(normalizeOffset(99)).toBe(10);
    expect(normalizeOffset(Number.NaN)).toBe(0);
  });

  it("adds the track offset to the global one", () => {
    expect(totalOffset(0.5, { k: -0.2 }, "k")).toBeCloseTo(0.3);
    expect(totalOffset(0.5, { k: -0.2 }, null)).toBe(0.5);
    expect(totalOffset(0.5, {}, "other")).toBe(0.5);
  });

  it("removes zeroed entries instead of storing them", () => {
    expect(withTrackOffset({ a: 1, b: 2 }, "a", 0)).toEqual({ b: 2 });
    expect(withTrackOffset({}, "a", 0.25)).toEqual({ a: 0.25 });
  });

  it("formats with an explicit sign", () => {
    expect(formatOffset(0.3)).toBe("+0.30 s");
    expect(formatOffset(-1)).toBe("−1.00 s");
    expect(formatOffset(0)).toBe("±0.00 s");
  });
});

describe("parseOffset", () => {
  it("reads what people type, signs and units included", () => {
    expect(parseOffset("0.35")).toBe(0.35);
    expect(parseOffset("-1.2")).toBe(-1.2);
    expect(parseOffset("+0.5 s")).toBe(0.5);
    expect(parseOffset("−1,25")).toBe(-1.25);
    expect(parseOffset(".5")).toBe(0.5);
    expect(parseOffset("±0.00 s")).toBe(0);
  });

  it("clamps to the offset range and rounds to 10 ms", () => {
    expect(parseOffset("99")).toBe(OFFSET_LIMIT);
    expect(parseOffset("0.123")).toBe(0.12);
  });

  it("refuses anything that isn't a number, so the field keeps its value", () => {
    expect(parseOffset("")).toBeNull();
    expect(parseOffset("abc")).toBeNull();
    expect(parseOffset("1.2.3")).toBeNull();
    expect(parseOffset("-")).toBeNull();
  });
});
