/** The always-on-top lyric strip. */
import { useEffect, useLayoutEffect, useMemo, useRef, useState, type PointerEvent, type ReactNode } from "react";
import { AnimatePresence, motion } from "motion/react";
import { fade, spring } from "../design/motion";
import { lineKeys } from "../lib/lyrics";
import { totalOffset, trackKey } from "../lib/offset";
import { backend } from "../services/ipc/backend";
import { useSession } from "../state/session";
import { useSettings } from "../state/settings";
import { Toaster } from "../ui/Toast";
import { Controls, UnlockButton, WidthHandles } from "./Controls";
import { useBackendEvents, useFurigana, useRows, useSettingsMirror, useStripHeight, useTranslations } from "./hooks";
import { PILL_OVERLAP_PX, PILL_PX, ROOM_PX, ROOM_X_PX } from "./rows";
import { LineView } from "./Line";
import { lineModel } from "./model";
import { phaseOf, QUIET_AFTER_MS, statusIcon, type Phase } from "./phase";
import { Stage, useActiveLine, useRetiming } from "./Stage";
import { Card, IdleSeal, StatusAlone, StatusIcon, useAfter, useFor } from "./Status";
import { stageVars } from "./style";
import { isEstimate } from "./upgrade";

const HOVER_REST_MS = 300;
const HOVER_LINGER_MS = 600;
/** After launch, an idle strip names what it waits for this long. */
const LAUNCH_HINT_MS = 5000;
/** The ✓ after better timing lands. */
const SYNCED_MS = 2500;
/** Paused this long, the lyrics step back. */
const PAUSE_DIM_MS = 2000;

/** What the lyric rows hold when there is no line to sing: decided by the phase. */
function shownWithoutLine(phase: Phase, quiet: boolean): "idle" | "card" | "message" | "none" {
  if (phase === "idle") return "idle";
  if (phase === "missing" || phase === "instrumental") return quiet ? "none" : "message";
  return "card"; // searching, searching again, or lyrics waiting for their first line
}

/** The pointer over the strip, as the DOM sees it (Move mode); forgotten on locking, when the DOM stops hearing about it. */
function useHover(locked: boolean) {
  const [hovered, setHovered] = useState(false);
  useEffect(() => {
    if (locked) setHovered(false);
  }, [locked]);
  return { hovered, onPointerEnter: () => setHovered(true), onPointerLeave: () => setHovered(false) };
}

/** `on`, once it has held for a moment; off a moment after it stops. Passing over the strip doesn't flash anything. */
function useSettled(on: boolean): boolean {
  const [settled, setSettled] = useState(false);
  useEffect(() => {
    const t = setTimeout(() => setSettled(on), on ? HOVER_REST_MS : HOVER_LINGER_MS);
    return () => clearTimeout(t);
  }, [on]);
  return settled;
}

