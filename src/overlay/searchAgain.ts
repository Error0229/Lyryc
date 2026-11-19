/**
 * What "Search again" says when it ends, in words: where the new lyrics came
 * from, or that there are no others, and then whether everything was tried.
 */
import type { LookupMethod, LyricsSource, Tried } from "../lib/types";

/** Who the lyrics are from, where one name says it. */
const FROM: Partial<Record<LyricsSource, string>> = {
  lrclib: "LRCLIB",
  "lrclib-estimate": "LRCLIB",
  qq: "QQ Music",
  netease: "NetEase",
  kugou: "Kugou",
  ytmusic: "YouTube Music",
  "ytmusic-estimate": "YouTube Music",
  "cover-estimate": "the original song",
  "original-timing": "the original song",
};

export function foundMessage(source: LyricsSource): string {
  if (source === "instrumental") return "No lyrics found: it looks instrumental";
  const from = FROM[source];
  return from ? `New lyrics from ${from}` : "New lyrics found";
}

const METHOD: Record<LookupMethod, string> = {
  ytmusic: "YouTube Music",
  lrclib: "LRCLIB",
  sources: "the other sources",
  identify: "the AI",
  "web-search": "web search",
};

/** "a", "a and b", "a, b and c". */
function list(names: string[]): string {
  return names.length < 2 ? names.join("") : `${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}`;
}

/**
 * Nothing (else) was found. "Tried everything" is said only when every
 * method was asked and answered; otherwise the ones that could not be asked
 * are named, with how long until they are asked again when that is known.
 */
export function notFoundMessage(tried: Tried[]): string {
  const nothing = tried.some((t) => t.end === "rejected") ? "Nothing else found" : "Nothing found";
  const unasked = tried.filter((t) => t.end === "unavailable");
  if (unasked.length) {
    const wait = Math.max(0, ...unasked.map((t) => t.retrySecs ?? 0));
    const again = wait ? ` (again in ${Math.ceil(wait / 60)} min)` : "";
    return `${nothing}, but ${list(unasked.map((t) => METHOD[t.method]))} couldn't be asked${again}`;
  }
  if (tried.some((t) => t.end === "off")) return `Tried everything that's turned on. ${nothing}.`;
  return `Tried everything. ${nothing}.`;
}

/** Something was rejected for the song, so starting over would bring lyrics back. */
export function canStartOver(tried: Tried[]): boolean {
  return tried.some((t) => t.end === "rejected");
}

/** What a failed lookup tried; nothing when it failed some other way. */
export function triedOf(err: unknown): Tried[] {
  const tried = (err as { tried?: unknown } | null)?.tried;
  return Array.isArray(tried) ? (tried as Tried[]) : [];
}
