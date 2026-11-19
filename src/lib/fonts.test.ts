import { describe, expect, it } from "vitest";
import { defaultSettings } from "../state/schema";
import { stageVars } from "../overlay/style";
import { NO_ROWS } from "../overlay/rows";
import { cssFamily, fontsFor, fontVar, htmlLang, withFont } from "./fonts";
import { UNKNOWN } from "./language";

describe("lyric fonts", () => {
  it("quotes a family name, whatever is in it", () => {
    expect(cssFamily("Yu Gothic UI")).toBe('"Yu Gothic UI"');
    expect(cssFamily('Odd "Name"')).toBe('"Odd \\"Name\\""');
  });

  it("puts a language's own font ahead of the main one, which covers what it lacks", () => {
    const lyric = { ...defaultSettings.lyric, fontFamily: '"Segoe UI"', fonts: { ja: '"Yu Mincho"' } };
    const vars = stageVars({ lyric, translation: defaultSettings.translation, furigana: defaultSettings.furigana, rows: NO_ROWS }) as Record<string, unknown>;
    expect(vars["--lyric-font-ja"]).toBe('"Yu Mincho", "Segoe UI"');
    expect(vars["--lyric-font-ko"]).toBeUndefined();
    expect(fontVar("ko")).toBe("var(--lyric-font-ko, var(--lyric-font))");
  });

  it("offers a language only the fonts made for it", () => {
    const installed = [
      { name: "Yu Gothic", scripts: ["latin", "ja"] },
      { name: "Microsoft YaHei", scripts: ["latin", "zh"] },
      { name: "Segoe UI", scripts: ["latin"] },
    ];
    expect(fontsFor(installed, "ja")).toEqual([{ value: '"Yu Gothic"', label: "Yu Gothic" }]);
    expect(fontsFor(installed, "latin").map((f) => f.label)).toEqual(["Yu Gothic", "Microsoft YaHei", "Segoe UI"]);
    expect(fontsFor(installed, "th")).toEqual([]);
  });

  it("sets and clears one language's font", () => {
    const fonts = withFont({}, "ja", '"Yu Mincho"');
    expect(fonts).toEqual({ ja: '"Yu Mincho"' });
    expect(withFont(fonts, "ja", null)).toEqual({});
  });

  it("tags Chinese by script, and Latin not at all", () => {
    expect([htmlLang("zh"), htmlLang("zh-tw"), htmlLang("ja"), htmlLang(UNKNOWN)]).toEqual(["zh-Hans", "zh-Hant", "ja", undefined]);
  });
});
