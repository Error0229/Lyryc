/**
 * Typed wrapper over the real backend: every command here exists in
 * src-tauri/src/lib.rs `generate_handler!`, every event is emitted by the Rust
 * side. In a plain browser the same interface is served by ./mock.
 */
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Anchor, HitRect, LookupStage, LyricLine, LyricsSource, PlaybackCommand, PrealignStatus, Span, StripHover, StripView, TimedLyrics, TimeSample, Track, UpdateStatus } from "../../lib/types";
import type { TranslationPrefs } from "../../state/schema";
import { mockBackend } from "./mock";

export interface Translated {
  original: string;
  translated: string;
}

export interface BackendEvents {
  track: (t: Track) => void;
  time: (s: TimeSample) => void;
  playing: (p: boolean) => void;
  /** Locked (click-through) or Move mode, from any source: hotkey, tray, the overlay. */
  locked: (on: boolean) => void;
  /** The strip's anchored edge changed: live while it is dragged, and after it settles. */
  anchor: (a: Anchor) => void;
  /** The cursor over the locked strip (`strip-hover`). */
  hover: (h: StripHover) => void;
  /**
   * Better timing for a track that is already showing (`lyrics-timing-updated`).
   * `track` is the title/artist the request was made with, so a late upgrade
   * for an earlier song can be told apart.
   */
  lyrics: (p: { track: Pick<Track, "title" | "artist">; lines: LyricLine[]; source: LyricsSource }) => void;
  /** The lookup for `track` moved on to a slower pass (`lyrics-progress`). */
  progress: (p: { track: Pick<Track, "title" | "artist">; stage: LookupStage }) => void;
  /** A pending upgrade for `track` ended without better timing (`lyrics-timing-final`). */
  timingFinal: (track: Pick<Track, "title" | "artist">) => void;
  /** Word-level timing: download progress and state changes (`prealign-status`). */
  prealign: (s: PrealignStatus) => void;
  /** A song's saved lyrics were edited in Settings (`saved-lyrics-changed`): names as the cache keys them. */
  savedLyrics: (song: Pick<Track, "title" | "artist">) => void;
  /** The lookahead found an upcoming song's lyrics (`lyrics-ahead`): translate them before it plays. */
  ahead: (lines: LyricLine[]) => void;
  /** AI providers or routes changed, in any window (`llm-settings-changed`). */
  llmSettings: () => void;
  /** Self-update checks, downloads and failures (`update-status`). */
  update: (s: UpdateStatus) => void;
}

export interface Backend {
  initExtension(): Promise<void>;
  currentTrack(): Promise<Track | null>;
  /** `extraSources`: the user's opt-in for YouTube Music's lyrics and QQ Music / NetEase / Kugou. */
  /**
   * `refetch`: "Search again". The backend forgets the cached answer and asks
   * the LLM (with web search when the provider has it) before the player's names.
   */
  fetchLyrics(track: Track, extraSources: boolean, webSearch: boolean, refetch?: boolean): Promise<TimedLyrics>;
  /** Empty when the lyrics are already in the target language. */
  /** The user says the track has no words; "Search again" (`refetch`) undoes it. */
  markInstrumental(track: Track): Promise<void>;
  translate(lines: LyricLine[], prefs: TranslationPrefs): Promise<Translated[]>;
  furigana(text: string): Promise<Span[]>;
  languages(): Promise<Array<[code: string, name: string]>>;
  playback(cmd: PlaybackCommand, seekTime?: number): Promise<void>;
  /** The strip's height (CSS px; it grows away from its anchored edge) and the transparent room around it. */
  setFrame(height: number, room: { x: number; y: number }): Promise<void>;
  /** Move mode's edge handles (CSS px), symmetric about the strip's centre; `done` saves it. */
  setWidth(width: number, done: boolean): Promise<void>;
  openSettings(): Promise<void>;
  hideOverlay(): Promise<void>;
  /** A press on the strip in Move mode: the backend drags it until the button is let go. */
  dragStrip(): Promise<void>;
  stripState(): Promise<StripView>;
  setLocked(locked: boolean): Promise<void>;
  /** Replaces the rects the backend hit-tests the cursor against while locked. */
  stripHits(hits: HitRect[]): Promise<void>;
  resetPosition(): Promise<void>;
  openUrl(url: string): Promise<void>;
  appVersion(): Promise<string>;
  /**
   * Word-level timing (YouTube Music). The backend keeps no copy of the
   * settings: the overlay mirrors `prealign` and `extraSources` here on start
   * and on change. Turning it on starts the one-time downloads.
   */
  prealignConfigure(enabled: boolean, extraSources: boolean): Promise<PrealignStatus>;
  prealignStatus(): Promise<PrealignStatus>;
  /**
   * Times a saved song's lyrics that have no times by listening to it: the
   * YouTube Music video it last played as (word-level timing must be on).
   * Takes a minute or so; resolves to `lines` with their times, and rejects
   * with the reason when it can't (never played there, words not heard, ...).
   */
  alignLyrics(artist: string, track: string, lines: string[]): Promise<LyricLine[]>;
  /** "Prepare lyrics for upcoming songs", mirrored like `prealignConfigure`. */
  lookaheadConfigure(enabled: boolean, extraSources: boolean): Promise<void>;
  /** The cache of lyrics, translations and word timings: how much is saved. */
  cacheSize(): Promise<CacheSize>;
  /** Empties that cache; resolves to what is left. */
  cacheClear(): Promise<CacheSize>;
  /** Every saved song, newest first. */
  savedSongs(): Promise<SavedSong[]>;
  /** One saved song with its lines, translations and word timings; null once it's gone. */
  savedSong(artist: string, track: string): Promise<SavedSongDetail | null>;
  /** Applies the user's change to a saved song and resolves to the song as it is now. */
  savedSongChange(artist: string, track: string, change: SavedChange): Promise<SavedSongDetail | null>;
  /** Deletes the model, runtime and yt-dlp, and turns the feature off. */
  prealignRemoveFiles(): Promise<PrealignStatus>;
  /** The backend checks for updates by itself; `updateCheck` is "Check now". */
  updateStatus(): Promise<UpdateStatus>;
  updateCheck(): Promise<UpdateStatus>;
  /** Downloads, installs and restarts the app. Resolves only if that fails. */
  updateInstall(): Promise<void>;
  subscribe(on: Partial<BackendEvents>): () => void;
}

