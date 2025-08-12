export interface LRCLibRecord {
  trackName: string;
  artistName: string;
  duration?: number;
  plainLyrics?: string;
  syncedLyrics?: string;
}

export async function fetchLRCLib(track: string, artist: string): Promise<LRCLibRecord | null> {
  const base = "https://lrclib.net/api/search";
  const paramsQ = new URLSearchParams({ q: `${track} ${artist}` });
  const paramsExact = new URLSearchParams({ track_name: track, artist_name: artist });
  const urls = [
    `${base}?${paramsQ.toString()}`,
    `${base}?${paramsExact.toString()}`,
  ];
  for (const url of urls) {
    const res = await fetch(url, { headers: { "User-Agent": "LyrycTests/1.0" } });
    if (!res.ok) continue;
    const arr = (await res.json()) as any[];
    if (!Array.isArray(arr) || arr.length === 0) continue;
    const pick = arr.find((x) => x.syncedLyrics && String(x.syncedLyrics).trim().length > 0) ?? arr[0];
    return {
      trackName: pick.trackName ?? track,
      artistName: pick.artistName ?? artist,
      duration: typeof pick.duration === "number" ? pick.duration : undefined,
      plainLyrics: pick.plainLyrics ?? undefined,
      syncedLyrics: pick.syncedLyrics ?? undefined,
    };
  }
  return null;
}

