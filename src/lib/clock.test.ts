import { describe, expect, it } from "vitest";
import { clockTime, CORRECTION_SECONDS, isResend, reconcile, sampleAge, stopped } from "./clock";

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

  it("absorbs running ahead by half a second without stepping back (late samples)", () => {
    // Seen live: the clock ran ~0.5 s ahead of late samples and snapped back
    // across a line boundary, so the line flipped back and forth.
    let a = reconcile(reconcile(stopped, 94, true, 0), 94, true, 0);
    const before = clockTime(a, 900);
    a = reconcile(a, before - 0.47, true, 900);
    let prev = clockTime(a, 900);
    expect(prev).toBeCloseTo(before);
    for (let ms = 916; ms <= 4000; ms += 16) {
      const t = clockTime(a, ms);
      expect(t).toBeGreaterThan(prev);
      prev = t;
    }
    expect(clockTime(a, 4000)).toBeCloseTo(94 + 4 - 0.47); // back on the backend's timeline
  });

  it("still snaps back for a seek", () => {
    const a = reconcile(reconcile(stopped, 100, true, 0), 100, true, 0);
    const b = reconcile(a, 60, true, 1000);
    expect(clockTime(b, 1000)).toBe(60);
  });

  it("ages a sample by when it was measured, ignoring stamps that say nothing", () => {
    expect(sampleAge(10_000, 9_750, 10_300)).toBeCloseTo(0.3);
    // Seen live: reports arrived in bursts, several seconds late.
    expect(sampleAge(10_000, 9_750, 17_500)).toBeCloseTo(7.5);
    expect(sampleAge(10_000, 9_750, 9_000)).toBe(0); // clock skew
    expect(sampleAge(10_000, 9_750, 60_000)).toBeCloseTo(50); // seen live: 20-60 s backlogs
    expect(sampleAge(10_000, 9_750, 200_000)).toBe(0); // beyond belief
    expect(sampleAge(10_000, 10_000, 17_500)).toBe(0); // an old extension repeats the detection stamp
    expect(sampleAge(undefined, 9_750, 10_000)).toBe(0);
  });

  it("drops a re-sent report once stamps are known to advance, never for an old extension", () => {
    expect(isResend(10_000, 10_000, true)).toBe(true); // the same stamp again: a re-send (seen live)
    expect(isResend(9_750, 10_000, true)).toBe(true); // out of order
    expect(isResend(10_250, 10_000, true)).toBe(false); // a new report
    expect(isResend(10_000, 10_000, false)).toBe(false); // an old extension repeats its stamp: still counts
    expect(isResend(undefined, 10_000, true)).toBe(false);
  });

  it("resumes from the reported time after a pause", () => {
    const a = reconcile(reconcile(stopped, 5, true, 0), 5, false, 2000);
    expect(clockTime(a, 9000)).toBe(5);
    const b = reconcile(a, 5, true, 9000);
    expect(clockTime(b, 10000)).toBeCloseTo(6);
  });
});