/** `cache_size`: entries across lyrics, translations and timings, and bytes on disk. */
export interface CacheSize {
  entries: number;
  bytes: number;
}

/** `saved_songs`: a song in the cache. `artist`/`track` are lowercase, as the cache keys them. */
export interface SavedSong {
  artist: string;
  track: string;
  /** "missing": nothing was found for it. */
  kind: "lyrics" | "missing" | "instrumental";
  source: string | null;
  lines: number;
  cachedAt: number;
  /** Unix seconds when it's looked up again; null: kept (edited lyrics). */
  expiresAt: number | null;
  translations: number;
  timings: number;
}

/** A saved translation: one line per lyric line. */
export interface SavedTranslation {
  key: string;
  lines: string[];
}

/** Saved word timing for one YouTube video; `rejected`: aligned and thrown away. */
export interface SavedTiming {
  videoId: string;
  hash: string;
  rejected: boolean;
}

export interface SavedSongDetail {
  song: SavedSong;
  lines: LyricLine[];
  translations: SavedTranslation[];
  timings: SavedTiming[];
}

/**
 * `saved_song_change`. A save drops the song's translations when the words
 * change, and its word timing when the lines change at all.
 */
export type SavedChange =
  | { op: "save"; lines: LyricLine[]; translations: SavedTranslation[] }
  | { op: "forget" }
  | { op: "mark"; instrumental: boolean }
  | { op: "forgetTranslation"; key: string }
  | { op: "forgetTiming"; videoId: string; hash: string };

/** Wire shape of the `lyrics-timing-updated` payload. */
interface TimingUpdate {
  title: string;
  artist: string;
  lines: LyricLine[];
  source: LyricsSource;
}

interface TranslationResult {
  success: boolean;
  translated_lines: Array<{ original_text: string; translated_text: string }>;
  error?: string | null;
  is_same_language: boolean;
}

/** Wire shape of `TranslationConfig` (snake_case, no keys: the backend owns them). */
function translationConfig(p: TranslationPrefs) {
  return {
    source_language: p.sourceLanguage,
    target_language: p.targetLanguage,
    style: p.style,
    custom_prompt: p.style === "custom" ? p.customPrompt : null,
    preserve_rhyme: p.preserveRhyme,
    preserve_meaning: p.preserveMeaning,
    maintain_structure: p.maintainStructure,
    provider: "",
  };
}

