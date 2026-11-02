/** Domain types shared by both windows. Mirrors the backend payloads. */

export interface Track {
  title: string;
  artist: string;
  album?: string | null;
  duration?: number | null;
  thumbnail?: string | null;
}

/**
 * Word timing inside a line. The backend does not emit these yet; this is the
 * shape the frontend expects when the timing stream adds them (seconds, same
 * naming as LyricLine).
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
