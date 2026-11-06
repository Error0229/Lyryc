/** The strip's controls: the Move-mode toolbar, and the Unlock button a locked strip shows on hover. */
import { useRef, useState, type ComponentProps, type PointerEvent as ReactPointerEvent, type SyntheticEvent } from "react";
import { motion } from "motion/react";
import { ChevronLeft, CircleArrowUp, Ellipsis, EyeOff, Languages, Lock, LockOpen, Minus, Music4, Pause, Play, Plus, SearchCheck, Settings2, SkipBack, SkipForward } from "lucide-react";
import { spring, fade } from "../design/motion";
import { NUDGE_STEP, trackKey } from "../lib/offset";
import { backend } from "../services/ipc/backend";
import { useSession } from "../state/session";
import { settings, useSettings } from "../state/settings";
import { openSettings } from "../state/nav";
import { installUpdate, useUpdateStatus } from "../state/update";
import { IconButton } from "../ui/IconButton";
import { OffsetField } from "../ui/OffsetField";
import { markInstrumental, refetchLyrics } from "./actions";
import { useHitRect } from "./hits";
import { ROOM_X_PX } from "./rows";
import { NameTip } from "./Status";
import { playhead } from "./playhead";

const Divider = () => <span aria-hidden className="mx-1 h-4 w-px bg-glass-line" />;

/** A button in the strip's pill: an icon in a circle, named by the shared tip. */
const Btn = (props: Omit<ComponentProps<typeof IconButton>, "tooltip" | "round">) => <IconButton tone="glass" {...props} tooltip="external" round />;

/** The pill the toolbar and its buttons sit in: 4px around 28px circles, so the curves are concentric. */
const PILL = "flex h-9 items-center gap-0.5 rounded-full bg-glass-strong p-1 shadow-2 ring-1 ring-glass-line";

/** Which side of the pill its tip goes: beyond it, away from the strip. */
export type PillTipSide = "above" | "below";

function OffsetNudge({ track }: { track: { artist: string; title: string } }) {
  const key = trackKey(track.artist, track.title);
  const value = useSettings((s) => s.sync.trackOffsets[key] ?? 0);
  const nudge = (d: number) => settings.setTrackOffset(key, value + d);
  return (
    <div className="flex items-center">
      <Btn label="Lyrics later" icon={<Minus />} onClick={() => nudge(-NUDGE_STEP)} />
      <OffsetField tone="glass" label="Offset for this track" value={value} onChange={(v) => settings.setTrackOffset(key, v)} className="rounded-full" />
      <Btn label="Lyrics earlier" icon={<Plus />} onClick={() => nudge(NUDGE_STEP)} />
    </div>
  );
}

/** Only there when a new version is out: one click installs it and restarts. */
function UpdateButton() {
  const update = useUpdateStatus();
  if (update.state !== "available" && update.state !== "downloading") return null;
  const downloading = update.state === "downloading";
  return (
    <Btn
      label={downloading ? `Downloading ${update.version}…` : `Update to ${update.version}`}
      icon={<CircleArrowUp className="text-accent" />}
      disabled={downloading}
      className={downloading ? "animate-pulse" : undefined}
      onClick={installUpdate}
    />
  );
}

function togglePlay(playing: boolean) {
  // Optimistic: the extension confirms via playback-state shortly after.
  useSession.setState({ playing: !playing });
  playhead.setPlaying(!playing);
  void backend.playback(playing ? "pause" : "play");
}

/**
 * Every control is an icon; its name (its accessible label) shows in one
 * shared tip over the hovered or focused one: `x` is its centre, px from the
 * group's left edge.
 */
function useNameTip() {
  const [tip, setTip] = useState<{ text: string; x: number } | null>(null);
  const name = (e: SyntheticEvent<HTMLElement>) => {
    const el = (e.target as Element).closest("button[aria-label], input[aria-label]");
    const text = el?.getAttribute("aria-label");
    if (!el || !text) return setTip(null);
    const r = el.getBoundingClientRect();
    setTip({ text, x: Math.round(r.left + r.width / 2 - e.currentTarget.getBoundingClientRect().left) });
  };
  const clear = () => setTip(null);
  return { tip, clear, bind: { onPointerOver: name, onFocus: name, onPointerLeave: clear, onBlur: clear } };
}

/**
 * Move mode's toolbar: a pill straddling the strip's edge away from the
 * anchor, so it never covers the lyric. Lock, playback, the lyrics lookup and
 * Settings stay in the row (disabled, not gone, without a track); "More" swaps
 * the row in place for the offset, translation, update and Hide. A first run also names
 * the gesture, in the tip's place, until something is hovered.
 */