const tauriBackend: Backend = {
  initExtension: () => invoke("init_extension_connection"),
  currentTrack: () => invoke<Track | null>("get_current_track"),
  fetchLyrics: (t, extraSources, webSearch, refetch = false) =>
    invoke<TimedLyrics>("fetch_lyrics_timed", {
      trackName: t.title,
      artistName: t.artist,
      duration: t.duration || null, // the extension sends 0 for "unknown"
      extraSources,
      webSearch,
      refetch,
      videoId: t.videoId ?? null,
    }),

  markInstrumental: (t) => invoke<void>("mark_instrumental", { trackName: t.title, artistName: t.artist, videoId: t.videoId ?? null }),

  async translate(lines, prefs) {
    const lyrics = lines.map(({ time, text, duration }) => ({ time, text, duration: duration ?? null }));
    const r = await invoke<TranslationResult>("translate_lyrics_command", { lyrics, config: translationConfig(prefs) });
    if (!r.success) throw new Error(r.error || "Translation failed");
    if (r.is_same_language) return [];
    return r.translated_lines.map((l) => ({ original: l.original_text, translated: l.translated_text }));
  },

  async furigana(text) {
    const r = await invoke<{ segments: Span[] }>("generate_furigana_command", { text });
    return r.segments;
  },

  languages: () => invoke<Array<[string, string]>>("get_supported_languages"),
  playback: (command, seekTime) => invoke("send_playback_command", { command, seekTime: seekTime ?? null }),
  setFrame: (height, room) => invoke<void>("strip_set_frame", { height, room }),
  setWidth: (width, done) => invoke<void>("strip_set_width", { width, done }),
  openSettings: () => invoke("open_settings_window"),
  hideOverlay: () => invoke("minimize_to_tray"),
  dragStrip: () => invoke<void>("strip_drag"),
  stripState: () => invoke<StripView>("strip_state"),
  setLocked: (locked) => invoke<void>("strip_set_locked", { locked }),
  stripHits: (hits) => invoke<void>("strip_hits", { hits }),
  resetPosition: () => invoke<void>("strip_reset"),

  async openUrl(url) {
    const { openUrl } = await import("@tauri-apps/plugin-opener");
    await openUrl(url);
  },

  async appVersion() {
    const { getVersion } = await import("@tauri-apps/api/app");
    return getVersion();
  },

  prealignConfigure: (enabled, extraSources) => invoke<PrealignStatus>("prealign_configure", { enabled, extraSources }),
  prealignStatus: () => invoke<PrealignStatus>("prealign_status"),
  alignLyrics: (artist, track, lines) => invoke<LyricLine[]>("prealign_align", { artist, track, lines }),
  lookaheadConfigure: (enabled, extraSources) => invoke<void>("lookahead_configure", { enabled, extraSources }),
  prealignRemoveFiles: () => invoke<PrealignStatus>("prealign_remove_files"),
  cacheSize: () => invoke<CacheSize>("cache_size"),
  cacheClear: () => invoke<CacheSize>("cache_clear"),
  savedSongs: () => invoke<SavedSong[]>("saved_songs"),
  savedSong: (artist, track) => invoke<SavedSongDetail | null>("saved_song", { artist, track }),
  savedSongChange: (artist, track, change) => invoke<SavedSongDetail | null>("saved_song_change", { artist, track, change }),
  updateStatus: () => invoke<UpdateStatus>("update_status"),
  updateCheck: () => invoke<UpdateStatus>("update_check"),
  updateInstall: () => invoke<void>("update_install"),

  subscribe(on) {
    const pending = [
      on.track && listen<Track>("track-updated", (e) => on.track!(e.payload)),
      on.time && listen<TimeSample>("track-time-update", (e) => on.time!(e.payload)),
      on.playing && listen<boolean>("playback-state", (e) => on.playing!(e.payload)),
      on.locked && listen<boolean>("strip-locked", (e) => on.locked!(e.payload)),
      on.anchor && listen<Anchor>("strip-anchor", (e) => on.anchor!(e.payload)),
      on.hover && listen<StripHover>("strip-hover", (e) => on.hover!(e.payload)),
      on.lyrics &&
        listen<TimingUpdate>("lyrics-timing-updated", ({ payload: p }) =>
          on.lyrics!({ track: { title: p.title, artist: p.artist }, lines: p.lines, source: p.source }),
        ),
      on.prealign && listen<PrealignStatus>("prealign-status", (e) => on.prealign!(e.payload)),
      on.progress &&
        listen<{ title: string; artist: string; stage: LookupStage }>("lyrics-progress", ({ payload: p }) =>
          on.progress!({ track: { title: p.title, artist: p.artist }, stage: p.stage }),
        ),
      on.timingFinal &&
        listen<{ title: string; artist: string }>("lyrics-timing-final", ({ payload: p }) => on.timingFinal!({ title: p.title, artist: p.artist })),
      on.llmSettings && listen("llm-settings-changed", () => on.llmSettings!()),
      on.ahead && listen<LyricLine[]>("lyrics-ahead", (e) => on.ahead!(e.payload)),
      on.savedLyrics && listen<{ title: string; artist: string }>("saved-lyrics-changed", (e) => on.savedLyrics!(e.payload)),
      on.update && listen<UpdateStatus>("update-status", (e) => on.update!(e.payload)),
    ].filter((p): p is Promise<() => void> => !!p);
    return () => pending.forEach((p) => void p.then((off) => off()));
  },
};

export const inTauri = isTauri();
export const backend: Backend = inTauri ? tauriBackend : mockBackend;
