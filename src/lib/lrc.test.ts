import { describe, expect, it } from "vitest";
import { formatLrc, formatTime, parseLrc, parseTime } from "./lrc";

describe("lrc", () => {
  it("reads timed lines, repeats, and skips the rest", () => {
    const lines = parseLrc("[ar:Someone]\n[00:01.50]夜明けの街を\r\n[00:10.00][01:20.25] chorus \nplain words\n[02:03]end");
    expect(lines.map((l) => [l.time, l.text])).toEqual([
      [1.5, "夜明けの街を"],
      [10, "chorus"],
      [80.25, "chorus"],
      [123, "end"],
    ]);
  });

  it("formats and parses times both ways", () => {
    expect(formatTime(62.5)).toBe("1:02.50");
    expect(formatTime(0)).toBe("0:00.00");
    expect(parseTime("1:02.50")).toBe(62.5);
    expect(parseTime("62.5")).toBe(62.5);
    expect(parseTime("1:xx")).toBeNull();
    expect(parseTime(formatTime(3599.99))).toBeCloseTo(3599.99);
  });

  it("writes LRC that reads back", () => {
    const lines = [
      { time: 1.5, text: "夜明けの街を", duration: null },
      { time: 83.25, text: "", duration: null },
    ];
    expect(formatLrc(lines)).toBe("[00:01.50]夜明けの街を\n[01:23.25]");
    expect(parseLrc(formatLrc(lines))).toEqual(lines);
  });
});
