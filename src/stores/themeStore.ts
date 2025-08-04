import { create } from 'zustand';
import { persist } from 'zustand/middleware';

export interface ThemeColors {
  primary: string;
  secondary: string;
  accent: string;
  background: string;
  backgroundSecondary: string;
  text: string;
  textSecondary: string;
  textMuted: string;
  border: string;
  highlight: string;
  success: string;
  warning: string;
  error: string;
}

export interface ThemeAnimation {
  duration: number; // Animation duration multiplier
  easing: string; // CSS easing function
  enableParticles: boolean;
  enableBlur: boolean;
  enableGlow: boolean;
}

export interface ThemeTypography {
  fontFamily: string;
  fontSize: {
    sm: string;
    base: string;
    lg: string;
    xl: string;
    '2xl': string;
    '3xl': string;
  };
  fontWeight: {
    normal: number;
    medium: number;
    semibold: number;
    bold: number;
  };
  lineHeight: {
    tight: number;
    normal: number;
    relaxed: number;
  };
}

export interface Theme {
  id: string;
  name: string;
  description: string;
  colors: ThemeColors;
  animation: ThemeAnimation;
  typography: ThemeTypography;
  custom: boolean;
}

interface ThemeState {
  currentTheme: Theme;
  themes: Theme[];
  customTheme: Theme | null;
  
  // Actions
  setTheme: (themeId: string) => void;
  updateCustomTheme: (theme: Partial<Theme>) => void;
  resetToDefault: () => void;
  exportTheme: (themeId: string) => string;
  importTheme: (themeData: string) => boolean;
}

