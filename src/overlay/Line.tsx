/**
 * Renders one lyric line from tokens. Static between line changes: timing
 * lives in per-token CSS variables and progress comes from --t on the stage.
 */
import { memo, useLayoutEffect, useRef, type CSSProperties, type ReactNode } from "react";
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
      {/* The lit copy: highlighted, then dimmed to the sung colour, as two fronts on one layer. */}
      {!plain && (
        <span className="tok-fill" aria-hidden>
          <span className="tok-fill-in">{text}</span>
        </span>
      )}
    </span>
  );
}

/** Room kept at each end of a fitted line for the status slot (lyric.css .status-after), in em. */
const STATUS_RESERVE_EM = 1;

/**
 * Scale a nowrap line down to fit its box instead of wrapping (no resize per
 * line). It measures the words alone and always keeps room for the status
 * slot at each end, so a status icon coming or going never rescales the line.
 */
function useFitWidth(tokens: readonly Token[]) {
  const ref = useRef<HTMLSpanElement>(null);
  useLayoutEffect(() => {
    const el = ref.current;
    const words = el?.firstElementChild as HTMLElement | null;
    const box = el?.parentElement;
    if (!el || !words || !box) return;
    const fit = () => {
      const reserve = 2 * STATUS_RESERVE_EM * parseFloat(getComputedStyle(el).fontSize);
      el.style.setProperty("--fit", String(Math.min(1, box.clientWidth / Math.max(words.offsetWidth + reserve, 1))));
    };
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
  /** The song has a translation row: hold it with a faint ··· until the translation arrives. */
  awaitSub?: boolean;
  /** The next line, for the preview row (null: none, e.g. under the last line). */
  next?: string | null;
  /** The song has a next-line row: hold it even when there is no next line. */
  nextRow?: boolean;
  /** The status icon, just after the line's last word (it scales with the line). */
  status?: ReactNode;
}

export const LineView = memo(function LineView({ tokens, sub, plain, kana, display, awaitSub = false, next = null, nextRow = false, status = null }: LineViewProps) {
  const ref = useFitWidth(tokens);
  const side = display === "side-by-side" && sub;
  return (
    <div className={side ? "flex items-baseline justify-center gap-[1em]" : "block"}>
      <div className={side ? "lyric-main min-w-0 flex-1 text-right" : "lyric-main"}>
        <span ref={ref} className="lyric-line" data-plain={plain || undefined}>
          <span className="lyric-words">
            {tokens.map((tok, i) => (
              <TokenView key={i} tok={tok} plain={plain} kana={kana} />
            ))}
          </span>
          {status && <span className="status-after">{status}</span>}
        </span>
      </div>
      {sub && <span className={side ? "lyric-translation min-w-0 flex-1 text-left" : "lyric-translation"}>{sub}</span>}
      {!sub && awaitSub && (
        <span className="lyric-translation" data-await aria-hidden>
          ···
        </span>
      )}
      {nextRow && (
        <span className="lyric-next" aria-hidden={!next}>
          {next || "\u00a0"}
        </span>
      )}
    </div>
  );
});
