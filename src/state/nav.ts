/**
 * Cross-window navigation: the overlay asks the settings window to show a
 * page, or a saved song's editor. Written to localStorage so it works whether
 * or not the settings window is already open (the open window gets a
 * `storage` event).
 */
import type { Track } from "../lib/types";
import { backend } from "../services/ipc/backend";

export const pages = ["appearance", "lyrics", "translation", "providers", "furigana", "about"] as const;
export type SettingsPage = (typeof pages)[number];

const NAV_KEY = "lyryc-settings-page";
const FRESH_MS = 15_000;

function parse(raw: string | null): SettingsPage | null {
  try {
    const v = JSON.parse(raw ?? "null");
    return v && pages.includes(v.page) && Date.now() - v.at < FRESH_MS ? v.page : null;
  } catch {
    return null;
  }
}

export function openSettings(page?: SettingsPage): Promise<void> {
  if (page) localStorage.setItem(NAV_KEY, JSON.stringify({ page, at: Date.now() }));
  return backend.openSettings();
}

/** Page requested just before this window opened, if any. */
export function requestedPage(): SettingsPage | null {
  const page = parse(localStorage.getItem(NAV_KEY));
  localStorage.removeItem(NAV_KEY);
  return page;
}

export function onPageRequest(cb: (page: SettingsPage) => void): () => void {
  const handler = (e: StorageEvent) => {
    const page = e.key === NAV_KEY ? parse(e.newValue) : null;
    if (page) cb(page);
  };
  window.addEventListener("storage", handler);
  return () => window.removeEventListener("storage", handler);
}

/** A song by the names it plays under. */
export type SongRef = Pick<Track, "title" | "artist">;

const SONG_KEY = "lyryc-settings-song";

function parseSong(raw: string | null): SongRef | null {
  try {
    const v = JSON.parse(raw ?? "null");
    const fresh = v && Date.now() - v.at < FRESH_MS && typeof v.title === "string" && typeof v.artist === "string";
    return fresh ? { title: v.title, artist: v.artist } : null;
  } catch {
    return null;
  }
}

/** Settings on the song's saved lyrics, in their editor (Lyrics & sync → Browse → the song). */
export function openSavedSong(song: SongRef): Promise<void> {
  localStorage.setItem(SONG_KEY, JSON.stringify({ title: song.title, artist: song.artist, at: Date.now() }));
  return openSettings("lyrics");
}

/** The song whose editor was asked for just before the saved lyrics showed, if any. */
export function requestedSong(): SongRef | null {
  const song = parseSong(localStorage.getItem(SONG_KEY));
  localStorage.removeItem(SONG_KEY);
  return song;
}

export function onSongRequest(cb: (song: SongRef) => void): () => void {
  const handler = (e: StorageEvent) => {
    const song = e.key === SONG_KEY ? parseSong(e.newValue) : null;
    if (!song) return;
    localStorage.removeItem(SONG_KEY);
    cb(song);
  };
  window.addEventListener("storage", handler);
  return () => window.removeEventListener("storage", handler);
}
