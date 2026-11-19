/** How the saved-lyrics browser names a song's source and dates. */
import type { MadeWith, SavedSong, SavedTranslation } from "../../services/ipc/backend";

const SOURCES: Record<string, string> = {
  lrclib: "LRCLIB",
  "lrclib-estimate": "LRCLIB, estimated timing",
  qq: "QQ Music",
  netease: "NetEase",
  kugou: "Kugou",
  ytmusic: "YouTube Music",
  "ytmusic-estimate": "YouTube Music, estimated timing",
  "cover-estimate": "Original's words, estimated timing",
  "sources-estimate": "Estimated timing",
  "original-timing": "Original's timing",
  edited: "Edited by you",
};

export function sourceLabel(s: SavedSong): string {
  if (s.kind === "missing") return "No lyrics found";
  if (s.kind === "instrumental") return "Instrumental";
  // Rows from before sources were recorded are LRCLIB.
  return `${SOURCES[s.source ?? "lrclib"] ?? s.source} · ${s.lines} lines`;
}

const DAY = 86_400;
const date = (t: number) => new Date(t * 1000).toLocaleDateString(undefined, { month: "short", day: "numeric" });

/** "Saved Nov 3, looked up again Dec 3" / "kept until you delete it". */
export function savedOn(s: SavedSong): string {
  const until = s.expiresAt === null ? "kept until you delete it" : `looked up again ${s.expiresAt * 1000 <= Date.now() ? "next time it plays" : date(s.expiresAt)}`;
  return `Saved ${date(s.cachedAt)}, ${until}`;
}

/** "today", "3 days ago", or the date. */
export function ago(t: number, now = Date.now() / 1000): string {
  const days = Math.floor((now - t) / DAY);
  if (days < 1) return "today";
  if (days === 1) return "yesterday";
  return days < 60 ? `${days} days ago` : date(t);
}

/** Names a language code ("zh-TW" → "Chinese Traditional"). */
export type LanguageName = (code: string) => string;

/** At most `n` characters, cut at a word when there is one. */
function clip(s: string, n: number): string {
  if (s.length <= n) return s;
  const cut = s.slice(0, n);
  const space = cut.lastIndexOf(" ");
  return `${(space > n / 2 ? cut.slice(0, space) : cut).trimEnd()}…`;
}

/** What made a translation, a phrase per setting, always in this order. */
function phrases(m: MadeWith, language: LanguageName): string[] {
  const style = m.style === "custom" ? `prompt "${clip(m.custom_prompt?.trim() || "custom", 24)}"` : `${m.style} style`;
  const off = [!m.preserve_rhyme && "rhyme", !m.preserve_meaning && "meaning", !m.maintain_structure && "structure"].filter(Boolean);
  return [language(m.target_language), style, `${m.model} (${m.provider})`, off.length ? `not keeping ${off.join(", ")}` : ""];
}

const UNKNOWN = "Earlier settings";

/**
 * Short names for a song's translations, to pick one: only what tells them
 * apart ("Poetic style" / "Natural style"), the language when nothing does.
 * One made before its settings were recorded is "Earlier settings".
 */
export function translationNames(list: readonly SavedTranslation[], language: LanguageName): string[] {
  const all = list.map((t) => t.madeWith && phrases(t.madeWith, language));
  const known = all.filter((p): p is string[] => !!p);
  const differs = (k: number) => new Set(known.map((p) => p[k])).size > 1;
  const names = all.map((p) => {
    if (!p) return UNKNOWN;
    const told = p.filter((x, k) => x && differs(k));
    return capital((told.length ? told : [p[0]]).join(" · "));
  });
  // Same settings, told apart by when (a provider renamed, say).
  return names.map((n, i) => (names.indexOf(n) === names.lastIndexOf(n) ? n : `${n} · ${date(list[i].cachedAt)}`));
}

/** Everything that made it, and when: "Japanese · poetic style · gpt-4.1 (GitHub) · Nov 3". */
export function translationMadeWith(t: SavedTranslation, language: LanguageName): string {
  const made = t.madeWith ? phrases(t.madeWith, language).filter(Boolean) : ["Made before its settings were recorded"];
  return capital([...made, date(t.cachedAt)].join(" · "));
}

const capital = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);
