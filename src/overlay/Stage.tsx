/**
 * The lyric stage: owns the --t variable and the line enter/exit transition.
 * React re-renders here only when the active line (or its content) changes.
 */
import { useEffect, useLayoutEffect, useRef, useState, type ReactNode, type RefObject } from "react";
import { AnimatePresence, motion, type Variants } from "motion/react";
import { duration, ease, spring, fade } from "../design/motion";
import { activeLineIndex } from "../lib/timeline";
import type { LyricLine } from "../lib/types";
import { playhead } from "./playhead";
import "./lyric.css";

/** Write --t every frame; report the active line index only when it changes. */
export function useActiveLine(stage: RefObject<HTMLElement | null>, lines: readonly LyricLine[], offset: number): number {
  const [index, setIndex] = useState(-1);
  const live = useRef({ lines, offset, index: -1, framed: lines });

  useLayoutEffect(() => {
    live.current.lines = lines;
    live.current.offset = offset;
    playhead.kick();
  }, [lines, offset]);

  useEffect(
    () =>
      playhead.onFrame((t) => {
        const s = live.current;
        const time = t + s.offset;
        stage.current?.style.setProperty("--t", time.toFixed(3));
        const i = activeLineIndex(s.lines, time);
        // Diagnostic (user report: lyrics jumping back and forth): the line
        // going backwards while the same lyrics play is never intended.
        if (i >= 0 && i < s.index && s.lines === s.framed) {
          console.warn(`[overlay] line went back ${s.index}→${i} at t=${time.toFixed(2)}s (line ${i} starts ${s.lines[i]?.time.toFixed(2)}s, offset ${s.offset}) | ${playhead.trail()}`);
        }
        s.framed = s.lines; // new lyrics may legitimately start lower
        if (i !== s.index) {
          s.index = i;
          setIndex(i);
        }
      }),
    [stage],
  );

  return index;
}

/** True for a moment after `stamp` changes, so the fill eases into new timing. */
export function useRetiming(stamp: number): boolean {
  const [on, setOn] = useState(false);
  useEffect(() => {
    if (!stamp) return;
    setOn(true);
    const id = setTimeout(() => setOn(false), 400);
    return () => clearTimeout(id);
  }, [stamp]);
  return on;
}

/** How far apart the lyric row and the next-line row are, px; the scroll moves by this. */
function useRowStep(item: RefObject<HTMLDivElement | null>, itemKey: string) {
  const step = useRef(0);
  useLayoutEffect(() => {
    const main = item.current?.querySelector(".lyric-main");
    const next = item.current?.querySelector(".lyric-next");
    if (main && next) step.current = next.getBoundingClientRect().top - main.getBoundingClientRect().top;
  }, [item, itemKey]);
  return step;
}

/** Even pace, not a snap: the scroll should read as the rows moving. */
const SCROLL = { duration: duration.slow, ease: ease.standard };

/**
 * A new line: either it fades up into place and the old one fades out, or
 * (`scroll`, "Show the next line") the stage scrolls by one row: everything
 * moves up by the row step, the old line and its translation leave at the top,
 * and the line that was the preview arrives in the lyric row (it grows from the
 * preview's size there: `.lyric-main` in lyric.css), its own translation and
 * preview coming in from below. Transform and opacity only.
 */
const lineMotion: Variants = {
  enter: ({ scroll, step }: Advance) => (scroll ? { opacity: 0, y: step } : { opacity: 0, y: "0.4em", scale: 0.985 }),
  show: ({ scroll }: Advance) => ({
    opacity: 1,
    y: 0,
    scale: 1,
    transition: scroll ? SCROLL : { ...spring.lyric, opacity: fade("slow") },
  }),
  leave: ({ scroll, step }: Advance) =>
    scroll
      ? { opacity: 0, y: -step, transition: { ...SCROLL, opacity: fade("base") } }
      : { opacity: 0, y: "-0.35em", scale: 0.985, transition: fade("base") },
};

interface Advance {
  scroll: boolean;
  step: number;
}

export interface StageProps {
  stageRef: RefObject<HTMLDivElement | null>;
  /** Identity of what is shown; a new key animates the old line out. */
  itemKey: string;
  /** Lines advance by scrolling one row (the next-line row is showing). */
  scroll?: boolean;
  retiming: boolean;
  rubyUnder: boolean;
  children: ReactNode;
}

export function Stage({ stageRef, itemKey, scroll = false, retiming, rubyUnder, children }: StageProps) {
  const item = useRef<HTMLDivElement>(null);
  const step = useRowStep(item, itemKey);
  const advance: Advance = { scroll: scroll && step.current > 0, step: step.current };
  return (
    <div
      ref={stageRef}
      className="lyric-stage grid min-h-(--stage-min) w-full items-end"
      data-retiming={retiming || undefined}
      data-ruby={rubyUnder ? "under" : undefined}
    >
      <AnimatePresence initial={false} custom={advance}>
        <motion.div
          key={itemKey}
          ref={item}
          className="col-start-1 row-start-1 w-full"
          data-advance={advance.scroll ? "scroll" : undefined}
          custom={advance}
          variants={lineMotion}
          initial="enter"
          animate="show"
          exit="leave"
        >
          {children}
        </motion.div>
      </AnimatePresence>
    </div>
  );
}
