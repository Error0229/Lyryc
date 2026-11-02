/**
 * Playback clock. The backend reports media time a few times a second; we
 * interpolate between reports with performance.now(). Small drift is absorbed
 * over CORRECTION_SECONDS so time never steps backwards; big jumps (seek,
 * pause, track change) snap.
 */

export interface Anchor {
  media: number; // media seconds at `at`
  at: number; // performance.now() ms
  playing: boolean;
  correction: number; // seconds still to absorb, spread over CORRECTION_SECONDS
}

export const SNAP_SECONDS = 0.4;
export const CORRECTION_SECONDS = 1;

export const stopped: Anchor = { media: 0, at: 0, playing: false, correction: 0 };

export function clockTime(a: Anchor, now: number): number {
  if (!a.playing) return a.media;
  const elapsed = Math.max(0, (now - a.at) / 1000);
  return a.media + elapsed + a.correction * Math.min(elapsed / CORRECTION_SECONDS, 1);
}

export function reconcile(prev: Anchor, time: number, playing: boolean, now: number): Anchor {
  const predicted = clockTime(prev, now);
  const drift = time - predicted;
  const keep = prev.playing && playing && Math.abs(drift) <= SNAP_SECONDS;
  return keep
    ? { media: predicted, at: now, playing, correction: drift }
    : { media: time, at: now, playing, correction: 0 };
}
