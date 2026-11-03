/** Overlay side effects that are not tied to a component. */
import { backend } from "../services/ipc/backend";
import { friendlyError, isNotConfigured } from "../services/ipc/llmClient";
import { normalizeLines } from "../lib/lyrics";
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
  useSession.setState({ track, lyrics: [], source: null, status: "loading", translations: {}, retimedAt: 0 });
  playhead.reset();
  void loadLyrics(track);
}

async function loadLyrics(track: Track) {
  const id = ++request;
  try {
    const r = await backend.fetchLyrics(track, useSettings.getState().extraSources);
    if (id === request) applyLyrics(track, r.lines, r.source);
  } catch {
    if (id === request) useSession.setState({ status: "missing" });
  }
}

/**
 * Install lyrics for the current track. A second call for the same track is a
 * timing swap: lines keep their identity and the fill eases to the new times.
 * Late arrivals for another track are dropped.
 */
export function applyLyrics(track: Pick<Track, "title" | "artist">, raw: LyricLine[], source: LyricsSource) {
  const s = useSession.getState();
  if (!accepts(s, { track, source })) return;
  const lines = normalizeLines(raw);
  useSession.setState({
    lyrics: lines,
    source: lines.length ? source : null,
    status: lines.length ? "ready" : "missing",
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
