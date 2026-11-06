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
/**
 * Running *ahead* of a sample by up to this much is absorbed by slowing down,
 * never by snapping back: a sample that arrived late (the extension's report
 * travels through the browser, the socket and the backend) would otherwise
 * pull the lyrics back across a line boundary and forward again.
 */
export const ABSORB_BEHIND_SECONDS = 1.5;
/**
 * Trust a sample's measured-at stamp up to this age. Reports can reach the
 * overlay seconds late (a throttled background tab, the extension's service
 * worker waking up) and in bursts; their stamp says by how much.
 */
export const MAX_SAMPLE_AGE_SECONDS = 120;

/** Corrections spread over at least CORRECTION_SECONDS, and slow enough that
 * the clock keeps moving forward at half speed or more. */
const correctionWindow = (correction: number) => Math.max(CORRECTION_SECONDS, 2 * Math.abs(correction));

export const stopped: Anchor = { media: 0, at: 0, playing: false, correction: 0 };

export function clockTime(a: Anchor, now: number): number {
  if (!a.playing) return a.media;
  const elapsed = Math.max(0, (now - a.at) / 1000);
  return a.media + elapsed + a.correction * Math.min(elapsed / correctionWindow(a.correction), 1);
}

export function reconcile(prev: Anchor, time: number, playing: boolean, now: number): Anchor {
  const predicted = clockTime(prev, now);
  const drift = time - predicted;
  const keep = prev.playing && playing && drift <= SNAP_SECONDS && drift >= -ABSORB_BEHIND_SECONDS;
  return keep
    ? { media: predicted, at: now, playing, correction: drift }
    : { media: time, at: now, playing, correction: 0 };
}

/**
 * How long ago (seconds) a playing sample was measured, to add to its media
 * time. Only a stamp that moved forward since the previous sample's counts:
 * older extension builds repeat the track's detection time in every report,
 * which says nothing about this sample.
 */
export function sampleAge(measuredAt: number | null | undefined, previous: number | null | undefined, nowMs: number): number {
  if (!measuredAt || (previous && measuredAt <= previous)) return 0;
  const age = (nowMs - measuredAt) / 1000;
  return age > 0 && age <= MAX_SAMPLE_AGE_SECONDS ? age : 0;
}

/**
 * Whether a report repeats or predates one already used. Once stamps have
 * been seen to advance (a current extension), such a report is a re-send of
 * old data (the background script resends the last track when it reconnects
 * or a track is detected) and must not move the clock: seen live, a re-sent
 * 59.96 s pulled a clock at 93.6 s back by 34 s. Older extensions repeat one
 * stamp forever; their reports all count.
 */
export function isResend(measuredAt: number | null | undefined, previous: number | null | undefined, advancing: boolean): boolean {
  return !!(advancing && measuredAt && previous && measuredAt <= previous);
}
