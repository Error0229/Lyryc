/**
 * The strip's one status voice: an icon per state, with the content (after the
 * line, heading the title card, or alone in the middle), never at the strip's
 * border; its name only on hover (decisions #9, #22). Plus what shows in the
 * lyric rows while there is no line: the title card, the not-found message,
 * the idle seal.
 */
import { useEffect, useRef, useState, type ReactNode } from "react";
import { AnimatePresence, motion } from "motion/react";
import { CaptionsOff, Check, CirclePlay, EqualApproximately, Globe, Music4, RefreshCw, Search, SearchX, Sparkles } from "lucide-react";
import seal from "../../src-tauri/icons/tray/48.png";
import { fade, spring } from "../design/motion";
import { useHitRect } from "./hits";
import type { IconId, StatusIcon as Icon } from "./phase";

export type TipSide = "above" | "below" | "left" | "right";

/**
 * Where the tip sits around its (relative) parent. Centred by flex boxes, not
 * by translate(-50%): a half-pixel transform leaves text and icons blurred.
 */
const TIP_PLACE: Record<TipSide, string> = {
  above: "bottom-full mb-1.5 w-0 justify-center",
  below: "top-full mt-1.5 w-0 justify-center",
  left: "inset-y-0 right-full mr-1.5 items-center",
  right: "inset-y-0 left-full ml-1.5 items-center",
};

/**
 * The shared tooltip: an element's name, on `side` of its parent. Above or
 * below, it centres on `x` (px from the parent's left; default the middle)
 * and slides there when the hovered element changes.
 */
export function NameTip({ text, side, x }: { text: string | null; side: TipSide; x?: number }) {
  const along = side === "above" || side === "below";
  return (
    <AnimatePresence>
      {text && (
        <motion.div
          role="tooltip"
          className={`pointer-events-none absolute z-(--z-tooltip) flex ${TIP_PLACE[side]}`}
          style={along && x === undefined ? { left: "50%" } : undefined}
          initial={{ opacity: 0, ...(along && x !== undefined && { left: x }) }}
          animate={{ opacity: 1, ...(along && x !== undefined && { left: x }) }}
          exit={{ opacity: 0 }}
          transition={{ ...fade("fast"), left: spring.snappy }}
        >
          <span className="rounded-md bg-glass-strong px-2 py-1 text-2xs font-medium whitespace-nowrap text-glass-fg shadow-2">{text}</span>
        </motion.div>
      )}
    </AnimatePresence>
  );
}

/** Four bars that rise and fall: word timing is being worked out. */
function Bars() {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" className="status-bars">
      <path d="M5 8v8" />
      <path d="M9.7 5v14" />
      <path d="M14.3 7v10" />
      <path d="M19 9.5v5" />
    </svg>
  );
}

const ICONS: Record<IconId, ReactNode> = {
  waiting: <CirclePlay />,
  search: <Search />,
  identify: <Sparkles />,
  sources: <Globe />,
  estimate: <EqualApproximately />,
  syncing: <Bars />,
  synced: <Check />,
  refetch: <RefreshCw />,
  same: <Check />,
  "nothing-else": <SearchX />,
  "no-lyrics": <CaptionsOff />,
  instrumental: <Music4 />,
};

/**
 * The icon, sized by the text it sits with (lyric.css .status-slot). Hovered
 * (`hovered`: the backend's cursor poll while locked, or the DOM) it shows its
 * name; `pinned` shows it regardless.
 */
export function StatusIcon({ icon, hovered, pinned }: { icon: Icon; hovered: boolean; pinned: boolean }) {
  const ref = useRef<HTMLDivElement>(null);
  const [domHover, setDomHover] = useState(false);
  useHitRect("status", ref, false, 4);
  return (
    <motion.div
      ref={ref}
      className="status-slot"
      data-motion={icon.motion}
      aria-label={icon.tip}
      role="status"
      onPointerEnter={() => setDomHover(true)}
      onPointerLeave={() => setDomHover(false)}
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={fade("slow")}
    >
      <span key={icon.id} className="status-icon">
        {ICONS[icon.id]}
      </span>
      <NameTip text={hovered || domHover || pinned ? icon.tip : null} side="right" />
    </motion.div>
  );
}

/** Two rows of words where the lyrics go: the title card while searching (its status icon ahead of the title), the not-found message. */
export function Card({ title, sub, status }: { title: string; sub?: string; status?: ReactNode }) {
  return (
    <div className="lyric-card">
      <span className="lyric-card-head">
        {status}
        <span className="lyric-card-title">{title}</span>
      </span>
      {sub && <span className="lyric-card-sub">{sub}</span>}
    </div>
  );
}

/** Idle: the faint seal where the lyrics will appear, with the status beside it while it names what it waits for. */
export function IdleSeal({ status }: { status?: ReactNode }) {
  return (
    <div className="status-alone mb-1">
      <img src={seal} alt="" aria-hidden className="size-6 rounded-sm opacity-30" draggable={false} />
      {status}
    </div>
  );
}

/** Nothing else to show (a song gone quiet: no lyrics, instrumental): the icon alone, in the middle. */
export function StatusAlone({ status }: { status: ReactNode }) {
  return <div className="status-alone">{status}</div>;
}

/** True for `ms` after `stamp` changes to a non-zero value. */
export function useFor(stamp: number, ms: number): boolean {
  const [on, setOn] = useState(false);
  useEffect(() => {
    if (!stamp) return;
    setOn(true);
    const t = setTimeout(() => setOn(false), ms);
    return () => clearTimeout(t);
  }, [stamp, ms]);
  return on;
}

/** True once `on` has held for `ms` (and `key` hasn't changed); false as soon as it stops. */
export function useAfter(on: boolean, ms: number, key: unknown = null): boolean {
  const [after, setAfter] = useState(false);
  useEffect(() => {
    setAfter(false);
    if (!on) return;
    const t = setTimeout(() => setAfter(true), ms);
    return () => clearTimeout(t);
  }, [on, ms, key]);
  return after;
}
