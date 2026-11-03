/**
 * Live overlay preview: the real LineView and stage variables, driven by a
 * small looping clock instead of the playhead.
 */
import { useEffect, useMemo, useRef } from "react";
import { tokenize } from "../lib/timeline";
import type { LyricLine, Span } from "../lib/types";
import { LineView } from "../overlay/Line";
import { stageVars } from "../overlay/style";
import { useSettings } from "../state/settings";
import "../overlay/lyric.css";

const samples: Record<"en" | "ja", { line: LyricLine; spans?: Span[]; translation: string }> = {
  en: {
    line: {
      time: 0,
      duration: 4,
      text: "Hold the light a little longer",
      words: ["Hold", "the", "light", "a", "little", "longer"].map((text, i) => ({ time: 0.3 + i * 0.55, duration: 0.5, text })),
    },
    translation: "Sostén la luz un poco más",
  },
  ja: {
    line: {
      time: 0,
      duration: 4,
      text: "夜明けの駅で君を待つ",
      words: ["夜明けの", "駅で", "君を", "待つ"].map((text, i) => ({ time: 0.3 + i * 0.8, duration: 0.75, text })),
    },
    spans: [
      { text: "夜明", reading: "よあ" },
      { text: "けの" },
      { text: "駅", reading: "えき" },
      { text: "で" },
      { text: "君", reading: "きみ" },
      { text: "を" },
      { text: "待", reading: "ま" },
      { text: "つ" },
    ],
    translation: "Waiting for you at the station at dawn",
  },
};

const LOOP_SECONDS = 4.6;

/** Loop --t over the sample line while mounted. */
function useLoop(ref: React.RefObject<HTMLElement | null>) {
  useEffect(() => {
    const start = performance.now();
    let frame = requestAnimationFrame(function tick(now) {
      ref.current?.style.setProperty("--t", (((now - start) / 1000) % LOOP_SECONDS).toFixed(3));
      frame = requestAnimationFrame(tick);
    });
    return () => cancelAnimationFrame(frame);
  }, [ref]);
}

export function Preview({ sample = "en", forceTranslation = false }: { sample?: "en" | "ja"; forceTranslation?: boolean }) {
  const { lyric, translation, furigana, wordHighlight } = useSettings();
  const ref = useRef<HTMLDivElement>(null);
  useLoop(ref);

  const s = samples[sample];
  const showTranslation = translation.enabled || forceTranslation;
  const ruby = sample === "ja" && furigana.enabled;
  const translatedOnly = showTranslation && translation.display === "translated";
  const tokens = useMemo(
    () =>
      translatedOnly
        ? tokenize(s.translation, 0, 4)
        : tokenize(s.line.text, 0, 4, wordHighlight ? s.line.words : undefined, ruby ? s.spans : undefined),
    [s, translatedOnly, wordHighlight, ruby],
  );
  const vars = stageVars({ lyric, translation, furigana, showTranslation });

  return (
    <div
      aria-label="Overlay preview"
      className="relative overflow-hidden rounded-lg bg-[#1f2228] bg-[radial-gradient(circle_at_20%_20%,#3a3f4a_0,transparent_55%),radial-gradient(circle_at_85%_80%,#2c3340_0,transparent_50%)] p-3 shadow-1"
    >
      <div className="flex min-h-24 items-center rounded-md bg-(--strip-bg) px-4 py-2" style={vars}>
        <div
          ref={ref}
          className="lyric-stage grid min-h-(--stage-min) w-full items-center"
          style={{ fontSize: "min(var(--lyric-size), 2.25rem)" }}
          data-ruby={furigana.mode === "under" ? "under" : undefined}>
          <LineView
            tokens={tokens}
            sub={showTranslation && !translatedOnly ? s.translation : null}
            plain={!wordHighlight}
            kana={furigana.mode === "replace"}
            display={translation.display}
          />
        </div>
      </div>
    </div>
  );
}
