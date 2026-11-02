/**
 * Overlay colour presets. They only touch colour and effect fields, never
 * typography, so applying one keeps the user's font and size.
 */
import type { LyricStyle } from "../state/schema";

type PresetFields = Pick<
  LyricStyle,
  "textColor" | "activeColor" | "sungColor" | "upcomingColor" | "shadow" | "shadowColor" | "shadowBlur" | "glow" | "glowColor" | "background" | "backgroundOpacity"
>;

export interface OverlayPreset {
  id: string;
  name: string;
  style: PresetFields;
}

const base: PresetFields = {
  textColor: "#ffffff",
  activeColor: "#ffffff",
  sungColor: "#ffffff",
  upcomingColor: "#ffffff73",
  shadow: true,
  shadowColor: "#000000",
  shadowBlur: 16,
  glow: false,
  glowColor: "#ffffff",
  background: "#000000",
  backgroundOpacity: 0,
};

export const overlayPresets: OverlayPreset[] = [
  { id: "classic", name: "Classic", style: base },
  { id: "karaoke", name: "Karaoke", style: { ...base, activeColor: "#ffd166", sungColor: "#ffe7a8", upcomingColor: "#ffffffb3", shadowBlur: 18 } },
  { id: "glow", name: "Glow", style: { ...base, sungColor: "#dbe6ff", upcomingColor: "#ffffff59", glow: true, glowColor: "#8fb0ff", shadowBlur: 20 } },
  { id: "panel", name: "Panel", style: { ...base, upcomingColor: "#ffffff66", shadow: false, background: "#0e1015", backgroundOpacity: 72 } },
  {
    id: "paper",
    name: "Paper",
    style: { ...base, textColor: "#17191e", activeColor: "#17191e", sungColor: "#17191e", upcomingColor: "#17191e61", shadow: false, background: "#f6f5f1", backgroundOpacity: 90 },
  },
];

export function matchPreset(style: LyricStyle): string | null {
  const keys = Object.keys(base) as Array<keyof PresetFields>;
  return overlayPresets.find((p) => keys.every((k) => p.style[k] === style[k]))?.id ?? null;
}
