import { describe, expect, it } from "vitest";
import { accepts } from "./upgrade";

const song = { title: "Paper Lanterns", artist: "The Quiet Hours" };
const other = { title: "Station Lights", artist: "The Quiet Hours" };

describe("accepts", () => {
  it("takes real timing over an estimate for the same track", () => {
    expect(accepts({ track: song, source: "lrclib-estimate" }, { track: song, source: "qq" })).toBe(true);
  });

  it("matches the track the way offsets do (case, outer spaces)", () => {
    const shouted = { title: " PAPER lanterns", artist: "the quiet hours " };
    expect(accepts({ track: song, source: "lrclib-estimate" }, { track: shouted, source: "netease" })).toBe(true);
  });

  it("ignores a late upgrade for the previous song", () => {
    expect(accepts({ track: other, source: "lrclib-estimate" }, { track: song, source: "kugou" })).toBe(false);
    expect(accepts({ track: null, source: null }, { track: song, source: "qq" })).toBe(false);
  });

  it("takes the first answer, estimate or not", () => {
    expect(accepts({ track: song, source: null }, { track: song, source: "lrclib-estimate" })).toBe(true);
    expect(accepts({ track: song, source: null }, { track: song, source: "lrclib" })).toBe(true);
  });

  it("never lets an estimate undo real timing that arrived first", () => {
    expect(accepts({ track: song, source: "qq" }, { track: song, source: "lrclib-estimate" })).toBe(false);
    expect(accepts({ track: song, source: "lrclib-estimate" }, { track: song, source: "lrclib-estimate" })).toBe(true);
  });

  it("keeps word timing once it has it", () => {
    expect(accepts({ track: song, source: "lrclib" }, { track: song, source: "aligned" })).toBe(true);
    expect(accepts({ track: song, source: "lrclib-estimate" }, { track: song, source: "aligned" })).toBe(true);
    expect(accepts({ track: song, source: "aligned" }, { track: song, source: "qq" })).toBe(false);
    expect(accepts({ track: song, source: "aligned" }, { track: song, source: "lrclib-estimate" })).toBe(false);
    expect(accepts({ track: song, source: "aligned" }, { track: song, source: "aligned" })).toBe(true);
  });

  it("lets no background answer undo an instrumental mark", () => {
    expect(accepts({ track: song, source: "instrumental" }, { track: song, source: "aligned" })).toBe(false);
    expect(accepts({ track: song, source: "instrumental" }, { track: song, source: "qq" })).toBe(false);
  });

  it("treats YouTube Music's plain lyrics as an estimate too", () => {
    expect(accepts({ track: song, source: "ytmusic-estimate" }, { track: song, source: "qq" })).toBe(true);
    expect(accepts({ track: song, source: "ytmusic" }, { track: song, source: "ytmusic-estimate" })).toBe(false);
  });

  it("still lets real timing be re-timed", () => {
    expect(accepts({ track: song, source: "lrclib" }, { track: song, source: "lrclib" })).toBe(true);
  });
});
