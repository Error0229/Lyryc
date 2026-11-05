/**
 * Runtime state of the overlay window. Not persisted. Playback *time* is not
 * here on purpose: it lives in the playhead (src/overlay/playhead.ts) so time
 * updates never re-render React.
 */
import { create } from "zustand";
import type { Anchor, LookupStage, LyricLine, LyricsSource, Span, StripHover, Track } from "../lib/types";

export type LyricsStatus = "idle" | "loading" | "ready" | "missing" | "instrumental";

export interface Session {
  track: Track | null;
  lyrics: LyricLine[];
  /** Where the current lyrics' timing came from; null while none are shown. */
  source: LyricsSource | null;
  status: LyricsStatus;
  /** Translated text keyed by translationKey(original). */
  translations: Record<string, string>;
  /** Furigana spans keyed by original line text. */
  furigana: Record<string, Span[]>;
  /** performance.now() of the last same-track lyric swap; 0 if none. */
  retimedAt: number;
  /** "Search again" is running for this track. */
  refetching: boolean;
  /** The lookup's slower pass right now (`lyrics-progress`); null before one starts. */
  stage: LookupStage | null;
  /** Better timing may still arrive for these lyrics (`TimedLyrics.pending`). */
  pending: boolean;
  /** The pre-aligner is working on this song right now (`prealign-status`). */
  aligning: boolean;
  /** How "Search again" just ended, when it changed nothing (shown for a moment). */
  outcome: "same" | "nothing-else" | null;
  /** Bumped when AI providers or routes change: a failed translation retries. */
  llmRevision: number;
  playing: boolean;
  /** Locked = click-through; unlocked = Move mode (toolbar, draggable). */
  locked: boolean;
  anchor: Anchor;
  /** Opened in Move mode because nothing was ever placed: show the hint. */
  firstRun: boolean;
  /** The cursor over the strip while it is locked (the backend follows it). */
  hover: StripHover;
}

export const useSession = create<Session>()(() => ({
  track: null,
  lyrics: [],
  source: null,
  status: "idle",
  translations: {},
  furigana: {},
  retimedAt: 0,
  refetching: false,
  stage: null,
  pending: false,
  aligning: false,
  outcome: null,
  llmRevision: 0,
  playing: false,
  locked: true,
  anchor: "bottom",
  firstRun: false,
  hover: { inside: false, target: null },
}));
