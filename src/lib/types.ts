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
  /** A cover: the original's words, spread over this recording (an estimate). */
  | "cover-estimate"
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
}

export type PlaybackCommand = "play" | "pause" | "next" | "previous" | "seek";
