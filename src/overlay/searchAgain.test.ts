import { describe, expect, it } from "vitest";
import type { NotFound, Tried } from "../lib/types";
import { foundMessage, notFoundMessage, notFoundOf } from "./searchAgain";

describe("foundMessage", () => {
  it("names where the new lyrics came from", () => {
    expect(foundMessage("qq")).toBe("New lyrics from QQ Music");
    expect(foundMessage("ytmusic-estimate")).toBe("New lyrics from YouTube Music");
    expect(foundMessage("cover-estimate")).toBe("New lyrics from the original song");
    expect(foundMessage("aligned")).toBe("New lyrics found");
    expect(foundMessage("instrumental")).toBe("No lyrics found: it looks instrumental");
  });
});

describe("notFoundMessage", () => {
  const answered: Tried[] = [
    { method: "ytmusic", end: "rejected" },
    { method: "identify", end: "nothing" },
    { method: "sources", end: "nothing" },
    { method: "lrclib", end: "rejected" },
  ];
  const after = (tried: Tried[]): NotFound => ({ tried, rejected: true });
  const first = (tried: Tried[]): NotFound => ({ tried, rejected: false });

  it("says everything was tried only when everything answered", () => {
    expect(notFoundMessage(after(answered))).toBe("Tried everything. Nothing else found.");
    expect(notFoundMessage(first([{ method: "lrclib", end: "nothing" }]))).toBe("Tried everything. Nothing found.");
  });

  it("names what could not be asked, and until when", () => {
    const refused: Tried[] = [...answered, { method: "web-search", end: "unavailable", retrySecs: 3480 }];
    expect(notFoundMessage(after(refused))).toBe("Nothing else found, but web search couldn't be asked (again in 58 min)");
    const down: Tried[] = [
      { method: "lrclib", end: "unavailable" },
      { method: "sources", end: "unavailable" },
      { method: "identify", end: "unavailable", retrySecs: 30 },
    ];
    expect(notFoundMessage(first(down))).toBe("Nothing found, but LRCLIB, the other sources and the AI couldn't be asked (again in 1 min)");
    expect(notFoundMessage(first([{ method: "lrclib", end: "unavailable" }]))).toBe("Nothing found, but LRCLIB couldn't be asked");
    // Rejected before, though no method reports it this time.
    expect(notFoundMessage(after([{ method: "lrclib", end: "unavailable" }]))).toBe("Nothing else found, but LRCLIB couldn't be asked");
  });

  it("says so when a method is switched off", () => {
    const off: Tried[] = [...answered, { method: "sources", end: "off" }];
    expect(notFoundMessage(after(off))).toBe("Tried everything that's turned on. Nothing else found.");
  });
});

describe("what a failed lookup carries", () => {
  it("reads the backend's answer, and nothing from any other error", () => {
    const nf: NotFound = { tried: [{ method: "lrclib", end: "unavailable" }], rejected: true };
    expect(notFoundOf(nf)).toEqual(nf);
    const none: NotFound = { tried: [], rejected: false };
    expect(notFoundOf(new Error("none"))).toEqual(none);
    expect(notFoundOf("no lyrics")).toEqual(none);
    expect(notFoundOf(null)).toEqual(none);
  });
});
