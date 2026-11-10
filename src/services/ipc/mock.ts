/**
 * Browser-mode backend: a fake player driven by URL params so the UI can be
 * built and screenshotted without Tauri.
 *
 *   ?scenario=idle|playing|translation|furigana|estimate|ytmusic|missing   (default: playing)
 *   &stage=identify|web-search|sources|other-recording   the lookup stays in that pass
 *   &hover=status|unlock   the cursor rests on that part of the (locked) strip
 *   &move (&firstrun)   start unlocked, in Move mode (as on a first run)
 *   &t=12.5     start at 12.5 s
 *   &paused     freeze the clock (deterministic screenshots)
 *   &retime     5 s in, push re-timed lyrics for the same track
 *   &upgrade=4  with `estimate`: push real timing 4 s after the lyrics load,
 *               as the backend does when "Extra lyric sources" finds some
 *               (also on, after 4 s, when that setting is on)
 *
 * `estimate` serves LRCLIB-style plain lyrics: the right text, guessed timing.
 * `ytmusic` serves the lyrics as YouTube Music's own timed lyrics.
 */
import type { Backend, BackendEvents, SavedChange, SavedSongDetail } from "./backend";
import type { LookupStage, LyricLine, PrealignStatus, Span, UpdateStatus } from "../../lib/types";
import { translationKey } from "../../lib/lyrics";
import { englishSong, japaneseSong, readings, type MockSong } from "./mockData";

const params = new URLSearchParams(typeof location === "undefined" ? "" : location.search);
export const mockScenario = params.get("scenario") ?? "playing";

const songs = [englishSong, japaneseSong];
/** The fake cache of lyrics, translations and timings (empty still has a few KB). */
let cache = { entries: 790, bytes: 11_600_000 };

/** The fake cache's songs, editable from Settings → Saved lyrics. */
const DAY = 86_400;
const now = () => Math.floor(Date.now() / 1000);
function savedSong(s: MockSong, source: string, age: number, timing: boolean): SavedSongDetail {
  const lines = s.lines.map(({ time, text, duration }) => ({ time, text, duration }));
  const translated = lines.map((l) => s.translations[translationKey(l.text)] ?? "");
  const translations = translated.some(Boolean) ? [{ key: `${s.track.title}-tr`, lines: translated }] : [];
  const timings = timing ? [{ videoId: "dQw4w9WgXcQ", hash: "h", rejected: false }] : [];
  return { song: summary(s.track.artist, s.track.title, "lyrics", source, now() - age * DAY, lines.length, translations.length, timings.length), lines, translations, timings };
}
function summary(artist: string, track: string, kind: "lyrics" | "missing" | "instrumental", source: string | null, cachedAt: number, lines = 0, translations = 0, timings = 0) {
  return { artist: artist.toLowerCase(), track: track.toLowerCase(), kind, source, lines, cachedAt, expiresAt: source === "edited" ? null : cachedAt + 30 * DAY, translations, timings };
}
const saved: SavedSongDetail[] = [
  savedSong(englishSong, "lrclib", 1, true),
  savedSong(japaneseSong, "ytmusic", 3, false),
  { song: summary("Nobody Knows", "Unreleased Demo", "missing", null, now() - 9 * DAY), lines: [], translations: [], timings: [] },
  { song: summary("Rain Machine", "Interlude", "instrumental", "instrumental", now() - 20 * DAY), lines: [], translations: [], timings: [] },
];
function changeSaved(d: SavedSongDetail, c: SavedChange): SavedSongDetail | null {
  const recount = (x: SavedSongDetail) => ({ ...x, song: { ...x.song, lines: x.lines.length, translations: x.translations.length, timings: x.timings.length } });
  switch (c.op) {
    case "forget":
      return null;
    case "mark":
      return recount({ ...d, lines: [], translations: [], timings: [], song: { ...d.song, kind: c.instrumental ? "instrumental" : "missing", source: c.instrumental ? "instrumental" : null } });
    case "forgetTranslation":
      return recount({ ...d, translations: d.translations.filter((t) => t.key !== c.key) });
    case "forgetTiming":
      return recount({ ...d, timings: d.timings.filter((t) => t.videoId !== c.videoId) });
    case "save": {
      const lines = [...c.lines].sort((a, b) => a.time - b.time);
      const words = (ls: LyricLine[]) => ls.map((l) => l.text).join("\n");
      const sameWords = words(lines) === words(d.lines);
      const changed = JSON.stringify(lines) !== JSON.stringify(d.lines);
      const translations = sameWords ? d.translations.map((t) => c.translations.find((n) => n.key === t.key) ?? t) : [];
      const song = changed ? { ...d.song, kind: "lyrics" as const, source: "edited", cachedAt: now(), expiresAt: null } : d.song;
      return recount({ song, lines, translations, timings: sameWords ? d.timings : [] });
    }
  }
}
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
const MODEL_BYTES = 397_744_269; // echora encoder + text head + frame head
let prealign: PrealignStatus = { enabled: false, state: "off", download: null, error: null, diskBytes: 0, workingOn: null, aligned: 0 };

