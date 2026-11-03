/**
 * Typed wrapper over the real backend: every command here exists in
 * src-tauri/src/lib.rs `generate_handler!`, every event is emitted by the Rust
 * side. In a plain browser the same interface is served by ./mock.
 */
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { LyricLine, LyricsSource, PlaybackCommand, PrealignStatus, Span, TimedLyrics, TimeSample, Track } from "../../lib/types";
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
  clickThrough: (on: boolean) => void;
  /**
   * Better timing for a track that is already showing (`lyrics-timing-updated`).
   * `track` is the title/artist the request was made with, so a late upgrade
   * for an earlier song can be told apart.
   */
  lyrics: (p: { track: Pick<Track, "title" | "artist">; lines: LyricLine[]; source: LyricsSource }) => void;
  /** Word-level timing: download progress and state changes (`prealign-status`). */
  prealign: (s: PrealignStatus) => void;
}

export interface Backend {
  initExtension(): Promise<void>;
  currentTrack(): Promise<Track | null>;
  /** `extraSources`: the user's opt-in for QQ Music / NetEase / Kugou. */
  fetchLyrics(track: Track, extraSources: boolean): Promise<TimedLyrics>;
  /** Empty when the lyrics are already in the target language. */
  translate(lines: LyricLine[], prefs: TranslationPrefs): Promise<Translated[]>;
  furigana(text: string): Promise<Span[]>;
  languages(): Promise<Array<[code: string, name: string]>>;
  playback(cmd: PlaybackCommand, seekTime?: number): Promise<void>;
  fitHeight(contentHeight: number, hasTranslation: boolean, hasFurigana: boolean): Promise<void>;
  openSettings(): Promise<void>;
  hideOverlay(): Promise<void>;
  startDrag(): Promise<void>;
  openUrl(url: string): Promise<void>;
  appVersion(): Promise<string>;
  /**
   * Word-level timing (YouTube Music). The backend keeps no copy of the
   * settings: the overlay mirrors `prealign` and `extraSources` here on start
   * and on change. Turning it on starts the one-time downloads.
   */
  prealignConfigure(enabled: boolean, extraSources: boolean): Promise<PrealignStatus>;
  prealignStatus(): Promise<PrealignStatus>;
  /** Deletes the model, runtime and yt-dlp, and turns the feature off. */
  prealignRemoveFiles(): Promise<PrealignStatus>;
  subscribe(on: Partial<BackendEvents>): () => void;
}

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
  fetchLyrics: (t, extraSources) =>
    invoke<TimedLyrics>("fetch_lyrics_timed", {
      trackName: t.title,
      artistName: t.artist,
      duration: t.duration || null, // the extension sends 0 for "unknown"
      extraSources,
    }),

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
  fitHeight: (contentHeight, hasTranslation, hasFurigana) =>
    invoke("adjust_window_height", { contentHeight, hasTranslation, hasFurigana }),
  openSettings: () => invoke("open_settings_window"),
  hideOverlay: () => invoke("minimize_to_tray"),
  startDrag: () => getCurrentWindow().startDragging(),

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
  prealignRemoveFiles: () => invoke<PrealignStatus>("prealign_remove_files"),

  subscribe(on) {
    const pending = [
      on.track && listen<Track>("track-updated", (e) => on.track!(e.payload)),
      on.time && listen<TimeSample>("track-time-update", (e) => on.time!(e.payload)),
      on.playing && listen<boolean>("playback-state", (e) => on.playing!(e.payload)),
      on.clickThrough && listen("click-through-enabled", () => on.clickThrough!(true)),
      on.clickThrough && listen("click-through-disabled", () => on.clickThrough!(false)),
      on.lyrics &&
        listen<TimingUpdate>("lyrics-timing-updated", ({ payload: p }) =>
          on.lyrics!({ track: { title: p.title, artist: p.artist }, lines: p.lines, source: p.source }),
        ),
      on.prealign && listen<PrealignStatus>("prealign-status", (e) => on.prealign!(e.payload)),
    ].filter((p): p is Promise<() => void> => !!p);
    return () => pending.forEach((p) => void p.then((off) => off()));
  },
};

export const inTauri = isTauri();
export const backend: Backend = inTauri ? tauriBackend : mockBackend;
