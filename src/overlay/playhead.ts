/**
 * The one clock of the overlay. Backend time samples re-anchor it; a single
 * requestAnimationFrame loop runs only while playing and hands the
 * interpolated time to frame listeners. React never sees per-frame time.
 */
import { clockTime, reconcile, stopped, type Anchor } from "../lib/clock";

type Listener = (t: number) => void;

let anchor: Anchor = stopped;
let frame = 0;
const listeners = new Set<Listener>();

function tick(now: number) {
  frame = 0;
  const t = clockTime(anchor, now);
  listeners.forEach((l) => l(t));
  if (anchor.playing && listeners.size) frame = requestAnimationFrame(tick);
}

/** Make sure at least one more frame runs (and keeps running while playing). */
function kick() {
  if (!frame && listeners.size) frame = requestAnimationFrame(tick);
}

/** Diagnostic (user report: lyrics jumping back): the last clock inputs, dumped with each warning. */
const trail: string[] = [];
function note(entry: string) {
  trail.push(`${(performance.now() / 1000).toFixed(1)} ${entry}`);
  if (trail.length > 12) trail.shift();
}

export const playhead = {
  /** `age`: how much of `time` is the report's lateness correction (diagnostics only). */
  sample(time: number, playing: boolean, age = 0) {
    const now = performance.now();
    const before = clockTime(anchor, now);
    anchor = reconcile(anchor, time, playing, now);
    note(`sample ${time.toFixed(2)}${age ? ` (+${age.toFixed(1)} late)` : ""}${playing ? "" : " paused"}, clock was ${before.toFixed(2)}`);
    // Diagnostic: a snap is a seek, a track change, a pause, or time sources disagreeing.
    if (anchor.correction === 0 && Math.abs(before - time) > 0.4 && before > 0.5) {
      console.warn(`[clock] snapped ${time < before ? "back" : "forward"} ${before.toFixed(2)}s→${time.toFixed(2)}s${playing ? "" : " (paused)"} | ${trail.join(" ; ")}`);
    }
    kick();
  },
  /** playback-state arrives with every progress report; only act on change. */
  setPlaying(playing: boolean) {
    if (anchor.playing === playing) return;
    const now = performance.now();
    anchor = { media: clockTime(anchor, now), at: now, playing, correction: 0 };
    note(`${playing ? "play" : "pause"} at ${anchor.media.toFixed(2)}`);
    kick();
  },
  trail: () => trail.join(" ; "),
  reset() {
    anchor = stopped;
    kick();
  },
  now: () => clockTime(anchor, performance.now()),
  kick,
  onFrame(l: Listener): () => void {
    listeners.add(l);
    kick();
    return () => void listeners.delete(l);
  },
};