function setPrealign(p: Partial<PrealignStatus>) {
  prealign = { ...prealign, ...p };
  emit("prealign", prealign);
}

/** Self-update: `?update` pretends a new version is out. */
let update: UpdateStatus = { state: "idle" };

function setUpdate(s: UpdateStatus) {
  update = s;
  emit("update", update);
}

async function fakeCheck(): Promise<UpdateStatus> {
  setUpdate({ state: "checking" });
  await wait(600);
  const notes = ["Faster lyrics lookup.", "Fixes the overlay on a second monitor."].join("\n");
  setUpdate(params.has("update") ? { state: "available", version: "0.2.0", notes } : { state: "upToDate" });
  return update;
}

if (typeof window !== "undefined" && params.has("update")) setTimeout(() => void fakeCheck(), 1500);

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
    const stage = params.get("stage") as LookupStage | null;
    if (stage) {
      emit("progress", { track, stage });
      await new Promise(() => {}); // stays in that pass
    }
    const found = songs.find((s) => s.track.title === track.title);
    if (!found || mockScenario === "missing") throw new Error("No lyrics found");
    if (mockScenario === "ytmusic") return { lines: found.lines, source: "ytmusic", pending: false };
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

  setFrame: async () => {},
  setWidth: async () => {},
  openSettings: async () => void window.open("/settings.html", "lyryc-settings", "width=960,height=700"),
  hideOverlay: async () => {},
  dragStrip: async () => {},
  // The browser preview has no click-through: hover works through the DOM.
  stripState: async () => ({ locked: !params.has("move"), anchor: params.get("anchor") === "top" ? "top" : "bottom", firstRun: params.has("firstrun") }),
  setLocked: async (locked) => emit("locked", locked),
  stripHits: async () => {},
  resetPosition: async () => {},
  openUrl: async (url) => void window.open(url, "_blank", "noopener"),
  appVersion: async () => "0.1.0-dev",

  async prealignConfigure(enabled) {
    if (enabled === prealign.enabled) return prealign;
    setPrealign({ enabled, state: enabled ? (prealign.diskBytes ? "ready" : "downloading") : "off" });
    if (enabled && !prealign.diskBytes) fakeDownload();
    return prealign;
  },
  prealignStatus: async () => prealign,
  /** A pretend listen: the lines four seconds apart. A first line "fail" isn't heard. */
  async alignLyrics(_artist, _track, lines) {
    await wait(2500);
    if (lines[0]?.toLowerCase() === "fail") throw "these words weren't heard in the song. Check that they are its lyrics";
    return lines.map((text, i) => ({ time: 12.34 + i * 4, text, duration: 4 }));
  },
  lookaheadConfigure: async () => {},
  markInstrumental: async () => {},
  async prealignRemoveFiles() {
    setPrealign({ enabled: false, state: "off", download: null, diskBytes: 0 });
    return prealign;
  },
  cacheSize: async () => cache,
  savedSongs: async () => saved.map((d) => d.song),
  savedSong: async (artist, track) => saved.find((d) => d.song.artist === artist && d.song.track === track) ?? null,
  async savedSongChange(artist, track, change) {
    const i = saved.findIndex((d) => d.song.artist === artist && d.song.track === track);
    if (i < 0) return null;
    const next = changeSaved(saved[i], change);
    if (next) saved[i] = next;
    else saved.splice(i, 1);
    return next;
  },
  async cacheClear() {
    cache = { entries: 0, bytes: 24_576 };
    return cache;
  },

  updateStatus: async () => update,
  updateCheck: fakeCheck,
  async updateInstall() {
    if (update.state !== "available") throw new Error("No update to install. Check for updates first.");
    const { version } = update;
    const total = 9_400_000;
    for (let done = 0; done < total; done += 940_000) {
      setUpdate({ state: "downloading", version, done, total });
      await wait(150);
    }
    setUpdate({ state: "error", message: `Couldn't install ${version}: the browser preview can't restart itself.` });
    throw new Error("browser preview");
  },

  subscribe(on) {
    subscribers.add(on);
    setTimeout(() => {
      if (mockScenario !== "idle") on.track?.(song().track);
      on.playing?.(playing);
      if (mockScenario !== "idle") emitTime();
      const target = params.get("hover");
      if (target) on.hover?.({ inside: true, target });
    }, 30);
    // Stand in for the backend's cursor poll (it decides hover while locked):
    // over the strip or the pill beside it.
    let inside = false;
    const follow = (e: PointerEvent) => {
      const now = !!(e.target as Element | null)?.closest?.("[data-strip], [data-pill]");
      if (now !== inside && !params.get("hover")) on.hover?.({ inside: (inside = now), target: null });
    };
    document.addEventListener("pointermove", follow);
    return () => {
      subscribers.delete(on);
      document.removeEventListener("pointermove", follow);
    };
  },
};
