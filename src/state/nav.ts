/**
 * Cross-window navigation: the overlay asks the settings window to show a
 * page. Written to localStorage so it works whether or not the settings
 * window is already open (the open window gets a `storage` event).
 */
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
