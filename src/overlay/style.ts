/**
 * Settings → CSS custom properties for the lyric stage. Pure, so the overlay
 * and the settings preview render identically and it can be unit tested.
 */
import type { CSSProperties } from "react";
import type { FuriganaPrefs, LyricStyle, TranslationPrefs } from "../state/schema";
import { withAlpha } from "../lib/color";
import { rowsEm, type Rows } from "./rows";

export interface StageOptions {
  lyric: LyricStyle;
  translation: TranslationPrefs;
  furigana: FuriganaPrefs;
  /** The rows the song reserves (see rows.ts): the stage is always this tall. */
  rows: Rows;
}

const ORIGIN = { left: "left", center: "center", right: "right" } as const;

export function textShadow(on: boolean, color: string, blur: number): string {
  if (!on || blur <= 0) return "none";
  // Centred, no offset copy: an offset shadow reads as a second, ghost glyph.
  return `0 0 ${Math.max(1, Math.round(blur / 4))}px ${withAlpha(color, 0.35)}, 0 0 ${blur}px ${withAlpha(color, 0.4)}`;
}

export function stageVars({ lyric, translation, furigana, rows }: StageOptions): CSSProperties {
  const lh = lyric.lineHeight;

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
    "--scrim": lyric.scrim,
    "--tr-scale": translation.scale,
    "--tr-color": translation.color,
    "--tr-opacity": translation.opacity,
    "--rt-scale": `${furigana.scale}em`,
    "--rt-color": furigana.color,
    "--rt-opacity": furigana.opacity,
    "--stage-min": `${rowsEm(rows, lh, furigana.scale, translation.scale)}em`,
  } as CSSProperties;
}