/** Move mode ends with Esc too (unlocking focuses the strip). */
function useEscToLock(moving: boolean) {
  useEffect(() => {
    if (!moving) return;
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && void backend.setLocked(true);
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [moving]);
}

/** The pill's distance from the window edge on its side: it reaches PILL_OVERLAP_PX into the strip. */
const PILL_INSET = ROOM_PX + PILL_OVERLAP_PX - PILL_PX;

/** Dragging the strip moves it (the backend follows the cursor); controls keep their clicks. */
function dragFromEmpty(e: PointerEvent) {
  if (e.button !== 0 || (e.target as Element).closest("button, input, [role=toolbar], [data-handle]")) return;
  void backend.dragStrip();
}

/**
 * Centres its content across its parent on a whole device pixel. Flex
 * centring lands on .5 px whenever the two widths differ in parity (a 107 px
 * toolbar in a 1014 px window), and then every 1-2 px icon stroke is smeared
 * over two pixels.
 */
function Centred({ children }: { children: ReactNode }) {
  const ref = useRef<HTMLDivElement>(null);
  const [left, setLeft] = useState<number | null>(null);
  useLayoutEffect(() => {
    const el = ref.current;
    const parent = el?.parentElement;
    if (!el || !parent) return;
    const place = () => {
      const k = window.devicePixelRatio;
      setLeft(Math.round(((parent.clientWidth - el.getBoundingClientRect().width) / 2) * k) / k);
    };
    place();
    const ro = new ResizeObserver(place);
    ro.observe(el);
    ro.observe(parent);
    return () => ro.disconnect();
  }, []);
  return (
    <div ref={ref} data-pill className="pointer-events-auto absolute top-0" style={{ left: left ?? 0, visibility: left === null ? "hidden" : undefined }}>
      {children}
    </div>
  );
}

export function Overlay() {
  useBackendEvents();
  useSettingsMirror();
  useTranslations();

  const s = useSession();
  const cfg = useSettings();
  const stageRef = useRef<HTMLDivElement>(null);
  const hover = useHover(s.locked);

  const key = s.track ? trackKey(s.track.artist, s.track.title) : null;
  const offset = totalOffset(cfg.sync.globalOffset, cfg.sync.trackOffsets, key);
  const index = useActiveLine(stageRef, s.lyrics, offset);
  const furiganaOn = useFurigana(index);
  const retiming = useRetiming(s.retimedAt);
  const keys = useMemo(() => lineKeys(s.lyrics), [s.lyrics]);

  // Guessed timing must not drive a highlight: show estimated lines plain.
  const estimated = isEstimate(s.source);
  const highlight = cfg.wordHighlight && !estimated;
  const translations = cfg.translation.enabled ? s.translations : null;
  const rows = useRows();
  const model = useMemo(
    () =>
      lineModel({
        lines: s.lyrics,
        keys,
        index,
        wordHighlight: highlight,
        translations,
        display: cfg.translation.display,
        furigana: furiganaOn ? s.furigana : null,
      }),
    [s.lyrics, keys, index, highlight, translations, cfg.translation.display, furiganaOn, s.furigana],
  );
  const vars = useMemo(
    () => stageVars({ lyric: cfg.lyric, translation: cfg.translation, furigana: cfg.furigana, rows }),
    [cfg.lyric, cfg.translation, cfg.furigana, rows],
  );


  // Locked: click-through, and the backend tells us where the cursor is. Unlocked: Move mode.
  const moving = !s.locked;
  // Locked, only the backend's cursor poll knows: the window is click-through,
  // so the DOM misses the pointer leaving (after Lock, or off the Unlock pill)
  // and its own hover would stick. Unlocked, the DOM sees every move.
  const stripHovered = s.locked ? s.hover.inside : hover.hovered;
  const unlockShown = useSettled(s.locked && stripHovered) && s.locked;
  useEscToLock(moving);
  useStripHeight(rows);
  // The strip sits ROOM_PX in from the window's top and bottom; the pill on the
  // side away from the anchor (it swaps sides live while the strip is dragged).
  const bottom = s.anchor === "bottom";
  const box = { top: ROOM_PX, bottom: ROOM_PX, left: ROOM_X_PX, right: ROOM_X_PX };
  const slot = bottom ? { top: PILL_INSET } : { bottom: PILL_INSET };
  const tipSide = bottom ? "above" : "below";

  const phase = phaseOf({ hasTrack: !!s.track, status: s.status, stage: s.stage, source: s.source, refetching: s.refetching });
  const [launched, setLaunched] = useState(true);
  useEffect(() => void setTimeout(() => setLaunched(false), LAUNCH_HINT_MS), []);
  const quiet = useAfter(phase === "missing" || phase === "instrumental", QUIET_AFTER_MS[phase] ?? 0, key);
  const justSynced = useFor(s.retimedAt, SYNCED_MS);
  const dim = useAfter(!s.playing && !!model, PAUSE_DIM_MS) || (s.refetching && !!model);
  const icon = statusIcon({
    phase,
    stage: s.stage,
    source: s.source,
    syncing: s.pending || s.aligning,
    justSynced,
    outcome: s.outcome,
    quiet,
    idleHint: launched || stripHovered,
  });
  const empty = shownWithoutLine(phase, quiet);
  // The status icon goes with the content, never at the strip's border.
  const status = (
    <AnimatePresence>
      {!moving && icon && <StatusIcon key="status" icon={icon} hovered={s.hover.target === "status"} pinned={phase === "idle" && launched} />}
    </AnimatePresence>
  );
  const itemKey = model ? `line:${model.key}` : `${empty}:${key ?? ""}`;

  return (
    <div className="relative h-screen" style={vars} onPointerEnter={hover.onPointerEnter} onPointerLeave={hover.onPointerLeave}>
      {/* The strip: what shows. */}
      <div
        data-strip
        className={`absolute overflow-hidden rounded-lg bg-(--strip-bg) ${moving ? "cursor-move" : ""}`}
        style={box}
        onPointerDown={moving ? dragFromEmpty : undefined}
      >
        <motion.div
          aria-hidden
          className="pointer-events-none absolute inset-0 bg-black/25"
          initial={false}
          animate={{ opacity: moving ? 1 : 0 }}
          transition={fade("slow")}
        />
        <div className="relative flex h-full items-center px-6 py-2">
          {cfg.lyric.scrim > 0 && <div aria-hidden className="lyric-scrim" />}
          <div className="lyric-dim relative w-full" data-dim={dim || undefined}>
            <Stage stageRef={stageRef} itemKey={itemKey} scroll={rows.next && !!model} retiming={retiming} rubyUnder={cfg.furigana.mode === "under"}>
              {model ? (
                <LineView
                  tokens={model.tokens}
                  sub={model.sub}
                  plain={!highlight}
                  kana={cfg.furigana.mode === "replace"}
                  display={cfg.translation.display}
                  awaitSub={rows.translation}
                  next={model.next}
                  nextRow={rows.next}
                  status={icon && !moving ? status : null}
                />
              ) : empty === "idle" ? (
                <IdleSeal status={status} />
              ) : empty === "message" && s.track ? (
                <Card title={phase === "instrumental" ? "♪ Instrumental" : "No synced lyrics"} sub={`${s.track.title} — ${s.track.artist}`} status={status} />
              ) : empty === "card" && s.track ? (
                <Card title={s.track.title} sub={s.track.artist} status={status} />
              ) : (
                <StatusAlone status={status} />
              )}
            </Stage>
          </div>
        </div>
      </div>
      {/* Move mode's outline and width grips, on the strip's edge (outside its clip). */}
      <AnimatePresence>
        {moving && (
          <motion.div
            key="outline"
            className="pointer-events-none absolute"
            style={box}
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={fade("base")}
          >
            <div aria-hidden className="absolute inset-0 rounded-lg ring-2 ring-brand-bright/70 ring-inset" />
            <div className="pointer-events-auto">
              <WidthHandles />
            </div>
          </motion.div>
        )}
      </AnimatePresence>
      {/* The pill slot, full width so the pill centres without a transform: the
          Unlock pill while locked, the toolbar in Move mode. Its box is fixed
          (a pill appearing doesn't move it), so it only slides when the anchor
          flips; the hit rects are measured at rest. */}
      <motion.div
        layout="position"
        transition={spring.gentle}
        className="pointer-events-none absolute inset-x-0 z-(--z-controls)"
        style={{ ...slot, height: PILL_PX }}
      >
        <Centred>
          {moving ? <Controls tipSide={tipSide} /> : <AnimatePresence>{unlockShown && <UnlockButton tipSide={tipSide} />}</AnimatePresence>}
        </Centred>
      </motion.div>
      <Toaster placement="strip" />
    </div>
  );
}
