/**
 * Renders one lyric line from tokens. Static between line changes: timing
 * lives in per-token CSS variables and progress comes from --t on the stage.
 */
import { memo, useLayoutEffect, useRef, type CSSProperties } from "react";
import type { Token } from "../lib/timeline";
import type { TranslationDisplay } from "../state/schema";

function Text({ tok, kana }: { tok: Token; kana: boolean }) {
  if (!tok.reading) return <>{tok.text}</>;
  if (kana) return <>{tok.reading}</>;
  return (
    <ruby>
      {tok.text}
      <rt>{tok.reading}</rt>
    </ruby>
  );
}

function TokenView({ tok, plain, kana }: { tok: Token; plain: boolean; kana: boolean }) {
  const style = {
    "--ws": tok.start,
    "--wl": Math.max(tok.end - tok.start, 0.001),
    "--wd": tok.done,
  } as CSSProperties;
  const text = <Text tok={tok} kana={kana} />;
  return (
    <span className="tok" style={style}>
      <span className="tok-base">{text}</span>
      {!plain && (
        <>
          <span className="tok-fill" aria-hidden>
            <span className="tok-fill-in">{text}</span>
          </span>
          <span className="tok-sung" aria-hidden>
            {text}
          </span>
        </>
      )}
    </span>
  );
}

/** Scale a nowrap line down to fit its box instead of wrapping (no resize per line). */
function useFitWidth(tokens: readonly Token[]) {
  const ref = useRef<HTMLSpanElement>(null);
  useLayoutEffect(() => {
    const el = ref.current;
    const box = el?.parentElement;
    if (!el || !box) return;
    const fit = () => el.style.setProperty("--fit", String(Math.min(1, box.clientWidth / Math.max(el.scrollWidth, 1))));
    fit();
    const ro = new ResizeObserver(fit);
    ro.observe(box);
    return () => ro.disconnect();
  }, [tokens]);
  return ref;
}

export interface LineViewProps {
  tokens: Token[];
  sub: string | null;
  plain: boolean;
  kana: boolean;
  display: TranslationDisplay;
}

export const LineView = memo(function LineView({ tokens, sub, plain, kana, display }: LineViewProps) {
  const ref = useFitWidth(tokens);
  const side = display === "side-by-side" && sub;
  return (
    <div className={side ? "flex items-baseline justify-center gap-[1em]" : "block"}>
      <div className={side ? "min-w-0 flex-1 text-right" : undefined}>
        <span ref={ref} className="lyric-line" data-plain={plain || undefined}>
          {tokens.map((tok, i) => (
            <TokenView key={i} tok={tok} plain={plain} kana={kana} />
          ))}
        </span>
      </div>
      {sub && <span className={side ? "lyric-translation min-w-0 flex-1 text-left" : "lyric-translation"}>{sub}</span>}
    </div>
  );
});
