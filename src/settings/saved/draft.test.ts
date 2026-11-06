import { describe, expect, it } from "vitest";
import type { SavedSongDetail } from "../../services/ipc/backend";
import { blankRow, draftOf, retimed, rowsOf } from "./draft";

const saved: SavedSongDetail = {
  song: { artist: "a", track: "t", kind: "lyrics", source: "lrclib", lines: 2, cachedAt: 0, expiresAt: 1, translations: 1, timings: 1 },
  lines: [
    { time: 1.234, text: "夜明けの街を", duration: 2.5 },
    { time: 4, text: "君と歩いた", duration: null },
  ],
  translations: [{ key: "k", lines: ["The town at dawn", "I walked with you"] }],
  timings: [{ videoId: "v", hash: "h", rejected: false }],
};

const load = () => rowsOf(saved.lines, saved.translations.map((t) => t.lines));

describe("saved lyrics draft", () => {
  it("has nothing to save until something changes, even for times finer than it shows", () => {
    const d = draftOf(saved, load());
    expect(d).toMatchObject({ wordsChanged: false, linesChanged: false, change: null });
  });

  it("saves an edited translation line alone", () => {
    const rows = load();
    rows[0].tr[0] = "Dawn's town";
    const d = draftOf(saved, rows);
    expect(d.linesChanged).toBe(false);
    expect(d.change).toEqual({ op: "save", lines: saved.lines, translations: [{ key: "k", lines: ["Dawn's town", "I walked with you"] }] });
  });

  it("a new time keeps the words and translation, and forgets the line's length", () => {
    const rows = load();
    rows[0] = retimed(rows[0], "0:01.50");
    const d = draftOf(saved, rows);
    expect(d).toMatchObject({ wordsChanged: false, linesChanged: true });
    expect(d.change).toMatchObject({ lines: [{ time: 1.5, duration: null }, { time: 4 }], translations: [{ key: "k" }] });
  });

  it("new words send no translations; moving a line past another changes the words' order", () => {
    const rows = load();
    rows[1] = retimed(rows[1], "0:00.50");
    const d = draftOf(saved, rows);
    expect(d.wordsChanged).toBe(true);
    expect(d.change).toMatchObject({ lines: [{ text: "君と歩いた" }, { text: "夜明けの街を" }], translations: [] });
  });

  it("an added line is a change; a time that doesn't read blocks saving", () => {
    expect(draftOf(saved, [...load(), blankRow(8, 1)]).wordsChanged).toBe(true);
    const rows = load();
    rows[1] = retimed(rows[1], "4:xx");
    const d = draftOf(saved, rows);
    expect([...d.bad]).toEqual([rows[1].id]);
    expect(d.change).toBeNull();
  });

  it("no lines is nothing to save: deleting the lyrics is its own action", () => {
    expect(draftOf(saved, []).change).toBeNull();
  });
});
