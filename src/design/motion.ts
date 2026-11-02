/**
 * Motion tokens for JS-driven animation (motion/react). Mirrors the --dur-* and
 * --ease-* CSS variables so CSS and JS animations share one vocabulary.
 * Rule: animate transform and opacity only.
 */
import type { Transition } from "motion/react";

/** Seconds, as motion expects. */
export const duration = {
  instant: 0.08,
  fast: 0.14,
  base: 0.2,
  slow: 0.32,
  line: 0.42,
} as const;

export const ease = {
  standard: [0.2, 0, 0, 1],
  out: [0.16, 1, 0.3, 1],
  in: [0.4, 0, 1, 1],
} as const satisfies Record<string, readonly [number, number, number, number]>;

/** Named springs. Pick by intent, never tune inline. */
export const spring = {
  /** Small UI affordances: switches, segmented thumbs, icon presses. */
  snappy: { type: "spring", stiffness: 520, damping: 38, mass: 0.7 },
  /** Panels and controls appearing. */
  gentle: { type: "spring", stiffness: 300, damping: 32, mass: 0.9 },
  /** Lyric lines: soft settle, no overshoot you can see. */
  lyric: { type: "spring", stiffness: 190, damping: 28, mass: 1 },
} as const satisfies Record<string, Transition>;

export const fade = (d: keyof typeof duration = "base"): Transition => ({
  duration: duration[d],
  ease: ease.standard,
});
