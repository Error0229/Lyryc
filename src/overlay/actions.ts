/** Overlay side effects that are not tied to a component. */
import { backend } from "../services/ipc/backend";
import { friendlyError, isNotConfigured } from "../services/ipc/llmClient";
import { normalizeLines, textSignature } from "../lib/lyrics";
import type { LyricLine, LyricsSource, Track } from "../lib/types";
import { useSession } from "../state/session";
import { useSettings } from "../state/settings";
import { openSettings } from "../state/nav";
import { toast } from "../ui/Toast";
import { playhead } from "./playhead";
import { accepts, sameTrack } from "./upgrade";

let request = 0;

export function setTrack(track: Track) {
  if (sameTrack(useSession.getState().track, track)) return;
  useSession.setState({ track, lyrics: [], source: null, status: "loading", translations: {}, retimedAt: 0, refetching: false });
  playhead.reset();
  void loadLyrics(track);
}

async function loadLyrics(track: Track) {
  const id = ++request;
  try {
    const { extraSources, identifyWithSearch } = useSettings.getState();
    const r = await backend.fetchLyrics(track, extraSources, identifyWithSearch);
    if (id === request) applyLyrics(track, r.lines, r.source);
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
    if (r.source === source && textSignature(r.lines) === textSignature(lyrics)) {
      toast(
        r.source === "instrumental"
          ? { title: "Still no lyrics found", description: "The AI also says this track has no words." }
          : { title: "Found the same lyrics again", description: "No source has a different version of this track." },
      );
    }
    install(r.lines, r.source);
  } catch (e) {
    if (id !== request) return;
    const title = lyrics.length ? "No other lyrics found" : "Still no lyrics for this track";
    toast({ title, description: friendlyError(e), tone: "danger" });
    // Unmarking an instrumental that has no lyrics either: it is just missing.
    if (!lyrics.length) useSession.setState({ status: "missing", source: null });
  } finally {
    if (id === request) useSession.setState({ refetching: false });
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
export function applyLyrics(track: Pick<Track, "title" | "artist">, raw: LyricLine[], source: LyricsSource) {
  if (accepts(useSession.getState(), { track, source })) install(raw, source);
}

function install(raw: LyricLine[], source: LyricsSource) {
  const s = useSession.getState();
  const lines = normalizeLines(raw);
  useSession.setState({
    lyrics: lines,
    source: lines.length || source === "instrumental" ? source : null,
    status: lines.length ? "ready" : source === "instrumental" ? "instrumental" : "missing",
    retimedAt: s.lyrics.length && lines.length ? performance.now() : s.retimedAt,
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
