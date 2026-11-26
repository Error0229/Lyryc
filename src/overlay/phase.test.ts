import { describe, expect, it } from "vitest";
import { phaseOf, statusIcon, type IconInput, type PhaseInput } from "./phase";

const at = (p: Partial<PhaseInput>) => phaseOf({ hasTrack: true, status: "loading", stage: null, source: null, refetching: false, ...p });
const icon = (i: Partial<IconInput>) =>
  statusIcon({ phase: "word", stage: null, source: null, syncing: false, syncFailed: false, justSynced: false, quiet: false, idleHint: false, searched: null, justSearched: false, ...i });

describe("phaseOf", () => {
  it("is idle without a track", () => {
    expect(at({ hasTrack: false, status: "ready" })).toBe("idle");
  });

  it("follows the lookup's stages", () => {
    expect(at({})).toBe("lookup");
    expect(at({ stage: "identify" })).toBe("identify");
    expect(at({ stage: "web-search" })).toBe("identify");
    expect(at({ stage: "sources" })).toBe("sources");
    expect(at({ stage: "other-recording" })).toBe("sources");
  });

  it("names the timing the lyrics came with", () => {
    expect(at({ status: "ready", source: "lrclib-estimate" })).toBe("estimate");
    expect(at({ status: "ready", source: "cover-estimate" })).toBe("estimate");
    expect(at({ status: "ready", source: "lrclib" })).toBe("line");
    expect(at({ status: "ready", source: "original-timing" })).toBe("line");
    expect(at({ status: "ready", source: "aligned" })).toBe("word");
  });

  it("puts Search again before everything but idle, and follows its passes", () => {
    expect(at({ status: "ready", source: "aligned", refetching: true })).toBe("refetch");
    expect(at({ status: "missing", refetching: true })).toBe("refetch");
    expect(at({ status: "ready", source: "lrclib", refetching: true, stage: "web-search" })).toBe("identify");
    expect(at({ status: "missing", refetching: true, stage: "other-recording" })).toBe("sources");
    expect(at({ status: "missing" })).toBe("missing");
    expect(at({ status: "instrumental" })).toBe("instrumental");
  });
});

describe("statusIcon", () => {
  it("is silent when everything is fine", () => {
    expect(icon({ phase: "word" })).toBeNull();
    expect(icon({ phase: "line" })).toBeNull();
    expect(icon({ phase: "idle" })).toBeNull();
  });

  it("breathes while the lookup works, and says which pass", () => {
    expect(icon({ phase: "lookup" })).toMatchObject({ id: "search", motion: "breathe" });
    expect(icon({ phase: "identify", stage: "web-search" })?.tip).toBe("Searching the web");
    expect(icon({ phase: "sources", stage: "other-recording" })?.tip).toBe("Finding the original's words");
  });

  it("says not synced, syncing when better timing is on its way, or that syncing failed", () => {
    expect(icon({ phase: "estimate" })).toMatchObject({ id: "estimate", tip: "Not synced" });
    expect(icon({ phase: "estimate", source: "cover-estimate" })?.tip).toBe("Original's words, not synced");
    expect(icon({ phase: "estimate", syncing: true })).toMatchObject({ id: "syncing", motion: "bars" });
    expect(icon({ phase: "estimate", syncFailed: true })?.tip).toBe("Couldn't sync to the vocals");
    expect(icon({ phase: "line", syncing: true })?.id).toBe("syncing");
  });

  it("shows the upgrade once, then goes quiet", () => {
    expect(icon({ phase: "word", justSynced: true })).toMatchObject({ id: "synced", tip: "Synced to the vocals", motion: "done" });
    expect(icon({ phase: "line", justSynced: true })?.tip).toBe("Synced lyrics found");
    // An "upgrade" that is still an estimate isn't one.
    expect(icon({ phase: "estimate", justSynced: true })?.id).toBe("estimate");
  });

  it("spins while Search again starts", () => {
    expect(icon({ phase: "refetch" })).toMatchObject({ id: "refetch", motion: "spin" });
  });

  it("keeps not-found and instrumental in words first, then only an icon", () => {
    expect(icon({ phase: "missing" })).toBeNull();
    expect(icon({ phase: "missing", quiet: true })).toMatchObject({ id: "no-lyrics", action: "paste-lyrics" });
    expect(icon({ phase: "instrumental", quiet: true })?.id).toBe("instrumental");
  });

  it("names how Search again ended: at once when nothing is left, for a moment over new lyrics", () => {
    const nothing = "Nothing else found, but web search couldn't be asked (again in 1 min)";
    expect(icon({ phase: "missing", searched: nothing })).toMatchObject({ id: "no-lyrics", tip: nothing, action: "paste-lyrics" });
    expect(icon({ phase: "word", searched: "New lyrics from QQ Music", justSearched: true })).toMatchObject({ id: "synced", tip: "New lyrics from QQ Music" });
    expect(icon({ phase: "estimate", searched: "New lyrics from LRCLIB", justSearched: true })?.id).toBe("synced");
    expect(icon({ phase: "word", searched: "New lyrics from QQ Music" })).toBeNull();
    expect(icon({ phase: "instrumental", searched: "No lyrics found: it looks instrumental" })?.tip).toBe("No lyrics found: it looks instrumental");
  });

  it("says what an idle strip waits for only when asked", () => {
    expect(icon({ phase: "idle", idleHint: true })?.tip).toBe("Waiting for YouTube Music");
  });

  it("keeps every name short", () => {
    const all: Partial<IconInput>[] = [
      { phase: "lookup" }, { phase: "identify" }, { phase: "sources" }, { phase: "estimate", source: "cover-estimate" },
      { phase: "estimate", syncing: true }, { phase: "word", justSynced: true }, { phase: "refetch" }, { phase: "missing", quiet: true },
    ];
    for (const i of all) expect(icon(i)!.tip.split(" ").length).toBeLessThanOrEqual(4);
  });
});
