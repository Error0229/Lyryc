import { invoke } from "@tauri-apps/api/core";

export interface LRCLibRaw {
  trackName: string;
  artistName: string;
  albumName?: string;
  duration?: number; // seconds
  plainLyrics: string;
  syncedLyrics: string; // LRC raw
}

export async function fetchLRCLibRaw(trackName: string, artistName: string): Promise<LRCLibRaw | null> {
  try {
    const res = await invoke("fetch_lrclib_raw", {
      trackName,
      artistName,
    });
    return res as LRCLibRaw;
  } catch (e) {
    console.warn("fetchLRCLibRaw failed:", e);
    return null;
  }
}

