import { describe, expect, it } from "vitest";
import type { Tried } from "../lib/types";
import { canStartOver, foundMessage, notFoundMessage, triedOf } from "./searchAgain";

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

  it("says everything was tried only when everything answered", () => {
    expect(notFoundMessage(answered)).toBe("Tried everything. Nothing else found.");
    expect(notFoundMessage([{ method: "lrclib", end: "nothing" }])).toBe("Tried everything. Nothing found.");
  });

  it("names what could not be asked, and until when", () => {
    const refused: Tried[] = [...answered, { method: "web-search", end: "unavailable", retrySecs: 3480 }];
    expect(notFoundMessage(refused)).toBe("Nothing else found, but web search couldn't be asked (again in 58 min)");
    const down: Tried[] = [
      { method: "lrclib", end: "unavailable" },
      { method: "sources", end: "unavailable" },
      { method: "identify", end: "unavailable", retrySecs: 30 },
    ];
    expect(notFoundMessage(down)).toBe("Nothing found, but LRCLIB, the other sources and the AI couldn't be asked (again in 1 min)");
    expect(notFoundMessage([{ method: "lrclib", end: "unavailable" }])).toBe("Nothing found, but LRCLIB couldn't be asked");
  });

  it("says so when a method is switched off", () => {
    const off: Tried[] = [...answered, { method: "sources", end: "off" }];
    expect(notFoundMessage(off)).toBe("Tried everything that's turned on. Nothing else found.");
  });
});

describe("what a failed lookup carries", () => {
  it("offers to start over only when something was rejected", () => {
    expect(canStartOver([{ method: "lrclib", end: "rejected" }])).toBe(true);
    expect(canStartOver([{ method: "lrclib", end: "nothing" }])).toBe(false);
  });

  it("reads the backend's list, and nothing from any other error", () => {
    expect(triedOf({ tried: [{ method: "lrclib", end: "nothing" }] })).toHaveLength(1);
    expect(triedOf(new Error("none"))).toEqual([]);
    expect(triedOf("no lyrics")).toEqual([]);
    expect(triedOf(null)).toEqual([]);
  });
});
