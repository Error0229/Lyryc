/**
 * Runtime state of the overlay window. Not persisted. Playback *time* is not
 * here on purpose: it lives in the playhead (src/overlay/playhead.ts) so time
 * updates never re-render React.
 */
import { create } from "zustand";
import type { LyricLine, Span, Track } from "../lib/types";

export type LyricsStatus = "idle" | "loading" | "ready" | "missing";

export interface Session {
  track: Track | null;
  lyrics: LyricLine[];
  status: LyricsStatus;
  /** Translated text keyed by translationKey(original). */
  translations: Record<string, string>;
  /** Furigana spans keyed by original line text. */
  furigana: Record<string, Span[]>;
  /** performance.now() of the last same-track lyric swap; 0 if none. */
  retimedAt: number;
  playing: boolean;
  clickThrough: boolean;
}

export const useSession = create<Session>()(() => ({
  track: null,
  lyrics: [],
  status: "idle",
  translations: {},
  furigana: {},
  retimedAt: 0,
  playing: false,
  clickThrough: false,
}));
