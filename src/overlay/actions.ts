/** Overlay side effects that are not tied to a component. */
import { backend } from "../services/ipc/backend";
import { friendlyError, isNotConfigured } from "../services/ipc/llmClient";
import { normalizeLines } from "../lib/lyrics";
import { trackKey } from "../lib/offset";
import type { LyricLine, Track } from "../lib/types";
import { useSession } from "../state/session";
import { openSettings } from "../state/nav";
import { toast } from "../ui/Toast";
import { playhead } from "./playhead";

const same = (a: Track | null, b: Track | null) => !!a && !!b && trackKey(a.artist, a.title) === trackKey(b.artist, b.title);

let request = 0;

export function setTrack(track: Track) {
  if (same(useSession.getState().track, track)) return;
  useSession.setState({ track, lyrics: [], status: "loading", translations: {}, retimedAt: 0 });
  playhead.reset();
  void loadLyrics(track);
}

async function loadLyrics(track: Track) {
  const id = ++request;
  try {
    const lines = await backend.fetchLyrics(track);
    if (id === request) applyLyrics(track, lines);
  } catch {
    if (id === request) useSession.setState({ status: "missing" });
  }
}

/**
 * Install lyrics for the current track. A second call for the same track is a
 * timing swap: lines keep their identity and the fill eases to the new times.
 */
export function applyLyrics(track: Track, raw: LyricLine[]) {
  const s = useSession.getState();
  if (!same(s.track, track)) return;
  const lines = normalizeLines(raw);
  useSession.setState({
    lyrics: lines,
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
