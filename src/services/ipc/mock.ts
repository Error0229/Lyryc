/**
 * Browser-mode backend: a fake player driven by URL params so the UI can be
 * built and screenshotted without Tauri.
 *
 *   ?scenario=idle|playing|translation|furigana|estimate   (default: playing)
 *   &t=12.5     start at 12.5 s
 *   &paused     freeze the clock (deterministic screenshots)
 *   &retime     5 s in, push re-timed lyrics for the same track
 *   &upgrade=4  with `estimate`: push real timing 4 s after the lyrics load,
 *               as the backend does when "Extra lyric sources" finds some
 *               (also on, after 4 s, when that setting is on)
 *
 * `estimate` serves LRCLIB-style plain lyrics: the right text, guessed timing.
 */
import type { Backend, BackendEvents } from "./backend";
import type { LyricLine, PrealignStatus, Span } from "../../lib/types";
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

/** Word-level timing: a pretend download that finishes in a few seconds. */
const MODEL_BYTES = 356_289_622;
let prealign: PrealignStatus = { enabled: false, state: "off", download: null, error: null, diskBytes: 0, workingOn: null, aligned: 0 };

function setPrealign(p: Partial<PrealignStatus>) {
  prealign = { ...prealign, ...p };
  emit("prealign", prealign);
}

function fakeDownload() {
  let done = 0;
  const step = () => {
    if (!prealign.enabled) return;
    done = Math.min(MODEL_BYTES, done + MODEL_BYTES / 20);
    if (done < MODEL_BYTES) {
      setPrealign({ state: "downloading", download: { file: "model", done, total: MODEL_BYTES } });
      setTimeout(step, 200);
    } else {
      setPrealign({ state: "ready", download: null, diskBytes: MODEL_BYTES + 12_418_080 + 18_000_000 });
    }
  };
  step();
}

/** Simulates the backend delivering better timing mid-song. */
function pushRetimed() {
  const lines = song().lines.map((l) => ({
    ...l,
    time: l.time + 0.35,
    words: l.words?.map((w) => ({ ...w, time: w.time + 0.35, duration: (w.duration ?? 0) * 0.8 })),
  }));
  emit("lyrics", { track: song().track, lines, source: "lrclib" });
}
if (typeof window !== "undefined" && params.has("retime")) setTimeout(pushRetimed, 5000);

/** Plain lyrics spread evenly between 8% and 92% of the track, like the backend's estimate. */
function estimated(s: MockSong): LyricLine[] {
  const total = s.track.duration ?? 0;
  const start = total * 0.08;
  const step = (total * 0.84) / s.lines.length;
  return s.lines.map((l, i) => ({ time: start + i * step, duration: step, text: l.text }));
}

const upgradeAfter = Number(params.get("upgrade")) || 4;

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

  async fetchLyrics(track, extraSources) {
    await wait(350);
    const found = songs.find((s) => s.track.title === track.title);
    if (!found) throw new Error("No lyrics found");
    if (mockScenario !== "estimate") return { lines: found.lines, source: "lrclib", pending: false };

    const pending = extraSources || params.has("upgrade");
    if (pending) {
      const upgrade = { track: { title: track.title, artist: track.artist }, lines: found.lines, source: "qq" as const };
      setTimeout(() => emit("lyrics", upgrade), upgradeAfter * 1000);
    }
    return { lines: estimated(found), source: "lrclib-estimate", pending };
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

  async prealignConfigure(enabled) {
    if (enabled === prealign.enabled) return prealign;
    setPrealign({ enabled, state: enabled ? (prealign.diskBytes ? "ready" : "downloading") : "off" });
    if (enabled && !prealign.diskBytes) fakeDownload();
    return prealign;
  },
  prealignStatus: async () => prealign,
  async prealignRemoveFiles() {
    setPrealign({ enabled: false, state: "off", download: null, diskBytes: 0 });
    return prealign;
  },

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
