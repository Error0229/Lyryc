/**
 * The lyric stage: owns the --t variable and the line enter/exit transition.
 * React re-renders here only when the active line (or its content) changes.
 */
import { useEffect, useLayoutEffect, useRef, useState, type ReactNode, type RefObject } from "react";
import { AnimatePresence, motion } from "motion/react";
import { spring, fade } from "../design/motion";
import { activeLineIndex } from "../lib/timeline";
import type { LyricLine } from "../lib/types";
import { playhead } from "./playhead";
import "./lyric.css";

/** Write --t every frame; report the active line index only when it changes. */
export function useActiveLine(stage: RefObject<HTMLElement | null>, lines: readonly LyricLine[], offset: number): number {
  const [index, setIndex] = useState(-1);
  const live = useRef({ lines, offset, index: -1 });

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

export interface StageProps {
  stageRef: RefObject<HTMLDivElement | null>;
  /** Identity of what is shown; a new key animates the old line out. */
  itemKey: string;
  retiming: boolean;
  rubyUnder: boolean;
  children: ReactNode;
}

export function Stage({ stageRef, itemKey, retiming, rubyUnder, children }: StageProps) {
  return (
    <div
      ref={stageRef}
      className="lyric-stage grid min-h-(--stage-min) w-full items-center"
      data-retiming={retiming || undefined}
      data-ruby={rubyUnder ? "under" : undefined}
    >
      <AnimatePresence initial={false}>
        <motion.div
          key={itemKey}
          className="col-start-1 row-start-1 w-full"
          initial={{ opacity: 0, y: "0.4em", scale: 0.985 }}
          animate={{ opacity: 1, y: 0, scale: 1, transition: { ...spring.lyric, opacity: fade("slow") } }}
          exit={{ opacity: 0, y: "-0.35em", scale: 0.985, transition: fade("base") }}
        >
          {children}
        </motion.div>
      </AnimatePresence>
    </div>
  );
}
