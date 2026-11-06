/** Domain types shared by both windows. Mirrors the backend payloads. */

export interface Track {
  title: string;
  artist: string;
  album?: string | null;
  duration?: number | null;
  thumbnail?: string | null;
  /** YouTube Music's id for the track, when it plays there. */
  videoId?: string | null;
}

/**
 * Word timing inside a line (seconds, same naming as LyricLine). Sent only with
 * source "aligned". Without `duration` a word runs until the next one starts.
 */
export interface LyricWord {
  time: number;
  text: string;
  duration?: number;
}

/** `fetch_lyrics` payload line (seconds). */
export interface LyricLine {
  time: number;
  text: string;
  duration?: number | null;
  words?: LyricWord[];
}

/**
/**
 * Where a lyric list's timing came from. `*-estimate` is plain text spread
 * over the track: good for scrolling, never for highlighting. `aligned`: the
 * opt-in pre-aligner's timing, with per-word `words`.
 */
export type LyricsSource =
  | "lrclib"
  | "lrclib-estimate"
  | "qq"
  | "netease"
  | "kugou"
  | "ytmusic"
  | "ytmusic-estimate"
  | "aligned"
  /** Another recording's words (a cover's original, the studio version), spread over this one (an estimate). */
  | "cover-estimate"
  /** QQ / NetEase / Kugou had this recording's words untimed; spread over it (an estimate). */
  | "sources-estimate"
  /** A cover with its original's real timing (same length): lines light up, word alignment of the cover wins. */
  | "original-timing"
  /** The song has no words: `lines` is empty. */
  | "instrumental";

/** `prealign_status` / `prealign-status`: the word-level timing feature's state. */
export interface PrealignStatus {
  enabled: boolean;
  state: "off" | "downloading" | "ready" | "error";
  download: { file: "model" | "runtime" | "yt-dlp"; done: number; total: number | null } | null;
  error: string | null;
  /** Model, runtime and yt-dlp on disk. */
  diskBytes: number;
  /** "Title — Artist" being aligned right now. */
  workingOn: string | null;
  /** Songs aligned this session. */
  aligned: number;
}

/** `update_status` / `update-status`: the self-updater's state. */
export type UpdateStatus =
  | { state: "idle" | "checking" | "upToDate" }
  | { state: "available"; version: string; notes: string | null }
  | { state: "downloading"; version: string; done: number; total: number | null }
  | { state: "error"; message: string };

/** `fetch_lyrics_timed` payload. `pending`: a better timing may still arrive. */
export interface TimedLyrics {
  lines: LyricLine[];
  source: LyricsSource;
  pending: boolean;
}

/** A run of text with an optional reading (furigana). */
export interface Span {
  text: string;
  reading?: string | null;
}

export interface TimeSample {
  currentTime: number;
  duration: number;
  isPlaying: boolean;
  /** When the extension read `currentTime` (Date.now() ms, same machine). */
  measuredAt?: number | null;
}

export type PlaybackCommand = "play" | "pause" | "next" | "previous" | "seek";

/** The edge of the screen's work area the lyric strip hangs from; it grows away from it. */
export type Anchor = "top" | "bottom";

/** `strip_state`: locked = click-through; unlocked = Move mode. */
export interface StripView {
  locked: boolean;
  anchor: Anchor;
  /** No placement saved yet: the strip opened in Move mode, with a hint. */
  firstRun: boolean;
}

/** `strip-hover`: the cursor over the locked strip, and the reported rect under it. */
export interface StripHover {
  inside: boolean;
  target: string | null;
}

/** A rect the overlay reports (CSS px from the window's top-left); `clickable` ones take clicks while locked. */
export interface HitRect {
  id: string;
  x: number;
  y: number;
  w: number;
  h: number;
  clickable?: boolean;
}

/** `lyrics-progress`: the lookup moved on to a slower pass. */
export type LookupStage = "identify" | "web-search" | "sources" | "other-recording";
