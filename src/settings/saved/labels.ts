/** How the saved-lyrics browser names a song's source and dates. */
import type { SavedSong } from "../../services/ipc/backend";

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
