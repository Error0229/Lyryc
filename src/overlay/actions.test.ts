import { beforeEach, describe, expect, it, vi } from "vitest";
import type { NotFound, TimedLyrics, Track } from "../lib/types";

// The lookups the overlay asks for, and a way to answer them by hand: lyrics, a
// `NotFound` to reject with, or null for some other failure.
const calls: Track[] = [];
const refetches: boolean[] = [];
const answers: Array<(r: TimedLyrics | NotFound | null) => void> = [];
const startedOver: Track[] = [];
vi.mock("../services/ipc/backend", () => ({
  backend: {
    fetchLyrics: (t: Track, _extra: boolean, _web: boolean, refetch = false) => {
      calls.push(t);
      refetches.push(refetch);
      return new Promise((resolve, reject) => answers.push((r) => (r && "lines" in r ? resolve(r) : reject(r ?? new Error("none")))));
    },
    startOver: async (t: Track) => void startedOver.push(t),
  },
}));

const { refetchLyrics, setTrack, startOver } = await import("./actions");
const { useSession } = await import("../state/session");

const bare: Track = { title: "細胞", artist: "Sato", duration: null, videoId: "f_VekM6tlqg" } as Track;
const full: Track = { ...bare, duration: 180 };
const flush = () => new Promise((r) => setTimeout(r, 0));

function reset() {
  for (const list of [calls, refetches, answers, startedOver]) list.length = 0;
  useSession.setState({ track: null, status: "idle", lyrics: [], source: null, refetching: false, stage: null, searched: null });
}

const lines = (...texts: string[]) => texts.map((text, i) => ({ time: i * 5, text, duration: 5 }));

describe("setTrack", () => {
  beforeEach(reset);

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

describe("refetchLyrics (Search again)", () => {
  beforeEach(reset);

  /** The song playing with lyrics from LRCLIB on screen. */
  async function showing() {
    setTrack(full);
    answers[0]({ lines: lines("wrong", "words", "here"), source: "lrclib", pending: false });
    await flush();
    expect(useSession.getState().status).toBe("ready");
  }

  it("waits for the first lookup: nothing has been seen that could be rejected", () => {
    setTrack(full);
    void refetchLyrics();
    expect(refetches).toEqual([false]);
  });

  it("shows what it found and says where from", async () => {
    await showing();
    void refetchLyrics();
    expect(refetches).toEqual([false, true]);
    expect(useSession.getState().refetching).toBe(true);
    answers[1]({ lines: lines("right", "words", "now"), source: "qq", pending: false });
    await flush();
    const s = useSession.getState();
    expect([s.refetching, s.source, s.lyrics[0].text]).toEqual([false, "qq", "right"]);
    expect(s.searched).toMatchObject({ message: "New lyrics from QQ Music", rejected: true });
  });

  it("forgets how it ended once the song changes", async () => {
    await showing();
    void refetchLyrics();
    answers[1]({ lines: lines("right", "words", "now"), source: "qq", pending: false });
    await flush();
    setTrack({ ...full, title: "Another", videoId: "another0000" });
    expect(useSession.getState().searched).toBeNull();
  });

  it("with nothing left shows none, says what was tried, and that there is something to start over", async () => {
    await showing();
    void refetchLyrics();
    answers[1]({ tried: [{ method: "lrclib", end: "rejected" }, { method: "web-search", end: "unavailable", retrySecs: 600 }], rejected: true });
    await flush();
    const s = useSession.getState();
    expect([s.refetching, s.status, s.lyrics.length]).toEqual([false, "missing", 0]);
    expect(s.searched).toMatchObject({ message: "Nothing else found, but web search couldn't be asked (again in 10 min)", rejected: true });

    // Start over: the rejections are forgotten, and the song is looked up as new.
    await startOver();
    await flush();
    expect(startedOver).toEqual([full]);
    expect(refetches).toEqual([false, true, false]);
    expect(useSession.getState()).toMatchObject({ status: "loading", searched: null });
  });

  it("has nothing to start over when nothing was rejected", async () => {
    setTrack(full);
    answers[0](null);
    await flush();
    void refetchLyrics();
    answers[1]({ tried: [{ method: "lrclib", end: "nothing" }], rejected: false });
    await flush();
    expect(useSession.getState().searched).toMatchObject({ message: "Tried everything. Nothing found.", rejected: false });
  });

  it("says nothing about a song that is no longer playing", async () => {
    await showing();
    void refetchLyrics();
    setTrack({ ...full, title: "Another", videoId: "another0000" });
    answers[1]({ tried: [], rejected: false });
    await flush();
    expect(useSession.getState()).toMatchObject({ refetching: false, searched: null });
  });
});
