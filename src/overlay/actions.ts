/** Overlay side effects that are not tied to a component. */
import { backend } from "../services/ipc/backend";
import { friendlyError, isNotConfigured } from "../services/ipc/llmClient";
import { normalizeLines, songCacheKey } from "../lib/lyrics";
import type { LyricLine, LyricsSource, Track } from "../lib/types";
import { useSession } from "../state/session";
import { useSettings } from "../state/settings";
import { openSavedSong, openSettings } from "../state/nav";
import { toast } from "../ui/Toast";
import { playhead } from "./playhead";
import { foundMessage, notFoundMessage, notFoundOf } from "./searchAgain";
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
  useSession.setState({ track, lyrics: [], source: null, status: "loading", translations: {}, retimedAt: 0, refetching: false, stage: null, pending: false, aligning: false, searched: null });
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
 * never serves them for this song again and looks for others, so each press
 * shows something new or says there is nothing else. What it finds replaces
 * what is shown, even word timing, since the user asked. When nothing is
 * left, nothing is shown either: those lyrics are rejected, not saved. How it
 * ended is the status icon's name (`searched`): where the new lyrics are
 * from, or what could not be asked.
 */
export async function refetchLyrics() {
  const { track, refetching, status, lyrics } = useSession.getState();
  // While the first lookup runs there is nothing to reject yet: its answer has not been seen.
  if (!track || refetching || status === "loading") return;
  const id = ++request;
  useSession.setState({ refetching: true, stage: null, searched: null });
  try {
    const { extraSources, identifyWithSearch } = useSettings.getState();
    const r = await backend.fetchLyrics(track, extraSources, identifyWithSearch, true);
    if (id !== request) return;
    install(r.lines, r.source, r.pending);
    // The lyrics that were showing are rejected now.
    useSession.setState({ searched: { message: foundMessage(r.source), rejected: lyrics.length > 0, at: performance.now() } });
  } catch (e) {
    if (id !== request) return;
    const nf = notFoundOf(e);
    useSession.setState({
      lyrics: [],
      source: null,
      status: "missing",
      translations: {},
      pending: false,
      searched: { message: notFoundMessage(nf), rejected: nf.rejected, at: performance.now() },
    });
  } finally {
    if (id === request) useSession.setState({ refetching: false, stage: null });
  }
}

/**
 * The saved-lyrics editor on the song playing, where lyrics can be pasted
 * (and timed by listening): the no-lyrics icon's click. It opens saved songs:
 * one that nothing was saved for goes in as "not found" first.
 */
export async function pasteLyrics() {
  const { track } = useSession.getState();
  if (!track) return;
  try {
    const saved = await backend.savedSong(track.artist, track.title);
    if (!saved) await backend.savedSongChange(track.artist, track.title, { op: "mark", instrumental: false });
    await openSavedSong(track);
  } catch (e) {
    toast({ title: "Couldn't open the lyrics editor", description: friendlyError(e), tone: "danger" });
  }
}

/** Forget what "Search again" rejected for this song, and look it up as if for the first time. */
export async function startOver() {
  const { track } = useSession.getState();
  if (!track) return;
  ++request; // answers on their way are for the song as it was
  useSession.setState({ lyrics: [], source: null, status: "loading", translations: {}, refetching: false, stage: null, pending: false, searched: null });
  try {
    await backend.startOver(track);
  } catch (e) {
    toast({ title: "Couldn't start over", description: friendlyError(e), tone: "danger" });
  }
  if (sameTrack(useSession.getState().track, track)) void loadLyrics(track);
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
  // "Nothing else found" no longer holds: the user saved some.
  useSession.setState((s) => ({ translations: {}, llmRevision: s.llmRevision + 1, searched: null }));
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
  useSession.setState({ lyrics: [], source: "instrumental", status: "instrumental", translations: {}, refetching: false, stage: null, searched: null });
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
      actions: [{ label: "Set up a provider", onClick: () => void openSettings("providers") }],
      duration: 10_000,
    });
    return;
  }
  toast({ title: `${what} failed`, description: friendlyError(err), tone: "danger" });
}
