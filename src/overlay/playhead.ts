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

export const playhead = {
  sample(time: number, playing: boolean) {
    anchor = reconcile(anchor, time, playing, performance.now());
    kick();
  },
  /** playback-state arrives with every progress report; only act on change. */
  setPlaying(playing: boolean) {
    if (anchor.playing === playing) return;
    const now = performance.now();
    anchor = { media: clockTime(anchor, now), at: now, playing, correction: 0 };
    kick();
  },
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
