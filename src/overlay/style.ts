/**
 * Settings → CSS custom properties for the lyric stage. Pure, so the overlay
 * and the settings preview render identically and it can be unit tested.
 */
import type { CSSProperties } from "react";
import type { FuriganaPrefs, LyricStyle, TranslationPrefs } from "../state/schema";
import { withAlpha } from "../lib/color";

export interface StageOptions {
  lyric: LyricStyle;
  translation: TranslationPrefs;
  furigana: FuriganaPrefs;
  /** Translation line is currently shown. */
  showTranslation: boolean;
}

const ORIGIN = { left: "left", center: "center", right: "right" } as const;

export function textShadow(on: boolean, color: string, blur: number): string {
  if (!on || blur <= 0) return "none";
  return `0 1px ${Math.round(blur / 4)}px ${withAlpha(color, 0.55)}, 0 0 ${blur}px ${withAlpha(color, 0.45)}`;
}

export function stageVars({ lyric, translation, furigana, showTranslation }: StageOptions): CSSProperties {
  const ruby = furigana.enabled && furigana.mode !== "replace";
  const stacked = showTranslation && translation.display === "bilingual";
  const lh = lyric.lineHeight;
  // Reserve room so the window height does not change line to line.
  const rtReserve = ruby ? furigana.scale * 1.1 : 0;
  const trReserve = stacked ? translation.scale * 1.3 + 0.15 : 0;

  return {
    "--lyric-font": lyric.fontFamily,
    "--lyric-size": `${lyric.fontSize}rem`,
    "--lyric-weight": lyric.fontWeight,
    "--lyric-lh": lh,
    "--lyric-ls": `${lyric.letterSpacing}em`,
    "--lyric-align": lyric.align,
    "--lyric-origin": ORIGIN[lyric.align],
    "--lyric-text": lyric.textColor,
    "--lyric-active": lyric.activeColor,
    "--lyric-sung": lyric.sungColor,
    "--lyric-upcoming": lyric.upcomingColor,
    "--lyric-shadow": textShadow(lyric.shadow, lyric.shadowColor, lyric.shadowBlur),
    "--lyric-glow": lyric.glow ? `0 0 ${Math.max(6, lyric.shadowBlur)}px ${withAlpha(lyric.glowColor, 0.7)}` : "none",
    "--strip-bg": withAlpha(lyric.background, lyric.backgroundOpacity / 100),
    "--tr-scale": translation.scale,
    "--tr-color": translation.color,
    "--tr-opacity": translation.opacity,
    "--rt-scale": `${furigana.scale}em`,
    "--rt-color": furigana.color,
    "--rt-opacity": furigana.opacity,
    "--stage-min": `${lh + rtReserve + trReserve}em`,
  } as CSSProperties;
}
