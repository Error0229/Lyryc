import { describe, expect, it } from "vitest";
import { limiter } from "./logForward";

describe("limiter", () => {
  it("drops a repeat of the last message for a while", () => {
    const allow = limiter(30, 10_000, 2_000);
    expect(allow("boom", 0)).toBe(true);
    expect(allow("boom", 500)).toBe(false);
    expect(allow("other", 600)).toBe(true);
    expect(allow("boom", 2_700)).toBe(true);
  });

  it("caps how many messages a window lets through", () => {
    const allow = limiter(3, 10_000, 0);
    const got = [0, 1, 2, 3, 4].map((i) => allow(`m${i}`, i));
    expect(got).toEqual([true, true, true, false, false]);
    expect(allow("later", 10_001)).toBe(true);
  });
});
