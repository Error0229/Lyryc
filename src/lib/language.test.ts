import { describe, expect, it } from "vitest";
import { detectLanguage, languageFamily, shouldTranslate, UNKNOWN } from "./language";
import type { LanguageFilter } from "../state/schema";

// Invented placeholder lines: only the script mix matters, not the words.
const ja = ["きょうはあめがふる", "まちのあかり", "Yeah yeah oh", "空白地帯", "ことばにならない", "Hold on tight"];
const ko = ["오늘도 너를 기다려", "Baby baby", "하늘을 봐", "Oh oh oh", "우리 같이 가자"];
const zhSimplified = ["我们一起走过", "这条长长的路", "Oh baby", "你说没关系"];
const zhTraditional = ["我們一起走過", "這條長長的路", "你說沒關係"];
const en = ["I walk the line", "and you know it is late", "we run to the sea"];
const es = ["yo camino por la calle", "y no sé que hacer", "con mi corazón"];

describe("detectLanguage", () => {
  it("reads a Japanese song with kanji-only lines and English hooks as Japanese", () => {
    expect(detectLanguage(ja)).toBe("ja");
  });

  it("reads a Korean song with English hooks as Korean", () => {
    expect(detectLanguage(ko)).toBe("ko");
  });

  it("tells Simplified from Traditional Chinese", () => {
    expect(detectLanguage(zhSimplified)).toBe("zh");
    expect(detectLanguage(zhTraditional)).toBe("zh-tw");
  });

  it("tells Latin-script languages apart by common words", () => {
    expect(detectLanguage(en)).toBe("en");
    expect(detectLanguage(es)).toBe("es");
  });

  it("is unknown without letters or recognisable words", () => {
    expect(detectLanguage([])).toBe(UNKNOWN);
    expect(detectLanguage(["...", "♪"])).toBe(UNKNOWN);
    expect(detectLanguage(["zzz qqq"])).toBe(UNKNOWN);
  });
});

describe("languageFamily", () => {
  it("folds regional Chinese codes into Simplified and Traditional", () => {
    expect(languageFamily("zh-cn")).toBe("zh");
    expect(languageFamily("zh-hk")).toBe("zh-tw");
    expect(languageFamily("JA")).toBe("ja");
  });
});

describe("shouldTranslate", () => {
  const all: LanguageFilter = { mode: "all", languages: [] };
  const only: LanguageFilter = { mode: "only", languages: ["ja", "ko"] };
  const except: LanguageFilter = { mode: "except", languages: ["en", "zh"] };

  it("never translates a song already in the target language", () => {
    expect(shouldTranslate("zh-tw", "zh-hk", all)).toBe(false);
    expect(shouldTranslate("en", "en", all)).toBe(false);
  });

  it("still converts Simplified to Traditional Chinese", () => {
    expect(shouldTranslate("zh", "zh-tw", all)).toBe(true);
  });

  it("applies the allow list", () => {
    expect(shouldTranslate("ja", "zh-tw", only)).toBe(true);
    expect(shouldTranslate("en", "zh-tw", only)).toBe(false);
    expect(shouldTranslate(UNKNOWN, "zh-tw", only)).toBe(false);
  });

  it("applies the block list, by family", () => {
    expect(shouldTranslate("en", "zh-tw", except)).toBe(false);
    expect(shouldTranslate("zh", "ja", { mode: "except", languages: ["zh-cn"] })).toBe(false);
    expect(shouldTranslate("ja", "zh-tw", except)).toBe(true);
    expect(shouldTranslate(UNKNOWN, "zh-tw", except)).toBe(true);
  });
});
