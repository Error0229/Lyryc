/**
 * LEGACY COMPATIBILITY FILE
 */

import { useConfigStore } from './configStore';
import type { LyricsStyle } from './types';

export type { LyricsStyle };

// Style presets for getPresets()
const STYLE_PRESETS = [
  { name: 'Clean & Simple', style: { fontSize: 2.0, fontWeight: 'normal', fontFamily: 'Inter, system-ui' } },
  { name: 'Bold & Dynamic', style: { fontSize: 3.0, fontWeight: 'bold', fontFamily: 'Inter, system-ui' } },
  { name: 'Karaoke Style', style: { fontSize: 2.8, fontWeight: 'semibold', fontFamily: 'Inter, system-ui' } },
  { name: 'Neon Glow', style: { fontSize: 2.2, fontWeight: 'medium', fontFamily: 'Orbitron, monospace' } },
  { name: 'Minimal Focus', style: { fontSize: 1.8, fontWeight: 'normal', fontFamily: 'Inter, system-ui' } },
  { name: 'Ultra Compact', style: { fontSize: 0.8, fontWeight: 'normal', fontFamily: 'Inter, system-ui' } },
];

export const useLyricsStyleStore = () => {
  const style = useConfigStore((s) => s.lyricsStyle);
  const updateStyle = useConfigStore((s) => s.updateLyricsStyle);
  const applyPreset = useConfigStore((s) => s.applyStylePreset);
  const resetToDefault = useConfigStore((s) => s.resetAll);

  return {
    style,
    updateStyle,
    applyPreset,
    resetToDefault,
    getPresets: () => STYLE_PRESETS,
  };
};
