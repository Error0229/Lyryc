/**
 * What the strip is doing, as one named state, and the status icon that says
 * so. Pure: derived from session state plus a few clocks the caller keeps
 * (the 2.5 s after an upgrade, the 6 s a "not found" message stays, ...).
 *
 * States: Idle → Looking up → Identifying → Other sources → Estimated / Line
 * timing (→ Syncing words) → Word timing; side branches Searching again, Not
 * found, Instrumental. Silence (no icon) means everything is fine.
 */
import type { LookupStage, LyricsSource } from "../lib/types";
import type { LyricsStatus } from "../state/session";
import { isEstimate } from "./upgrade";

export type Phase =
  | "idle" // nothing playing
  | "lookup" // cache, YouTube Music, LRCLIB
  | "identify" // the LLM names the song
  | "sources" // QQ / NetEase / Kugou, or another recording's words
  | "estimate" // words known, timing guessed
  | "line" // real line times
  | "word" // word timing
  | "refetch" // "Search again" running
  | "missing"
  | "instrumental";

export interface PhaseInput {
  hasTrack: boolean;
  status: LyricsStatus;
  stage: LookupStage | null;
  source: LyricsSource | null;
  refetching: boolean;
}

export function phaseOf(s: PhaseInput): Phase {
  if (!s.hasTrack) return "idle";
  if (s.refetching) return "refetch";
  switch (s.status) {
    case "missing":
    case "instrumental":
      return s.status;
    case "ready":
      if (isEstimate(s.source)) return "estimate";
      return s.source === "aligned" ? "word" : "line";
    default:
      if (s.stage === "identify" || s.stage === "web-search") return "identify";
      if (s.stage === "sources" || s.stage === "other-recording") return "sources";
      return "lookup";
  }
}

export type IconId =
  | "waiting"
  | "search"
  | "identify"
  | "sources"
  | "estimate"
  | "syncing"
  | "synced"
  | "refetch"
  | "same"
  | "nothing-else"
  | "no-lyrics"
  | "instrumental";

/** How the icon moves: breathing while working, bars while syncing, spinning while searching again; green when done. */
export type IconMotion = "breathe" | "bars" | "spin" | "done";

export interface StatusIcon {
  id: IconId;
  /** Its name, shown on hover: at most a few words. */
  tip: string;
  motion?: IconMotion;
}

export interface IconInput {
  phase: Phase;
  stage: LookupStage | null;
  source: LyricsSource | null;
  /** Better timing is on its way (an upgrade, or the pre-aligner working on this song). */
  syncing: boolean;
  /** Timing just improved (for 2.5 s). */
  justSynced: boolean;
  /** "Search again" just ended with the same lyrics, or nothing else (for 3 s). */
  outcome: "same" | "nothing-else" | null;
  /** The "not found" / "instrumental" message has had its moment. */
  quiet: boolean;
  /** Idle, and just launched or hovered: say what the strip waits for. */
  idleHint: boolean;
}

const SYNCING: StatusIcon = { id: "syncing", tip: "Syncing to the vocals", motion: "bars" };

export function statusIcon(i: IconInput): StatusIcon | null {
  if (i.outcome === "same") return { id: "same", tip: "Same lyrics found" };
  if (i.outcome === "nothing-else") return { id: "nothing-else", tip: "Nothing else found" };
  if (i.justSynced && (i.phase === "word" || i.phase === "line")) {
    return { id: "synced", tip: i.phase === "word" ? "Synced to the vocals" : "Synced lyrics found", motion: "done" };
  }
  switch (i.phase) {
    case "idle":
      return i.idleHint ? { id: "waiting", tip: "Waiting for YouTube Music" } : null;
    case "lookup":
      return { id: "search", tip: "Looking up lyrics", motion: "breathe" };
    case "identify":
      return { id: "identify", tip: i.stage === "web-search" ? "Searching the web" : "Identifying the song", motion: "breathe" };
    case "sources":
      return { id: "sources", tip: i.stage === "other-recording" ? "Finding the original's words" : "Trying other sources", motion: "breathe" };
    case "estimate":
      if (i.syncing) return SYNCING;
      return { id: "estimate", tip: i.source === "cover-estimate" ? "Original's words, estimated" : "Estimated timing" };
    case "line":
      return i.syncing ? SYNCING : null;
    case "refetch":
      return { id: "refetch", tip: "Searching again", motion: "spin" };
    case "missing":
      return i.quiet ? { id: "no-lyrics", tip: "No lyrics found" } : null;
    case "instrumental":
      return i.quiet ? { id: "instrumental", tip: "Instrumental" } : null;
    case "word":
      return null;
  }
}

/** How long "not found" / "instrumental" stay in words before only the icon is left. */
export const QUIET_AFTER_MS: Partial<Record<Phase, number>> = { missing: 6000, instrumental: 4000 };
