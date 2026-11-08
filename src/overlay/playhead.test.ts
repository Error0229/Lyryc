import { afterEach, describe, expect, it, vi } from "vitest";
import { playhead } from "./playhead";

describe("playhead snap diagnostic", () => {
  afterEach(() => {
    playhead.reset();
    vi.restoreAllMocks();
  });

  function playTo(t: number) {
    playhead.sample(t, true);
    playhead.sample(t, true);
  }

  it("warns when the clock snaps back mid-song", () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    playTo(120);
    playhead.sample(60, true);
    expect(warn).toHaveBeenCalledOnce();
  });

  it("stays quiet when the next track loads (paused at 0)", () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    playTo(223.68);
    playhead.sample(0, false);
    expect(warn).not.toHaveBeenCalled();
  });
});
