/**
 * Browser-mode backend: a fake player driven by URL params so the UI can be
 * built and screenshotted without Tauri.
 *
 *   ?scenario=idle|playing|translation|furigana   (default: playing)
 *   &t=12.5     start at 12.5 s
 *   &paused     freeze the clock (deterministic screenshots)
 *   &retime     5 s in, push re-timed lyrics for the same track
 */
import type { Backend, BackendEvents } from "./backend";
import type { Span } from "../../lib/types";
import { translationKey } from "../../lib/lyrics";
import { englishSong, japaneseSong, readings, type MockSong } from "./mockData";

const params = new URLSearchParams(typeof location === "undefined" ? "" : location.search);
export const mockScenario = params.get("scenario") ?? "playing";

const songs = [englishSong, japaneseSong];
let songIndex = mockScenario === "furigana" ? 1 : 0;
let playing = !params.has("paused");
let base = Number(params.get("t") ?? 0);
let at = performance.now();
const subscribers = new Set<Partial<BackendEvents>>();

const song = (): MockSong => songs[songIndex];
const mediaTime = () => (playing ? base + (performance.now() - at) / 1000 : base);
const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

function seek(t: number) {
  base = t;
  at = performance.now();
}

function emit<K extends keyof BackendEvents>(key: K, ...args: Parameters<BackendEvents[K]>) {
  subscribers.forEach((s) => (s[key] as ((...a: unknown[]) => void) | undefined)?.(...args));
}

function emitTime() {
  const duration = song().track.duration ?? 0;
  if (mediaTime() > duration) seek(0);
  emit("time", { currentTime: mediaTime(), duration, isPlaying: playing });
}

function changeSong(step: number) {
  songIndex = (songIndex + step + songs.length) % songs.length;
  seek(0);
  emit("track", song().track);
  emitTime();
}

if (typeof window !== "undefined") setInterval(emitTime, 700);

/** Simulates the backend delivering better timing mid-song. */
function pushRetimed() {
  const lines = song().lines.map((l) => ({
    ...l,
    time: l.time + 0.35,
    words: l.words?.map((w) => ({ ...w, time: w.time + 0.35, duration: (w.duration ?? 0) * 0.8 })),
  }));
  emit("lyrics", { track: song().track, lines });
}
if (typeof window !== "undefined" && params.has("retime")) setTimeout(pushRetimed, 5000);

/** Greedy longest-match furigana over the fixture dictionary. */
function furigana(text: string): Span[] {
  const keys = Object.keys(readings).sort((a, b) => b.length - a.length);
  const out: Span[] = [];
  let plain = "";
  for (let i = 0; i < text.length; ) {
    const k = keys.find((key) => text.startsWith(key, i));
    if (!k) {
      plain += text[i++];
      continue;
    }
    if (plain) out.push({ text: plain });
    plain = "";
    out.push({ text: k, reading: readings[k] });
    i += k.length;
  }
  if (plain) out.push({ text: plain });
  return out;
}

export const mockBackend: Backend = {
  initExtension: async () => {},
  currentTrack: async () => (mockScenario === "idle" ? null : song().track),

  async fetchLyrics(track) {
    await wait(350);
    const found = songs.find((s) => s.track.title === track.title);
    if (!found) throw new Error("No lyrics found");
    return found.lines;
  },

  async translate(lines, prefs) {
    await wait(500);
    const dict = { ...englishSong.translations, ...japaneseSong.translations };
    return lines
      .filter((l) => l.text)
      .map((l) => ({ original: l.text, translated: dict[translationKey(l.text)] ?? `[${prefs.targetLanguage}] ${l.text}` }));
  },

  furigana: async (text) => furigana(text),

  languages: async () => [
    ["auto", "Auto-detect"],
    ["en", "English"],
    ["es", "Spanish"],
    ["fr", "French"],
    ["de", "German"],
    ["ja", "Japanese"],
    ["ko", "Korean"],
    ["zh", "Chinese"],
  ],

  async playback(cmd, seekTime) {
    if (cmd === "next" || cmd === "previous") return changeSong(cmd === "next" ? 1 : -1);
    if (cmd === "seek" && seekTime !== undefined) seek(seekTime);
    if (cmd === "play" || cmd === "pause") {
      seek(mediaTime());
      playing = cmd === "play";
      emit("playing", playing);
    }
    emitTime();
  },

  fitHeight: async () => {},
  openSettings: async () => void window.open("/settings.html", "lyryc-settings", "width=880,height=640"),
  hideOverlay: async () => {},
  startDrag: async () => {},
  openUrl: async (url) => void window.open(url, "_blank", "noopener"),
  appVersion: async () => "0.1.0-dev",

  subscribe(on) {
    subscribers.add(on);
    setTimeout(() => {
      if (mockScenario !== "idle") on.track?.(song().track);
      on.playing?.(playing);
      if (mockScenario !== "idle") emitTime();
    }, 30);
    return () => void subscribers.delete(on);
  },
};