// Default themes
const defaultThemes: Theme[] = [
  {
    id: 'classic',
    name: 'Classic',
    description: 'Clean white background with dark text',
    custom: false,
    colors: {
      primary: '#3B82F6',
      secondary: '#6366F1',
      accent: '#8B5CF6',
      background: 'linear-gradient(135deg, #ffffff 0%, #f8fafc 100%)',
      backgroundSecondary: '#f1f5f9',
      text: '#1e293b',
      textSecondary: '#475569',
      textMuted: '#64748b',
      border: '#e2e8f0',
      highlight: '#fbbf24',
      success: '#10b981',
      warning: '#f59e0b',
      error: '#ef4444',
    },
    animation: {
      duration: 1.0,
      easing: 'cubic-bezier(0.4, 0, 0.2, 1)',
      enableParticles: false,
      enableBlur: true,
      enableGlow: false,
    },
    typography: {
      fontFamily: 'Inter, system-ui, sans-serif',
      fontSize: {
        sm: '0.875rem',
        base: '1rem',
        lg: '1.125rem',
        xl: '1.25rem',
        '2xl': '1.5rem',
        '3xl': '1.875rem',
      },
      fontWeight: {
        normal: 400,
        medium: 500,
        semibold: 600,
        bold: 700,
      },
      lineHeight: {
        tight: 1.25,
        normal: 1.5,
        relaxed: 1.75,
      },
    },
  },
  {
    id: 'dark',
    name: 'Dark Night',
    description: 'Deep dark theme with purple accents',
    custom: false,
    colors: {
      primary: '#8B5CF6',
      secondary: '#A855F7',
      accent: '#F59E0B',
      background: 'linear-gradient(135deg, #0f0f23 0%, #1a1a2e 50%, #16213e 100%)',
      backgroundSecondary: '#1e1e3f',
      text: '#f8fafc',
      textSecondary: '#cbd5e1',
      textMuted: '#94a3b8',
      border: '#334155',
      highlight: '#fbbf24',
      success: '#22c55e',
      warning: '#f59e0b',
      error: '#ef4444',
    },
    animation: {
      duration: 1.2,
      easing: 'cubic-bezier(0.25, 0.46, 0.45, 0.94)',
      enableParticles: true,
      enableBlur: true,
      enableGlow: true,
    },
    typography: {
      fontFamily: 'Poppins, system-ui, sans-serif',
      fontSize: {
        sm: '0.875rem',
        base: '1rem',
        lg: '1.125rem',
        xl: '1.25rem',
        '2xl': '1.5rem',
        '3xl': '1.875rem',
      },
      fontWeight: {
        normal: 400,
        medium: 500,
        semibold: 600,
        bold: 700,
      },
      lineHeight: {
        tight: 1.25,
        normal: 1.5,
        relaxed: 1.75,
      },
    },
  },
  {
    id: 'neon',
    name: 'Neon Glow',
    description: 'Cyberpunk-inspired with bright neon colors',
    custom: false,
    colors: {
      primary: '#00f5ff',
      secondary: '#ff0080',
      accent: '#39ff14',
      background: 'linear-gradient(135deg, #0a0a0a 0%, #1a0033 50%, #330066 100%)',
      backgroundSecondary: '#1a1a2e',
      text: '#00f5ff',
      textSecondary: '#ff0080',
      textMuted: '#888',
      border: '#00f5ff',
      highlight: '#39ff14',
      success: '#39ff14',
      warning: '#ffaa00',
      error: '#ff0040',
    },
    animation: {
      duration: 0.8,
      easing: 'cubic-bezier(0.68, -0.55, 0.265, 1.55)',
      enableParticles: true,
      enableBlur: false,
      enableGlow: true,
    },
    typography: {
      fontFamily: 'Orbitron, monospace',
      fontSize: {
        sm: '0.875rem',
        base: '1rem',
        lg: '1.125rem',
        xl: '1.25rem',
        '2xl': '1.5rem',
        '3xl': '1.875rem',
      },
      fontWeight: {
        normal: 400,
        medium: 500,
        semibold: 600,
        bold: 700,
      },
      lineHeight: {
        tight: 1.25,
        normal: 1.5,
        relaxed: 1.75,
      },
    },
  },
  {
    id: 'karaoke',
    name: 'Karaoke Gold',
    description: 'Classic karaoke styling with gold highlights',
    custom: false,
    colors: {
      primary: '#d4af37',
      secondary: '#b8860b',
      accent: '#ffd700',
      background: 'linear-gradient(135deg, #1a1a1a 0%, #2d1b69 50%, #11001e 100%)',
      backgroundSecondary: '#2a2a3a',
      text: '#ffffff',
      textSecondary: '#d4af37',
      textMuted: '#cccccc',
      border: '#d4af37',
      highlight: '#ffd700',
      success: '#32cd32',
      warning: '#ffa500',
      error: '#ff6b6b',
    },
    animation: {
      duration: 1.5,
      easing: 'cubic-bezier(0.175, 0.885, 0.32, 1.275)',
      enableParticles: true,
      enableBlur: true,
      enableGlow: true,
    },
    typography: {
      fontFamily: 'Nunito, system-ui, sans-serif',
      fontSize: {
        sm: '0.875rem',
        base: '1rem',
        lg: '1.125rem',
        xl: '1.25rem',
        '2xl': '1.5rem',
        '3xl': '2rem',
      },
      fontWeight: {
        normal: 400,
        medium: 500,
        semibold: 600,
        bold: 800,
      },
      lineHeight: {
        tight: 1.2,
        normal: 1.4,
        relaxed: 1.6,
      },
    },
  },
];

export const useThemeStore = create<ThemeState>()(
  persist(
    (set, get) => ({
      currentTheme: defaultThemes[1], // Default to 'dark' theme
      themes: defaultThemes,
      customTheme: null,

      setTheme: (themeId) => {
        const theme = get().themes.find(t => t.id === themeId);
        if (theme) {
          set({ currentTheme: theme });
          
          // Apply theme to document
          applyThemeToDocument(theme);
        }
      },

      updateCustomTheme: (updatedTheme) => {
        const { customTheme } = get();
        const newCustomTheme = {
          ...customTheme,
          ...updatedTheme,
          id: 'custom',
          name: 'Custom',
          custom: true,
        } as Theme;

        set({ 
          customTheme: newCustomTheme,
          currentTheme: newCustomTheme,
          themes: [
            ...get().themes.filter(t => t.id !== 'custom'),
            newCustomTheme
          ]
        });

        applyThemeToDocument(newCustomTheme);
      },

      resetToDefault: () => {
        const defaultTheme = defaultThemes[1]; // Dark theme
        set({ 
          currentTheme: defaultTheme,
          customTheme: null,
          themes: defaultThemes
        });
        applyThemeToDocument(defaultTheme);
      },

      exportTheme: (themeId) => {
        const theme = get().themes.find(t => t.id === themeId);
        if (theme) {
          return JSON.stringify(theme, null, 2);
        }
        return '';
      },

      importTheme: (themeData) => {
        try {
          const theme = JSON.parse(themeData) as Theme;
          
          // Validate theme structure
          if (!theme.id || !theme.name || !theme.colors || !theme.animation || !theme.typography) {
            return false;
          }

          // Add to themes list
          const importedTheme = {
            ...theme,
            id: `imported_${Date.now()}`,
            custom: true,
          };

          set({
            themes: [...get().themes, importedTheme],
            currentTheme: importedTheme
          });

          applyThemeToDocument(importedTheme);
          return true;
        } catch (error) {
          console.error('Failed to import theme:', error);
          return false;
        }
      },
    }),
    {
      name: 'lyryc-theme-store',
      version: 1,
    }
  )
);

