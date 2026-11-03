/** Which incoming lyric lists may replace what the overlay shows. Pure. */
import { trackKey } from "../lib/offset";
import type { LyricsSource, Track } from "../lib/types";

type Named = Pick<Track, "title" | "artist">;

export const isEstimate = (source: LyricsSource | null) => source === "lrclib-estimate";

export function sameTrack(a: Named | null, b: Named | null): boolean {
  return !!a && !!b && trackKey(a.artist, a.title) === trackKey(b.artist, b.title);
}

/**
 * Lyrics are for the track on screen, never trade real timing for an
 * estimate, and never trade word timing for line timing. The last two cover
 * a background upgrade that lands before the answer it replaces.
 */
export function accepts(
  current: { track: Named | null; source: LyricsSource | null },
  incoming: { track: Named; source: LyricsSource },
): boolean {
  if (!sameTrack(current.track, incoming.track)) return false;
  if (current.source === "aligned") return incoming.source === "aligned";
  return !(isEstimate(incoming.source) && current.source !== null && !isEstimate(current.source));
}
