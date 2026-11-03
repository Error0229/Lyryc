/** The always-on-top lyric strip. */
import { useMemo, useRef, useState, type PointerEvent } from "react";
import { AnimatePresence, motion } from "motion/react";
import { fade } from "../design/motion";
import { lineKeys } from "../lib/lyrics";
import { totalOffset, trackKey } from "../lib/offset";
import { backend } from "../services/ipc/backend";
import { useSession, type Session } from "../state/session";
import { useSettings } from "../state/settings";
import { Toaster } from "../ui/Toast";
import { Controls } from "./Controls";
import { useBackendEvents, useFitWindow, useFurigana, useSettingsMirror, useTranslations } from "./hooks";
import { LineView } from "./Line";
import { lineModel } from "./model";
import { Stage, useActiveLine, useRetiming } from "./Stage";
import { stageVars } from "./style";
import { isEstimate } from "./upgrade";

const HOVER_LINGER_MS = 600;

/** Show controls on hover, and keep them a moment after the pointer leaves. */
function useHover() {
  const [hovered, setHovered] = useState(false);
  const timer = useRef(0);
  return {
    hovered,
    onPointerEnter: () => {
      clearTimeout(timer.current);
      setHovered(true);
    },
    onPointerLeave: () => {
      timer.current = window.setTimeout(() => setHovered(false), HOVER_LINGER_MS);
    },
  };
}

/** Dragging from empty space moves the window; controls keep their clicks. */
function dragFromEmpty(e: PointerEvent) {
  if (e.button !== 0 || (e.target as Element).closest("button, input, [role=toolbar]")) return;
  void backend.startDrag();
}

function statusText(s: Pick<Session, "track" | "status">): { title: string; hint?: string } {
  if (!s.track) return { title: "Play something in your browser", hint: "Lyrics appear here · Ctrl+Shift+D toggles click-through" };
  if (s.status === "loading") return { title: `Finding lyrics for “${s.track.title}”` };
  if (s.status === "missing") return { title: "No synced lyrics for this track", hint: `${s.track.title} — ${s.track.artist}` };
  if (s.status === "instrumental") return { title: "Instrumental ♪", hint: `${s.track.title} — ${s.track.artist}` };
  return { title: s.track.title, hint: s.track.artist };
}

/** A quiet corner note: estimated timing, or "Search again" at work. */
function Badge({ text, title }: { text: string; title: string }) {
  return (
    <motion.span
      key={text}
      className="pointer-events-none absolute right-3 bottom-1 text-2xs font-medium text-(--lyric-text) [text-shadow:var(--lyric-shadow)]"
      title={title}
      initial={{ opacity: 0 }}
      animate={{ opacity: 0.5 }}
      exit={{ opacity: 0 }}
      transition={fade("slow")}
    >
      {text}
    </motion.span>
  );
}

function Status({ title, hint, pulse }: { title: string; hint?: string; pulse: boolean }) {
  return (
    <div className="grid gap-1 px-2 text-[max(0.42em,13px)] [text-shadow:var(--lyric-shadow)]">
      <motion.span
        className="font-medium text-(--lyric-text)"
        animate={pulse ? { opacity: [0.55, 1, 0.55] } : { opacity: 0.85 }}
        transition={pulse ? { duration: 1.8, repeat: Infinity, ease: "easeInOut" } : fade()}
      >
        {title}
      </motion.span>
      {hint && <span className="text-[0.8em] text-(--lyric-text) opacity-55">{hint}</span>}
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
  const contentRef = useRef<HTMLDivElement>(null);
  const hover = useHover();

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
  const showTranslation = !!translations && Object.keys(translations).length > 0;
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
    () => stageVars({ lyric: cfg.lyric, translation: cfg.translation, furigana: cfg.furigana, showTranslation }),
    [cfg.lyric, cfg.translation, cfg.furigana, showTranslation],
  );

  useFitWindow(contentRef, showTranslation, furiganaOn);

  const status = statusText(s);
  const itemKey = model ? `line:${model.key}` : `status:${s.status}:${key ?? ""}`;
  const interactive = hover.hovered && !s.clickThrough;

  return (
    <div
      className="relative flex h-screen items-center overflow-hidden rounded-lg bg-(--strip-bg) px-6 py-2"
      style={vars}
      onPointerEnter={hover.onPointerEnter}
      onPointerLeave={hover.onPointerLeave}
      onPointerDown={dragFromEmpty}
    >
      <motion.div
        aria-hidden
        className="pointer-events-none absolute inset-0 rounded-lg border border-glass-line bg-black/25"
        initial={false}
        animate={{ opacity: interactive ? 1 : 0 }}
        transition={fade("slow")}
      />
      <div ref={contentRef} className="relative w-full">
        <Stage stageRef={stageRef} itemKey={itemKey} retiming={retiming} rubyUnder={cfg.furigana.mode === "under"}>
          {model ? (
            <LineView
              tokens={model.tokens}
              sub={model.sub}
              plain={!highlight}
              kana={cfg.furigana.mode === "replace"}
              display={cfg.translation.display}
            />
          ) : (
            <Status title={status.title} hint={status.hint} pulse={s.status === "loading"} />
          )}
        </Stage>
      </div>
      <AnimatePresence>
        {!interactive &&
          (s.refetching ? (
            <Badge text="searching again…" title="Looking for the right lyrics, skipping the cache" />
          ) : (
            estimated &&
            model &&
            (s.source === "cover-estimate" ? (
              <Badge text="≈ cover · estimated timing" title="A cover: the original song's words, with timing guessed for this recording" />
            ) : (
              <Badge text="≈ estimated timing" title="No synced timing for this track; lines advance on a guess" />
            ))
          ))}
      </AnimatePresence>
      <AnimatePresence>{interactive && <Controls />}</AnimatePresence>
      <Toaster placement="strip" />
    </div>
  );
}
