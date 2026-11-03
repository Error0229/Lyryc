import { describe, expect, it } from "vitest";
import { clockTime, CORRECTION_SECONDS, reconcile, stopped } from "./clock";

describe("clock", () => {
  it("is frozen when paused and advances in real time when playing", () => {
    const paused = reconcile(stopped, 10, false, 0);
    expect(clockTime(paused, 5000)).toBe(10);
    const playing = reconcile(paused, 10, true, 0);
    expect(clockTime(playing, 1500)).toBeCloseTo(11.5);
  });

  it("snaps on a large jump (seek)", () => {
    const a = reconcile(reconcile(stopped, 10, true, 0), 10, true, 0);
    const b = reconcile(a, 42, true, 1000);
    expect(clockTime(b, 1000)).toBe(42);
  });

  it("absorbs small drift gradually and never steps backwards", () => {
    let a = reconcile(stopped, 0, true, 0);
    a = reconcile(a, 0, true, 0);
    // At 1 s we predict 1.0; the backend says 0.8 (we ran ahead).
    const before = clockTime(a, 1000);
    a = reconcile(a, 0.8, true, 1000);
    expect(clockTime(a, 1000)).toBeCloseTo(before); // no jump at the sample
    let prev = clockTime(a, 1000);
    for (let ms = 1016; ms <= 3000; ms += 16) {
      const t = clockTime(a, ms);
      expect(t).toBeGreaterThan(prev);
      prev = t;
    }
    // After the correction window the clock matches the backend's timeline.
    expect(clockTime(a, 1000 + CORRECTION_SECONDS * 1000)).toBeCloseTo(0.8 + CORRECTION_SECONDS);
  });

  it("resumes from the reported time after a pause", () => {
    const a = reconcile(reconcile(stopped, 5, true, 0), 5, false, 2000);
    expect(clockTime(a, 9000)).toBe(5);
    const b = reconcile(a, 5, true, 9000);
    expect(clockTime(b, 10000)).toBeCloseTo(6);
  });
});