export function Controls({ tipSide }: { tipSide: PillTipSide }) {
  const names = useNameTip();
  const firstRun = useSession((s) => s.firstRun);
  const [more, setMore] = useState(false);
  const track = useSession((s) => s.track);
  const playing = useSession((s) => s.playing);
  const translating = useSettings((s) => s.translation.enabled);
  const refetching = useSession((s) => s.refetching);
  const instrumental = useSession((s) => s.status === "instrumental");
  const showMore = (on: boolean) => () => {
    setMore(on);
    names.clear();
  };

  return (
    <div className="relative">
      <NameTip text={names.tip?.text ?? (firstRun ? "Drag to move · Esc to lock" : null)} x={names.tip?.x} side={tipSide} />
      <div role="toolbar" aria-label="Lyryc controls" className={PILL} {...names.bind}>
        {more ? (
          <>
            <Btn label="Back" icon={<ChevronLeft />} onClick={showMore(false)} />
            <Divider />
            {track && (
              <>
                <OffsetNudge track={track} />
                <Divider />
              </>
            )}
            <Btn
              label={translating ? "Hide translation" : "Show translation"}
              aria-pressed={translating}
              icon={<Languages />}
              onClick={() => settings.translation({ enabled: !translating })}
            />
            <UpdateButton />
            <Btn label="Hide · Ctrl+Shift+M" icon={<EyeOff />} onClick={() => void backend.hideOverlay()} />
          </>
        ) : (
          <>
            <Btn tone="brand" label="Lock · Esc" icon={<Lock />} onClick={() => void backend.setLocked(true)} />
            <Divider />
            {/* Always here; without a track they are disabled, not gone. */}
            <Btn label="Previous" icon={<SkipBack />} disabled={!track} onClick={() => void backend.playback("previous")} />
            <Btn label={playing ? "Pause" : "Play"} icon={playing ? <Pause /> : <Play />} disabled={!track} onClick={() => togglePlay(playing)} />
            <Btn label="Next" icon={<SkipForward />} disabled={!track} onClick={() => void backend.playback("next")} />
            <Divider />
            <Btn
              label={refetching ? "Searching again…" : "Search lyrics again"}
              icon={<SearchCheck />}
              disabled={!track || refetching}
              className={refetching ? "animate-pulse" : undefined}
              onClick={() => void refetchLyrics()}
            />
            <Btn
              label={instrumental ? "Not instrumental" : "Mark instrumental"}
              aria-pressed={instrumental}
              icon={<Music4 />}
              disabled={!track || refetching}
              onClick={() => (instrumental ? void refetchLyrics() : markInstrumental())}
            />
            <Divider />
            <Btn label="Settings" icon={<Settings2 />} onClick={() => void openSettings()} />
            <Btn label="More" icon={<Ellipsis />} onClick={showMore(true)} />
          </>
        )}
      </div>
    </div>
  );
}

/**
 * All a locked strip shows on hover: the way into Move mode, a brand pill in
 * the spot the toolbar takes once unlocked. It reports its rect, so the
 * backend lets just this spot take the click; the rest stays click-through.
 */
export function UnlockButton({ tipSide }: { tipSide: PillTipSide }) {
  const ref = useRef<HTMLDivElement>(null);
  const names = useNameTip();
  useHitRect("unlock", ref, true, 4); // padded: it is still scaling in
  return (
    <motion.div
      ref={ref}
      className="relative"
      initial={{ opacity: 0, scale: 0.96 }}
      animate={{ opacity: 1, scale: 1, transition: spring.gentle }}
      exit={{ opacity: 0, transition: fade("fast") }}
    >
      <NameTip text={names.tip?.text ?? null} side={tipSide} />
      <button
        type="button"
        aria-label="Unlock · Ctrl+Shift+D"
        onClick={() => void backend.setLocked(false)}
        {...names.bind}
        className="flex h-9 items-center rounded-full bg-brand px-4 text-brand-fg shadow-2 ring-1 ring-black/25 transition-colors duration-(--dur-fast) hover:bg-brand-hover active:bg-brand-pressed [&_svg]:size-4"
      >
        <LockOpen />
      </button>
    </motion.div>
  );
}

/**
 * Move mode's width grips, one at each end. Dragging one widens or narrows the
 * strip about its centre, so a centred strip stays centred; letting go saves it.
 */
export function WidthHandles() {
  const drag = (side: -1 | 1) => (e: ReactPointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.preventDefault();
    const grip = e.currentTarget;
    grip.setPointerCapture(e.pointerId);
    const x0 = e.screenX;
    const w0 = window.innerWidth - 2 * ROOM_X_PX; // the strip's width; the window adds its end room
    let width = w0;
    let frame = 0;
    const move = (ev: PointerEvent) => {
      width = Math.round(w0 + side * 2 * (ev.screenX - x0));
      frame ||= requestAnimationFrame(() => {
        frame = 0;
        void backend.setWidth(width, false);
      });
    };
    const done = new AbortController();
    const up = () => {
      done.abort();
      cancelAnimationFrame(frame);
      void backend.setWidth(width, true);
    };
    grip.addEventListener("pointermove", move, { signal: done.signal });
    grip.addEventListener("pointerup", up, { signal: done.signal });
    grip.addEventListener("pointercancel", up, { signal: done.signal });
  };
  // Centred on the ring (2 px, inset): 6 px wide from -2 px, so 2 px either side
  // of its middle line. At most 40 px tall and always 12 px short of the strip's
  // top and bottom, so the line visibly runs on above and below it. No transforms.
  const grip = "absolute inset-y-0 z-(--z-controls) my-auto h-[min(2.5rem,calc(100%-1.5rem))] w-1.5 cursor-ew-resize rounded-full bg-brand-bright shadow-1 hover:bg-brand-fg";
  return (
    <>
      <div data-handle aria-hidden className={`${grip} -left-0.5`} onPointerDown={drag(-1)} />
      <div data-handle aria-hidden className={`${grip} -right-0.5`} onPointerDown={drag(1)} />
    </>
  );
}
