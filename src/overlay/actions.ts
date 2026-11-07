/** Overlay side effects that are not tied to a component. */
import { backend } from "../services/ipc/backend";
import { friendlyError, isNotConfigured } from "../services/ipc/llmClient";
import { normalizeLines, songCacheKey, textSignature } from "../lib/lyrics";
import type { LyricLine, LyricsSource, Track } from "../lib/types";
import { useSession } from "../state/session";
import { useSettings } from "../state/settings";
import { openSettings } from "../state/nav";
import { toast } from "../ui/Toast";
import { playhead } from "./playhead";
import { accepts, sameTrack } from "./upgrade";

let request = 0;

export function setTrack(track: Track) {
  const shown = useSession.getState();
  if (sameTrack(shown.track, track)) {
    // The player often sends the length and video id a moment after the title.
    // Keep them, and search again unless lyrics are already showing: a search
    // without them, finished or still running, skipped most of the sources.
    // (loadLyrics' request counter drops the stale one's answer.)
    if (!gainsDetail(shown.track!, track)) return;
    useSession.setState({ track });
    if (shown.status === "missing" || shown.status === "loading") {
      useSession.setState({ status: "loading" });
      void loadLyrics(track);
    }
    return;
  }
  useSession.setState({ track, lyrics: [], source: null, status: "loading", translations: {}, retimedAt: 0, refetching: false, stage: null, pending: false, aligning: false, outcome: null });
  playhead.reset();
  void loadLyrics(track);
}

export function gainsDetail(was: Track, now: Track): boolean {
  return (!was.duration && !!now.duration) || (!was.videoId && !!now.videoId);
}

async function loadLyrics(track: Track) {
  const id = ++request;
  try {
    const { extraSources, identifyWithSearch } = useSettings.getState();
    const r = await backend.fetchLyrics(track, extraSources, identifyWithSearch);
    if (id === request) applyLyrics(track, r.lines, r.source, r.pending);
  } catch {
    if (id === request) useSession.setState({ status: "missing" });
  }
}

/**
 * "Search again": the lyrics on screen are wrong (or missing). The backend
 * skips its cache and searches harder; whatever it finds replaces them, even
 * word timing, since the user asked. Nothing found keeps what is shown.
 */
export async function refetchLyrics() {
  const { track, lyrics, source, refetching } = useSession.getState();
  if (!track || refetching) return;
  const id = ++request;
  useSession.setState({ refetching: true });
  try {
    const { extraSources, identifyWithSearch } = useSettings.getState();
    const r = await backend.fetchLyrics(track, extraSources, identifyWithSearch, true);
    if (id !== request) return;
    if (r.source === source && textSignature(r.lines) === textSignature(lyrics)) sayOutcome("same");
    install(r.lines, r.source, r.pending);
  } catch {
    if (id !== request) return;
    // The strip says so itself: its icon, or the "No synced lyrics" message.
    if (lyrics.length) sayOutcome("nothing-else");
    // Unmarking an instrumental that has no lyrics either: it is just missing.
    else useSession.setState({ status: "missing", source: null });
  } finally {
    if (id === request) useSession.setState({ refetching: false });
  }
}

const cacheKey = (t: Pick<Track, "title" | "artist">) => songCacheKey(t.artist, t.title);

/**
 * The user edited this song's saved lyrics in Settings: show what is saved
 * now, whatever is showing (word timing included), and translate it again
 * from the cache. Other songs' edits are for when they play.
 */
export async function reloadSaved(song: Pick<Track, "title" | "artist">) {
  const { track } = useSession.getState();
  if (!track || cacheKey(track) !== cacheKey(song)) return;
  const id = ++request;
  useSession.setState((s) => ({ translations: {}, llmRevision: s.llmRevision + 1 }));
  try {
    const { extraSources, identifyWithSearch } = useSettings.getState();
    const r = await backend.fetchLyrics(track, extraSources, identifyWithSearch);
    if (id === request) install(r.lines, r.source, r.pending);
  } catch {
    if (id === request) useSession.setState({ lyrics: [], source: null, status: "missing" });
  }
}

/**
 * The user says this track has no words: show it so, and never look it up
 * again. Answers still on their way are dropped. "Search again" undoes it.
 */
export function markInstrumental() {
  const { track } = useSession.getState();
  if (!track) return;
  ++request;
  useSession.setState({ lyrics: [], source: "instrumental", status: "instrumental", translations: {}, refetching: false });
  backend.markInstrumental(track).catch((e) => toast({ title: "Couldn't save that", description: friendlyError(e), tone: "danger" }));
}

/**
 * Install lyrics for the current track. A second call for the same track is a
 * timing swap: lines keep their identity and the fill eases to the new times.
 * Late arrivals for another track are dropped.
 */
export function applyLyrics(track: Pick<Track, "title" | "artist">, raw: LyricLine[], source: LyricsSource, pending = false) {
  if (accepts(useSession.getState(), { track, source })) install(raw, source, pending);
}

let outcomeTimer = 0;

/** "Search again" changed nothing: the status icon says so for a moment. */
function sayOutcome(outcome: "same" | "nothing-else") {
  clearTimeout(outcomeTimer);
  useSession.setState({ outcome });
  outcomeTimer = window.setTimeout(() => useSession.setState({ outcome: null }), OUTCOME_MS);
}
const OUTCOME_MS = 3000;

function install(raw: LyricLine[], source: LyricsSource, pending: boolean) {
  const s = useSession.getState();
  const lines = normalizeLines(raw);
  useSession.setState({
    lyrics: lines,
    source: lines.length || source === "instrumental" ? source : null,
    status: lines.length ? "ready" : source === "instrumental" ? "instrumental" : "missing",
    retimedAt: s.lyrics.length && lines.length ? performance.now() : s.retimedAt,
    stage: null,
    pending,
  });
}

/** LLM failures: "not configured" becomes a call to action, the rest a plain error. */
export function reportLlmError(err: unknown, what: string) {
  if (isNotConfigured(err)) {
    toast({
      title: `${what} needs an AI provider`,
      description: friendlyError(err) || "Add an API key and pick a model.",
      action: { label: "Set up a provider", onClick: () => void openSettings("providers") },
      duration: 10_000,
    });
    return;
  }
  toast({ title: `${what} failed`, description: friendlyError(err), tone: "danger" });
}
