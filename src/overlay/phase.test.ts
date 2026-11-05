import { describe, expect, it } from "vitest";
import { phaseOf, statusIcon, type IconInput, type PhaseInput } from "./phase";

const at = (p: Partial<PhaseInput>) => phaseOf({ hasTrack: true, status: "loading", stage: null, source: null, refetching: false, ...p });
const icon = (i: Partial<IconInput>) =>
  statusIcon({ phase: "word", stage: null, source: null, syncing: false, justSynced: false, outcome: null, quiet: false, idleHint: false, ...i });

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

  it("puts Search again before everything but idle", () => {
    expect(at({ status: "ready", source: "aligned", refetching: true })).toBe("refetch");
    expect(at({ status: "missing", refetching: true })).toBe("refetch");
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

  it("says estimated, or syncing when better timing is on its way", () => {
    expect(icon({ phase: "estimate" })?.id).toBe("estimate");
    expect(icon({ phase: "estimate", source: "cover-estimate" })?.tip).toBe("Original's words, estimated");
    expect(icon({ phase: "estimate", syncing: true })).toMatchObject({ id: "syncing", motion: "bars" });
    expect(icon({ phase: "line", syncing: true })?.id).toBe("syncing");
  });

  it("shows the upgrade once, then goes quiet", () => {
    expect(icon({ phase: "word", justSynced: true })).toMatchObject({ id: "synced", tip: "Synced to the vocals", motion: "done" });
    expect(icon({ phase: "line", justSynced: true })?.tip).toBe("Synced lyrics found");
    // An "upgrade" that is still an estimate isn't one.
    expect(icon({ phase: "estimate", justSynced: true })?.id).toBe("estimate");
  });

  it("reports Search again's outcome over the state", () => {
    expect(icon({ phase: "refetch" })).toMatchObject({ id: "refetch", motion: "spin" });
    expect(icon({ phase: "word", outcome: "same" })?.tip).toBe("Same lyrics found");
    expect(icon({ phase: "missing", outcome: "nothing-else" })?.tip).toBe("Nothing else found");
  });

  it("keeps not-found and instrumental in words first, then only an icon", () => {
    expect(icon({ phase: "missing" })).toBeNull();
    expect(icon({ phase: "missing", quiet: true })?.id).toBe("no-lyrics");
    expect(icon({ phase: "instrumental", quiet: true })?.id).toBe("instrumental");
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
