import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Track } from "../lib/types";

// The lookups the overlay asks for, and a way to answer them by hand.
const calls: Track[] = [];
const answers: Array<(lines: unknown) => void> = [];
vi.mock("../services/ipc/backend", () => ({
  backend: {
    fetchLyrics: (t: Track) => {
      calls.push(t);
      return new Promise((resolve, reject) => answers.push((r) => (r ? resolve(r) : reject(new Error("none")))));
    },
  },
}));

const { setTrack } = await import("./actions");
const { useSession } = await import("../state/session");

const bare: Track = { title: "細胞", artist: "Sato", duration: null, videoId: "f_VekM6tlqg" } as Track;
const full: Track = { ...bare, duration: 180 };
const flush = () => new Promise((r) => setTimeout(r, 0));

describe("setTrack", () => {
  beforeEach(() => {
    calls.length = 0;
    answers.length = 0;
    useSession.setState({ track: null, status: "idle" });
  });

  it("searches again when the length arrives while the first search runs", async () => {
    setTrack(bare);
    setTrack(full);
    expect(calls.map((t) => t.duration ?? null)).toEqual([null, 180]);
    answers[0](null); // the length-less search finds nothing...
    await flush();
    expect(useSession.getState().status).toBe("loading"); // ...but its answer is stale
  });

  it("searches again when the length arrives after the first search found nothing", async () => {
    setTrack(bare);
    answers[0](null);
    await flush();
    expect(useSession.getState().status).toBe("missing");
    setTrack(full);
    expect(calls.length).toBe(2);
  });

  it("does not search again for a message that adds nothing", () => {
    setTrack(full);
    setTrack(bare);
    setTrack(full);
    expect(calls.length).toBe(1);
  });
});
