import { create } from 'zustand';
import { persist } from 'zustand/middleware';

export interface LyricsStyle {
  // Font options
  fontFamily: string;
  fontSize: number; // in rem
  fontWeight: 'normal' | 'medium' | 'semibold' | 'bold';
  
  // Text colors
  textColor: string;
  highlightColor: string;
  pastWordColor: string;
  futureWordColor: string;
  
  // Background options
  backgroundColor: string;
  backgroundOpacity: number; // 0-100
  backgroundBlur: number; // 0-20
  
  // Text effects
  textShadow: boolean;
  textShadowColor: string;
  textShadowBlur: number;
  textGlow: boolean;
  textGlowColor: string;
  
  // Layout
  lineHeight: number; // 1.0-3.0
  letterSpacing: number; // -0.1 to 0.5 in em
  textAlign: 'left' | 'center' | 'right';
  
  // Animation
  animationIntensity: number; // 0-100
}

const defaultStyle: LyricsStyle = {
  // Font
  fontFamily: 'Inter, system-ui, sans-serif',
  fontSize: 2.5, // rem
  fontWeight: 'normal',
  
  // Text colors
  textColor: '#ffffff',
  highlightColor: '#ffffff',
  pastWordColor: '#93c5fd', // blue-300
  futureWordColor: '#ffffff60', // white/60
  
  // Background
  backgroundColor: '#000000',
  backgroundOpacity: 0,
  backgroundBlur: 0,
  
  // Text effects  
  textShadow: true,
  textShadowColor: '#000000',
  textShadowBlur: 20,
  textGlow: false,
  textGlowColor: '#ffffff',
  
  // Layout
  lineHeight: 1.5,
  letterSpacing: 0.05,
  textAlign: 'center',
  
  // Animation
  animationIntensity: 50,
};

interface LyricsStyleState {
  style: LyricsStyle;
  
  // Actions
  updateStyle: (updates: Partial<LyricsStyle>) => void;
  resetToDefault: () => void;
  
  // Presets
  applyPreset: (presetName: string) => void;
  getPresets: () => { name: string; style: LyricsStyle }[];
}

// Style presets
const stylePresets = [
  {
    name: 'Clean & Simple',
    style: {
      ...defaultStyle,
      fontSize: 2.0,
      fontWeight: 'normal' as const,
      textShadow: false,
      animationIntensity: 20,
    }
  },
  {
    name: 'Bold & Dynamic',
    style: {
      ...defaultStyle,
      fontSize: 3.0,
      fontWeight: 'bold' as const,
      textShadow: true,
      textShadowBlur: 30,
      animationIntensity: 80,
      textGlow: true,
      textGlowColor: '#3b82f6',
    }
  },
  {
    name: 'Karaoke Style',
    style: {
      ...defaultStyle,
      fontSize: 2.8,
      fontWeight: 'semibold' as const,
      highlightColor: '#ffd700',
      pastWordColor: '#ffeb3b',
      textShadow: true,
      textShadowColor: '#000000',
      textShadowBlur: 15,
      animationIntensity: 70,
      backgroundColor: '#1a1a1a',
      backgroundOpacity: 30,
      backgroundBlur: 5,
    }
  },
  {
    name: 'Neon Glow',
    style: {
      ...defaultStyle,
      fontSize: 2.2,
      fontFamily: 'Orbitron, monospace',
      fontWeight: 'medium' as const,
      textColor: '#00f5ff',
      highlightColor: '#39ff14',
      pastWordColor: '#ff0080',
      textGlow: true,
      textGlowColor: '#00f5ff',
      textShadow: false,
      animationIntensity: 90,
      backgroundColor: '#0a0a0a',
      backgroundOpacity: 50,
    }
  },
  {
    name: 'Minimal Focus',
    style: {
      ...defaultStyle,
      fontSize: 1.8,
      fontWeight: 'normal' as const,
      textColor: '#e5e7eb',
      highlightColor: '#ffffff',
      pastWordColor: '#9ca3af',
      futureWordColor: '#6b7280',
      textShadow: false,
      animationIntensity: 10,
      backgroundColor: '#1f2937',
      backgroundOpacity: 20,
      backgroundBlur: 10,
    }
  }
];

export const useLyricsStyleStore = create<LyricsStyleState>()(
  persist(
    (set, get) => ({
      style: defaultStyle,
      
      updateStyle: (updates) => {
        set(state => ({
          style: { ...state.style, ...updates }
        }));
      },
      
      resetToDefault: () => {
        set({ style: defaultStyle });
      },
      
      applyPreset: (presetName) => {
        const preset = stylePresets.find(p => p.name === presetName);
        if (preset) {
          set({ style: preset.style });
        }
      },
      
      getPresets: () => stylePresets,
    }),
    {
      name: 'lyryc-lyrics-style-store',
      version: 1,
    }
  )
);