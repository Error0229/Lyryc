import React, { useState, useMemo } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { useLyricsStyleStore } from '../stores/lyricsStyleStore';
import { useTranslationStore } from '../stores/translationStore';
import { useHiraganaStore } from '../stores/hiraganaStore';
import { validateApiKey, getApiKeyLinks } from '../utils/apiKeyValidation';
import HiraganaSettings from './HiraganaSettings';

interface StyleControlsProps {
  isOpen: boolean;
  onClose: () => void;
}

const StyleControls: React.FC<StyleControlsProps> = ({ isOpen, onClose }) => {
  const { style, updateStyle, applyPreset, getPresets, resetToDefault } = useLyricsStyleStore();
  
  // Debug logging to track style updates
  const debugUpdateStyle = (updates: any) => {
    console.log('[StyleControls] Updating style:', updates);
    updateStyle(updates);
  };
  const { displayMode, updateDisplayMode, config, updateConfig } = useTranslationStore();
  const { settings: hiraganaSettings } = useHiraganaStore();
  const [activeTab, setActiveTab] = useState<'presets' | 'font' | 'colors' | 'effects' | 'layout' | 'translation' | 'hiragana'>('presets');
  const [showHiraganaSettings, setShowHiraganaSettings] = useState(false);
  
  // Validate current API key
  const currentApiKey = config.provider === 'github' ? config.github_api_key : config.gemini_api_key;
  const apiKeyValidation = useMemo(() => validateApiKey(config.provider, currentApiKey), [config.provider, currentApiKey]);
  const apiKeyLinks = useMemo(() => getApiKeyLinks(config.provider), [config.provider]);
  
  const presets = getPresets();
  
  const tabs = [
    { id: 'presets', label: '🎨 Presets', icon: '🎨' },
    { id: 'font', label: '📝 Font', icon: '📝' },
    { id: 'colors', label: '🌈 Colors', icon: '🌈' },
    { id: 'effects', label: '✨ Effects', icon: '✨' },
    { id: 'layout', label: '📐 Layout', icon: '📐' },
    { id: 'translation', label: '🌍 Translation', icon: '🌍' },
    { id: 'hiragana', label: 'あ Hiragana', icon: 'あ' },
  ];

  const fontOptions = [
    'Inter, system-ui, sans-serif',
    'Poppins, system-ui, sans-serif',
    'Orbitron, monospace',
    'Nunito, system-ui, sans-serif',
    'Georgia, serif',
    'Helvetica, Arial, sans-serif',
    'Courier New, monospace',
  ];

  if (!isOpen) return null;

  return (
    <motion.div
      initial={{ opacity: 0, scale: 0.9 }}
      animate={{ opacity: 1, scale: 1 }}
      exit={{ opacity: 0, scale: 0.9 }}
      className="fixed inset-4 z-50 flex items-center justify-center"
      onClick={onClose}
    >
      <div
        className="bg-black/95 backdrop-blur-lg rounded-2xl border border-white/20 shadow-2xl max-w-2xl w-full max-h-[80vh] overflow-hidden"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between p-4 border-b border-white/10">
          <h2 className="text-white text-lg font-medium">Display Settings</h2>
          <button 
            onClick={onClose}
            className="text-white/60 hover:text-white transition-colors p-1 rounded hover:bg-white/10"
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        <div className="flex">
          {/* Tab Navigation */}
          <div className="w-32 bg-black/30 border-r border-white/10">
            {tabs.map((tab) => (
              <button
                key={tab.id}
                onClick={() => setActiveTab(tab.id as any)}
                className={`
                  w-full p-3 text-left text-sm transition-colors border-b border-white/5 last:border-b-0
                  ${activeTab === tab.id 
                    ? 'bg-white/10 text-white border-r-2 border-r-blue-400' 
                    : 'text-white/60 hover:text-white hover:bg-white/5'
                  }
                `}
              >
                <div className="text-xs mb-1">{tab.icon}</div>
                <div className="text-xs">{tab.label.split(' ')[1]}</div>
              </button>
            ))}
          </div>

          {/* Content Area */}
          <div className="flex-1 p-4 max-h-[60vh] overflow-y-auto">
            <AnimatePresence mode="wait">
              {activeTab === 'presets' && (
                <motion.div
                  key="presets"
                  initial={{ opacity: 0, x: 20 }}
                  animate={{ opacity: 1, x: 0 }}
                  exit={{ opacity: 0, x: -20 }}
                  className="space-y-3"
                >
                  <div className="grid grid-cols-1 gap-2">
                    {presets.map((preset) => (
                      <button
                        key={preset.name}
                        onClick={() => applyPreset(preset.name)}
                        className="p-3 bg-white/5 hover:bg-white/10 rounded-lg border border-white/10 transition-colors text-left"
                      >
                        <div className="text-white font-medium text-sm">{preset.name}</div>
                        <div className="text-white/60 text-xs mt-1">
                          {preset.style.fontSize}rem • {preset.style.fontWeight} • {preset.style.fontFamily.split(',')[0]}
                        </div>
                      </button>
                    ))}
                  </div>
                  <button
                    onClick={resetToDefault}
                    className="w-full p-2 bg-red-500/20 hover:bg-red-500/30 rounded border border-red-400/30 text-red-200 text-sm transition-colors"
                  >
                    Reset to Default
                  </button>
                </motion.div>
              )}

              {activeTab === 'font' && (
                <motion.div
                  key="font"
                  initial={{ opacity: 0, x: 20 }}
                  animate={{ opacity: 1, x: 0 }}
                  exit={{ opacity: 0, x: -20 }}
                  className="space-y-4"
                >
                  {/* Font Family */}
                  <div>
                    <label className="block text-white/80 text-sm mb-2">Font Family</label>
                    <select
                      value={style.fontFamily}
                      onChange={(e) => debugUpdateStyle({ fontFamily: e.target.value })}
                      className="w-full p-2 bg-white/10 border border-white/20 rounded text-white text-sm focus:outline-none focus:border-blue-400"
                    >
                      {fontOptions.map((font) => (
                        <option key={font} value={font} className="bg-gray-800">
                          {font.split(',')[0]}
                        </option>
                      ))}
                    </select>
                  </div>

                  {/* Font Size */}
                  <div>
                    <label className="block text-white/80 text-sm mb-2">Size: {style.fontSize}rem</label>
                    <input
                      type="range"
                      min="0.5"
                      max="5"
                      step="0.1"
                      value={style.fontSize}
                      onChange={(e) => debugUpdateStyle({ fontSize: parseFloat(e.target.value) })}
                      className="w-full"
                    />
                  </div>

                  {/* Font Weight */}
                  <div>
                    <label className="block text-white/80 text-sm mb-2">Weight</label>
                    <div className="grid grid-cols-2 gap-2">
                      {(['normal', 'medium', 'semibold', 'bold'] as const).map((weight) => (
                        <button
                          key={weight}
                          onClick={() => updateStyle({ fontWeight: weight })}
                          className={`
                            p-2 rounded text-sm transition-colors capitalize
                            ${style.fontWeight === weight 
                              ? 'bg-blue-500/30 text-blue-200 border border-blue-400/50' 
                              : 'bg-white/5 text-white/70 border border-white/10 hover:bg-white/10'
                            }
                          `}
                        >
                          {weight}
                        </button>
                      ))}
                    </div>
                  </div>
                </motion.div>
              )}

              {activeTab === 'colors' && (
                <motion.div
                  key="colors"
                  initial={{ opacity: 0, x: 20 }}
                  animate={{ opacity: 1, x: 0 }}
                  exit={{ opacity: 0, x: -20 }}
                  className="space-y-4"
                >
                  {[
                    { key: 'textColor', label: 'Text Color', value: style.textColor },
                    { key: 'highlightColor', label: 'Highlight Color', value: style.highlightColor },
                    { key: 'pastWordColor', label: 'Past Words', value: style.pastWordColor },
                    { key: 'futureWordColor', label: 'Future Words', value: style.futureWordColor },
                    { key: 'backgroundColor', label: 'Background', value: style.backgroundColor },
                  ].map((color) => (
                    <div key={color.key} className="flex items-center space-x-3">
                      <div className="w-20 text-white/80 text-sm">{color.label}</div>
                      <input
                        type="color"
                        value={color.value}
                        onChange={(e) => debugUpdateStyle({ [color.key]: e.target.value })}
                        className="w-12 h-8 rounded border border-white/20 bg-transparent"
                      />
                      <input
                        type="text"
                        value={color.value}
                        onChange={(e) => debugUpdateStyle({ [color.key]: e.target.value })}
                        className="flex-1 p-1 bg-white/10 border border-white/20 rounded text-white text-sm focus:outline-none focus:border-blue-400"
                        placeholder="#ffffff"
                      />
                    </div>
                  ))}
                  
                  <div>
                    <label className="block text-white/80 text-sm mb-2">Background Opacity: {style.backgroundOpacity}%</label>
                    <input
                      type="range"
                      min="0"
                      max="100"
                      value={style.backgroundOpacity}
                      onChange={(e) => updateStyle({ backgroundOpacity: parseInt(e.target.value) })}
                      className="w-full"
                    />
                  </div>
                </motion.div>
              )}

              {activeTab === 'effects' && (
                <motion.div
                  key="effects"
                  initial={{ opacity: 0, x: 20 }}
                  animate={{ opacity: 1, x: 0 }}
                  exit={{ opacity: 0, x: -20 }}
                  className="space-y-4"
                >
                  {/* Text Shadow */}
                  <div className="flex items-center justify-between">
                    <span className="text-white/80 text-sm">Text Shadow</span>
                    <button
                      onClick={() => updateStyle({ textShadow: !style.textShadow })}
                      className={`
                        px-3 py-1 rounded text-sm transition-colors
                        ${style.textShadow 
                          ? 'bg-green-500/30 text-green-200' 
                          : 'bg-white/10 text-white/60'
                        }
                      `}
                    >
                      {style.textShadow ? 'On' : 'Off'}
                    </button>
                  </div>
                  
                  {style.textShadow && (
                    <div className="space-y-2 pl-4">
                      <div>
                        <label className="block text-white/60 text-xs mb-1">Blur: {style.textShadowBlur}px</label>
                        <input
                          type="range"
                          min="0"
                          max="50"
                          value={style.textShadowBlur}
                          onChange={(e) => updateStyle({ textShadowBlur: parseInt(e.target.value) })}
                          className="w-full"
                        />
                      </div>
                      <div className="flex items-center space-x-2">
                        <span className="text-white/60 text-xs">Color:</span>
                        <input
                          type="color"
                          value={style.textShadowColor}
                          onChange={(e) => updateStyle({ textShadowColor: e.target.value })}
                          className="w-8 h-6 rounded border border-white/20 bg-transparent"
                        />
                      </div>
                    </div>
                  )}

                  {/* Text Glow */}
                  <div className="flex items-center justify-between">
                    <span className="text-white/80 text-sm">Text Glow</span>
                    <button
                      onClick={() => updateStyle({ textGlow: !style.textGlow })}
                      className={`
                        px-3 py-1 rounded text-sm transition-colors
                        ${style.textGlow 
                          ? 'bg-green-500/30 text-green-200' 
                          : 'bg-white/10 text-white/60'
                        }
                      `}
                    >
                      {style.textGlow ? 'On' : 'Off'}
                    </button>
                  </div>
                  
                  {style.textGlow && (
                    <div className="flex items-center space-x-2 pl-4">
                      <span className="text-white/60 text-xs">Color:</span>
                      <input
                        type="color"
                        value={style.textGlowColor}
                        onChange={(e) => updateStyle({ textGlowColor: e.target.value })}
                        className="w-8 h-6 rounded border border-white/20 bg-transparent"
                      />
                    </div>
                  )}

                  {/* Background Blur */}
                  <div>
                    <label className="block text-white/80 text-sm mb-2">Background Blur: {style.backgroundBlur}px</label>
                    <input
                      type="range"
                      min="0"
                      max="20"
                      value={style.backgroundBlur}
                      onChange={(e) => updateStyle({ backgroundBlur: parseInt(e.target.value) })}
                      className="w-full"
                    />
                  </div>

                  {/* Animation Intensity */}
                  <div>
                    <label className="block text-white/80 text-sm mb-2">Animation: {style.animationIntensity}%</label>
                    <input
                      type="range"
                      min="0"
                      max="100"
                      value={style.animationIntensity}
                      onChange={(e) => updateStyle({ animationIntensity: parseInt(e.target.value) })}
                      className="w-full"
                    />
                  </div>
                </motion.div>
              )}

              {activeTab === 'layout' && (
                <motion.div
                  key="layout"
                  initial={{ opacity: 0, x: 20 }}
                  animate={{ opacity: 1, x: 0 }}
                  exit={{ opacity: 0, x: -20 }}
                  className="space-y-4"
                >
                  {/* Text Alignment */}
                  <div>
                    <label className="block text-white/80 text-sm mb-2">Text Alignment</label>
                    <div className="grid grid-cols-3 gap-2">
                      {(['left', 'center', 'right'] as const).map((align) => (
                        <button
                          key={align}
                          onClick={() => updateStyle({ textAlign: align })}
                          className={`
                            p-2 rounded text-sm transition-colors capitalize
                            ${style.textAlign === align 
                              ? 'bg-blue-500/30 text-blue-200 border border-blue-400/50' 
                              : 'bg-white/5 text-white/70 border border-white/10 hover:bg-white/10'
                            }
                          `}
                        >
                          {align}
                        </button>
                      ))}
                    </div>
                  </div>

                  {/* Line Height */}
                  <div>
                    <label className="block text-white/80 text-sm mb-2">Line Height: {style.lineHeight}</label>
                    <input
                      type="range"
                      min="1.0"
                      max="3.0"
                      step="0.1"
                      value={style.lineHeight}
                      onChange={(e) => updateStyle({ lineHeight: parseFloat(e.target.value) })}
                      className="w-full"
                    />
                  </div>

                  {/* Letter Spacing */}
                  <div>
                    <label className="block text-white/80 text-sm mb-2">Letter Spacing: {style.letterSpacing}em</label>
                    <input
                      type="range"
                      min="-0.1"
                      max="0.5"
                      step="0.01"
                      value={style.letterSpacing}
                      onChange={(e) => updateStyle({ letterSpacing: parseFloat(e.target.value) })}
                      className="w-full"
                    />
                  </div>
                </motion.div>
              )}

              {activeTab === 'translation' && (
                <motion.div
                  key="translation"
                  initial={{ opacity: 0, x: 20 }}
                  animate={{ opacity: 1, x: 0 }}
                  exit={{ opacity: 0, x: -20 }}
                  className="space-y-4"
                >
                  {/* Display Mode */}
                  <div>
                    <label className="block text-white/80 text-sm mb-2">Display Mode</label>
                    <div className="grid grid-cols-2 gap-2">
                      {(['original', 'translated', 'bilingual', 'side-by-side'] as const).map((mode) => (
                        <button
                          key={mode}
                          onClick={() => updateDisplayMode({ mode })}
                          className={`
                            p-2 rounded text-sm transition-colors capitalize
                            ${displayMode.mode === mode 
                              ? 'bg-blue-500/30 text-blue-200 border border-blue-400/50' 
                              : 'bg-white/5 text-white/70 border border-white/10 hover:bg-white/10'
                            }
                          `}
                        >
                          {mode === 'side-by-side' ? 'Side by Side' : mode}
                        </button>
                      ))}
                    </div>
                  </div>

                  {/* Font Sizes */}
                  <div className="grid grid-cols-2 gap-4">
                    <div>
                      <label className="block text-white/80 text-sm mb-2">Original Size: {Math.round(displayMode.fontSize.original * 100)}%</label>
                      <input
                        type="range"
                        min="0.5"
                        max="1.5"
                        step="0.05"
                        value={displayMode.fontSize.original}
                        onChange={(e) => updateDisplayMode({ 
                          fontSize: { ...displayMode.fontSize, original: parseFloat(e.target.value) }
                        })}
                        className="w-full"
                      />
                    </div>
                    <div>
                      <label className="block text-white/80 text-sm mb-2">Translation Size: {Math.round(displayMode.fontSize.translated * 100)}%</label>
                      <input
                        type="range"
                        min="0.5"
                        max="1.5"
                        step="0.05"
                        value={displayMode.fontSize.translated}
                        onChange={(e) => updateDisplayMode({ 
                          fontSize: { ...displayMode.fontSize, translated: parseFloat(e.target.value) }
                        })}
                        className="w-full"
                      />
                    </div>
                  </div>

                  {/* Opacity Settings */}
                  <div className="grid grid-cols-2 gap-4">
                    <div>
                      <label className="block text-white/80 text-sm mb-2">Original Opacity: {Math.round(displayMode.opacity.original * 100)}%</label>
                      <input
                        type="range"
                        min="0.3"
                        max="1"
                        step="0.05"
                        value={displayMode.opacity.original}
                        onChange={(e) => updateDisplayMode({ 
                          opacity: { ...displayMode.opacity, original: parseFloat(e.target.value) }
                        })}
                        className="w-full"
                      />
                    </div>
                    <div>
                      <label className="block text-white/80 text-sm mb-2">Translation Opacity: {Math.round(displayMode.opacity.translated * 100)}%</label>
                      <input
                        type="range"
                        min="0.3"
                        max="1"
                        step="0.05"
                        value={displayMode.opacity.translated}
                        onChange={(e) => updateDisplayMode({ 
                          opacity: { ...displayMode.opacity, translated: parseFloat(e.target.value) }
                        })}
                        className="w-full"
                      />
                    </div>
                  </div>

                  {/* Colors */}
                  <div className="grid grid-cols-2 gap-4">
                    <div className="flex items-center space-x-3">
                      <div className="text-white/80 text-sm">Original Color</div>
                      <input
                        type="color"
                        value={displayMode.colors.original}
                        onChange={(e) => updateDisplayMode({ 
                          colors: { ...displayMode.colors, original: e.target.value }
                        })}
                        className="w-12 h-8 rounded border border-white/20 bg-transparent"
                      />
                    </div>
                    <div className="flex items-center space-x-3">
                      <div className="text-white/80 text-sm">Translation Color</div>
                      <input
                        type="color"
                        value={displayMode.colors.translated}
                        onChange={(e) => updateDisplayMode({ 
                          colors: { ...displayMode.colors, translated: e.target.value }
                        })}
                        className="w-12 h-8 rounded border border-white/20 bg-transparent"
                      />
                    </div>
                  </div>

                  {/* Spacing */}
                  <div>
                    <label className="block text-white/80 text-sm mb-2">Line Spacing: {displayMode.spacing}px</label>
                    <input
                      type="range"
                      min="0"
                      max="20"
                      value={displayMode.spacing}
                      onChange={(e) => updateDisplayMode({ spacing: parseInt(e.target.value) })}
                      className="w-full"
                    />
                  </div>

                  {/* Provider Selection */}
                  <div>
                    <label className="block text-white/80 text-sm mb-2">Translation Provider</label>
                    <div className="grid grid-cols-2 gap-2">
                      {(['gemini', 'github'] as const).map((provider) => (
                        <button
                          key={provider}
                          onClick={() => updateConfig({ provider })}
                          className={`
                            p-3 rounded text-sm transition-colors capitalize
                            ${config.provider === provider 
                              ? 'bg-blue-500/30 text-blue-200 border border-blue-400/50' 
                              : 'bg-white/5 text-white/70 border border-white/10 hover:bg-white/10'
                            }
                          `}
                        >
                          <div className="font-medium">
                            {provider === 'github' ? 'GitHub AI' : 'Google Gemini'}
                            {provider === 'gemini' && (
                              <span className="text-green-400 text-xs ml-1">• Recommended</span>
                            )}
                          </div>
                          <div className="text-xs opacity-70 mt-1">
                            {provider === 'github' ? 'Llama, Maverick, Ministral' : '2.0 Flash, 2.5 Flash, 2.5 Pro'}
                          </div>
                        </button>
                      ))}
                    </div>
                  </div>

                  {/* API Key Configuration */}
                  <div>
                    <label className="block text-white/80 text-sm mb-2">
                      {config.provider === 'github' ? 'GitHub API Key' : 'Gemini API Key'}
                      {apiKeyValidation.isValid && (
                        <span className="text-green-400 text-xs ml-2">✓ {apiKeyValidation.message}</span>
                      )}
                    </label>
                    <div className="relative">
                      <input
                        type="password"
                        placeholder={config.provider === 'github' 
                          ? 'Enter GitHub personal access token' 
                          : 'Enter Gemini API key'
                        }
                        value={config.provider === 'github' 
                          ? config.github_api_key || '' 
                          : config.gemini_api_key || ''
                        }
                        onChange={(e) => {
                          const trimmedValue = e.target.value.trim();
                          updateConfig(
                            config.provider === 'github'
                              ? { github_api_key: trimmedValue }
                              : { gemini_api_key: trimmedValue }
                          );
                        }}
                        className={`w-full px-3 py-2 bg-white/5 border rounded text-white placeholder-white/50 focus:outline-none ${
                          currentApiKey && !apiKeyValidation.isValid
                            ? 'border-red-400 focus:border-red-400'
                            : apiKeyValidation.isValid
                            ? 'border-green-400 focus:border-green-400'
                            : 'border-white/20 focus:border-blue-400'
                        }`}
                      />
                    </div>
                    
                    {/* Validation message */}
                    {currentApiKey && !apiKeyValidation.isValid && (
                      <div className={`text-xs mt-1 ${
                        apiKeyValidation.severity === 'error' ? 'text-red-400' :
                        apiKeyValidation.severity === 'warning' ? 'text-yellow-400' :
                        'text-green-400'
                      }`}>
                        ⚠️ {apiKeyValidation.message}
                      </div>
                    )}
                    
                    {/* Help text and link */}
                    <div className="flex items-center justify-between mt-1">
                      <div className="text-xs text-white/50">
                        {config.provider === 'github' 
                          ? 'Get your token from GitHub Settings → Developer settings'
                          : 'Get your API key from Google AI Studio'
                        }
                      </div>
                      <button
                        onClick={() => window.open(apiKeyLinks.url, '_blank')}
                        className="text-xs text-blue-400 hover:text-blue-300 transition-colors"
                      >
                        {apiKeyLinks.label} ↗
                      </button>
                    </div>
                    
                    {config.provider === 'github' && (
                      <div className="text-xs text-yellow-400/70 mt-1">
                        💡 Your token needs access to GitHub AI models. Select "All repositories" or specific repos with model access.
                      </div>
                    )}
                  </div>

                  {/* Animation Settings */}
                  <div className="space-y-2">
                    <div className="flex items-center justify-between">
                      <span className="text-white/80 text-sm">Animations</span>
                      <button
                        onClick={() => updateDisplayMode({ 
                          animation: { ...displayMode.animation, enabled: !displayMode.animation.enabled }
                        })}
                        className={`
                          px-3 py-1 rounded text-sm transition-colors
                          ${displayMode.animation.enabled 
                            ? 'bg-green-500/30 text-green-200' 
                            : 'bg-white/10 text-white/60'
                          }
                        `}
                      >
                        {displayMode.animation.enabled ? 'On' : 'Off'}
                      </button>
                    </div>
                    
                    {displayMode.animation.enabled && (
                      <>
                        <div>
                          <label className="block text-white/80 text-sm mb-2">Animation Type</label>
                          <div className="grid grid-cols-3 gap-2">
                            {(['fade', 'slide', 'scale'] as const).map((type) => (
                              <button
                                key={type}
                                onClick={() => updateDisplayMode({ 
                                  animation: { ...displayMode.animation, type }
                                })}
                                className={`
                                  p-2 rounded text-sm transition-colors capitalize
                                  ${displayMode.animation.type === type 
                                    ? 'bg-blue-500/30 text-blue-200 border border-blue-400/50' 
                                    : 'bg-white/5 text-white/70 border border-white/10 hover:bg-white/10'
                                  }
                                `}
                              >
                                {type}
                              </button>
                            ))}
                          </div>
                        </div>
                        
                        <div>
                          <label className="block text-white/80 text-sm mb-2">Duration: {displayMode.animation.duration}ms</label>
                          <input
                            type="range"
                            min="100"
                            max="1000"
                            step="50"
                            value={displayMode.animation.duration}
                            onChange={(e) => updateDisplayMode({ 
                              animation: { ...displayMode.animation, duration: parseInt(e.target.value) }
                            })}
                            className="w-full"
                          />
                        </div>
                      </>
                    )}
                  </div>
                </motion.div>
              )}

              {activeTab === 'hiragana' && (
                <motion.div
                  key="hiragana"
                  initial={{ opacity: 0, x: 20 }}
                  animate={{ opacity: 1, x: 0 }}
                  exit={{ opacity: 0, x: -20 }}
                  className="space-y-4"
                >
                  <HiraganaSettings isVisible={true} />
                  
                  <div className="text-xs text-white/50 border-t border-white/10 pt-3">
                    <strong>How it works:</strong> When enabled, hiragana readings will automatically appear for Japanese lyrics containing kanji characters. The autoruby library analyzes the text and provides phonetic readings to help with pronunciation.
                  </div>
                </motion.div>
              )}
            </AnimatePresence>
          </div>
        </div>
      </div>
    </motion.div>
  );
};

export default StyleControls;