// Apply theme to document root
function applyThemeToDocument(theme: Theme) {
  const root = document.documentElement;
  
  // Apply CSS custom properties
  root.style.setProperty('--color-primary', theme.colors.primary);
  root.style.setProperty('--color-secondary', theme.colors.secondary);
  root.style.setProperty('--color-accent', theme.colors.accent);
  root.style.setProperty('--color-background', theme.colors.background);
  root.style.setProperty('--color-background-secondary', theme.colors.backgroundSecondary);
  root.style.setProperty('--color-text', theme.colors.text);
  root.style.setProperty('--color-text-secondary', theme.colors.textSecondary);
  root.style.setProperty('--color-text-muted', theme.colors.textMuted);
  root.style.setProperty('--color-border', theme.colors.border);
  root.style.setProperty('--color-highlight', theme.colors.highlight);
  root.style.setProperty('--color-success', theme.colors.success);
  root.style.setProperty('--color-warning', theme.colors.warning);
  root.style.setProperty('--color-error', theme.colors.error);

  // Animation properties
  root.style.setProperty('--animation-duration', `${theme.animation.duration}s`);
  root.style.setProperty('--animation-easing', theme.animation.easing);

  // Typography properties
  root.style.setProperty('--font-family', theme.typography.fontFamily);
  root.style.setProperty('--font-size-sm', theme.typography.fontSize.sm);
  root.style.setProperty('--font-size-base', theme.typography.fontSize.base);
  root.style.setProperty('--font-size-lg', theme.typography.fontSize.lg);
  root.style.setProperty('--font-size-xl', theme.typography.fontSize.xl);
  root.style.setProperty('--font-size-2xl', theme.typography.fontSize['2xl']);
  root.style.setProperty('--font-size-3xl', theme.typography.fontSize['3xl']);

  // Load Google Fonts if needed
  loadThemeFont(theme.typography.fontFamily);

  // Add theme class to body
  document.body.className = document.body.className.replace(/theme-\w+/g, '');
  document.body.classList.add(`theme-${theme.id}`);
}

// Load Google Fonts dynamically
function loadThemeFont(fontFamily: string) {
  const fontName = fontFamily.split(',')[0].trim().replace(/['"]/g, '');
  
  // Common Google Fonts mapping
  const googleFonts: Record<string, string> = {
    'Poppins': 'Poppins:wght@300;400;500;600;700',
    'Orbitron': 'Orbitron:wght@400;500;600;700;800;900',
    'Nunito': 'Nunito:wght@300;400;500;600;700;800',
    'Inter': 'Inter:wght@300;400;500;600;700',
  };

  if (googleFonts[fontName]) {
    const linkId = `font-${fontName.toLowerCase()}`;
    
    // Check if already loaded
    if (!document.getElementById(linkId)) {
      const link = document.createElement('link');
      link.id = linkId;
      link.rel = 'stylesheet';
      link.href = `https://fonts.googleapis.com/css2?family=${googleFonts[fontName]}&display=swap`;
      document.head.appendChild(link);
    }
  }
}

// Initialize theme on app start
if (typeof window !== 'undefined') {
  const store = useThemeStore.getState();
  applyThemeToDocument(store.currentTheme);
}