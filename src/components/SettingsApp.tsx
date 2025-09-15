import React, { useState, useMemo } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { useLyricsStyleStore } from '../stores/lyricsStyleStore';
import { useTranslationStore } from '../stores/translationStore';
import { useHiraganaStore } from '../stores/hiraganaStore';
import { validateApiKey, getApiKeyLinks } from '../utils/apiKeyValidation';
import HiraganaSettings from './HiraganaSettings';
import { invoke } from '@tauri-apps/api/core';

const SettingsApp: React.FC = () => {
  const { style, updateStyle, applyPreset, getPresets, resetToDefault } = useLyricsStyleStore();

  // Debug logging to track style updates
  const debugUpdateStyle = (updates: any) => {
    console.log('[SettingsApp] Updating style:', updates);
    updateStyle(updates);
  };
  const { displayMode, updateDisplayMode, config, updateConfig } = useTranslationStore();
  const { settings: hiraganaSettings } = useHiraganaStore();
  const [activeTab, setActiveTab] = useState<'presets' | 'font' | 'colors' | 'effects' | 'layout' | 'translation' | 'hiragana'>('presets');
  
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

  const handleClose = async () => {
    try {
      await invoke('close_settings_window');
    } catch (error) {
      console.error('Failed to close settings window:', error);
    }
  };

  return (
    <div className="h-screen bg-gradient-to-br from-gray-900 via-black to-gray-800 text-white">
      <div className="h-full flex flex-col">
        {/* Header */}
        <div className="flex items-center justify-between p-4 border-b border-white/10 bg-black/30">
          <h1 className="text-xl font-semibold">Lyryc Settings</h1>
          <button 
            onClick={handleClose}
            className="text-white/60 hover:text-white transition-colors p-2 rounded-lg hover:bg-white/10"
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        <div className="flex flex-1 overflow-hidden">
          {/* Tab Navigation */}
          <div className="w-48 bg-black/20 border-r border-white/10 flex-shrink-0">
            <div className="p-2">
              {tabs.map((tab) => (
                <button
                  key={tab.id}
                  onClick={() => setActiveTab(tab.id as any)}
                  className={`
                    w-full p-3 text-left text-sm transition-all duration-200 rounded-lg mb-1
                    flex items-center space-x-3
                    ${activeTab === tab.id 
                      ? 'bg-white/15 text-white border border-white/20 shadow-lg' 
                      : 'text-white/70 hover:text-white hover:bg-white/8'
                    }
                  `}
                >
                  <span className="text-lg">{tab.icon}</span>
                  <span className="font-medium">{tab.label.split(' ').slice(1).join(' ')}</span>
                </button>
              ))}
            </div>
          </div>

          {/* Content Area */}
          <div className="flex-1 p-6 overflow-y-auto">
            <AnimatePresence mode="wait">
              {activeTab === 'presets' && (
                <motion.div
                  key="presets"
                  initial={{ opacity: 0, x: 20 }}
                  animate={{ opacity: 1, x: 0 }}
                  exit={{ opacity: 0, x: -20 }}
                  className="space-y-4"
                >
                  <h2 className="text-lg font-semibold mb-4">Style Presets</h2>
                  <div className="grid grid-cols-1 gap-3">
                    {presets.map((preset) => (
                      <button
                        key={preset.name}
                        onClick={() => applyPreset(preset.name)}
                        className="p-4 bg-white/5 hover:bg-white/10 rounded-xl border border-white/10 transition-colors text-left"
                      >
                        <div className="text-white font-medium">{preset.name}</div>
                        <div className="text-white/60 text-sm mt-1">
                          {preset.style.fontSize}rem • {preset.style.fontWeight} • {preset.style.fontFamily.split(',')[0]}
                        </div>
                      </button>
                    ))}
                  </div>
                  <button
                    onClick={resetToDefault}
                    className="w-full p-3 bg-red-500/20 hover:bg-red-500/30 rounded-xl border border-red-400/30 text-red-200 transition-colors"
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
                  className="space-y-6"
                >
                  <h2 className="text-lg font-semibold mb-4">Font Settings</h2>
                  
                  {/* Font Family */}
                  <div>
                    <label className="block text-white/80 text-sm font-medium mb-3">Font Family</label>
                    <select
                      value={style.fontFamily}
                      onChange={(e) => debugUpdateStyle({ fontFamily: e.target.value })}
                      className="w-full p-3 bg-white/10 border border-white/20 rounded-xl text-white focus:outline-none focus:border-blue-400 focus:ring-2 focus:ring-blue-400/20"
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
                    <label className="block text-white/80 text-sm font-medium mb-3">
                      Font Size: <span className="text-blue-300">{style.fontSize}rem</span>
                    </label>
                    <input
                      type="range"
                      min="0.5"
                      max="5"
                      step="0.1"
                      value={style.fontSize}
                      onChange={(e) => debugUpdateStyle({ fontSize: parseFloat(e.target.value) })}
                      className="w-full h-2 bg-white/10 rounded-lg appearance-none cursor-pointer slider"
                    />
                  </div>

                  {/* Font Weight */}
                  <div>
                    <label className="block text-white/80 text-sm font-medium mb-3">Font Weight</label>
                    <div className="grid grid-cols-2 gap-3">
                      {(['normal', 'medium', 'semibold', 'bold'] as const).map((weight) => (
                        <button
                          key={weight}
                          onClick={() => debugUpdateStyle({ fontWeight: weight })}
                          className={`
                            p-3 rounded-xl transition-all duration-200 capitalize font-medium
                            ${style.fontWeight === weight 
                              ? 'bg-blue-500/30 text-blue-200 border border-blue-400/50 shadow-lg' 
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
                  className="space-y-6"
                >
                  <h2 className="text-lg font-semibold mb-4">Color Settings</h2>
                  
                  {[
                    { key: 'textColor', label: 'Text Color', value: style.textColor },
                    { key: 'highlightColor', label: 'Highlight Color', value: style.highlightColor },
                    { key: 'pastWordColor', label: 'Past Words', value: style.pastWordColor },
                    { key: 'futureWordColor', label: 'Future Words', value: style.futureWordColor },
                    { key: 'backgroundColor', label: 'Background', value: style.backgroundColor },
                  ].map((color) => (
                    <div key={color.key} className="flex items-center space-x-4">
                      <div className="w-32 text-white/80 font-medium">{color.label}</div>
                      <input
                        type="color"
                        value={color.value}
                        onChange={(e) => debugUpdateStyle({ [color.key]: e.target.value })}
                        className="w-12 h-12 rounded-lg border-2 border-white/20 bg-transparent cursor-pointer"
                      />
                      <input
                        type="text"
                        value={color.value}
                        onChange={(e) => debugUpdateStyle({ [color.key]: e.target.value })}
                        className="flex-1 p-3 bg-white/10 border border-white/20 rounded-xl text-white focus:outline-none focus:border-blue-400 focus:ring-2 focus:ring-blue-400/20"
                        placeholder="#ffffff"
                      />
                    </div>
                  ))}
                  
                  <div>
                    <label className="block text-white/80 text-sm font-medium mb-3">
                      Background Opacity: <span className="text-blue-300">{style.backgroundOpacity}%</span>
                    </label>
                    <input
                      type="range"
                      min="0"
                      max="100"
                      value={style.backgroundOpacity}
                      onChange={(e) => debugUpdateStyle({ backgroundOpacity: parseInt(e.target.value) })}
                      className="w-full h-2 bg-white/10 rounded-lg appearance-none cursor-pointer slider"
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
                  className="space-y-6"
                >
                  <h2 className="text-lg font-semibold mb-4">Visual Effects</h2>
                  
                  {/* Text Shadow */}
                  <div className="flex items-center justify-between p-4 bg-white/5 rounded-xl border border-white/10">
                    <span className="text-white/80 font-medium">Text Shadow</span>
                    <button
                      onClick={() => debugUpdateStyle({ textShadow: !style.textShadow })}
                      className={`
                        px-4 py-2 rounded-lg font-medium transition-all duration-200
                        ${style.textShadow 
                          ? 'bg-green-500/30 text-green-200 border border-green-400/50' 
                          : 'bg-white/10 text-white/60 border border-white/20'
                        }
                      `}
                    >
                      {style.textShadow ? 'On' : 'Off'}
                    </button>
                  </div>
                  
                  {style.textShadow && (
                    <div className="space-y-4 p-4 bg-white/5 rounded-xl border border-white/10">
                      <div>
                        <label className="block text-white/60 text-sm mb-2">
                          Blur: <span className="text-blue-300">{style.textShadowBlur}px</span>
                        </label>
                        <input
                          type="range"
                          min="0"
                          max="50"
                          value={style.textShadowBlur}
                          onChange={(e) => debugUpdateStyle({ textShadowBlur: parseInt(e.target.value) })}
                          className="w-full h-2 bg-white/10 rounded-lg appearance-none cursor-pointer slider"
                        />
                      </div>
                      <div className="flex items-center space-x-3">
                        <span className="text-white/60 text-sm">Color:</span>
                        <input
                          type="color"
                          value={style.textShadowColor}
                          onChange={(e) => debugUpdateStyle({ textShadowColor: e.target.value })}
                          className="w-10 h-10 rounded-lg border border-white/20 bg-transparent cursor-pointer"
                        />
                      </div>
                    </div>
                  )}

                  {/* Text Glow */}
                  <div className="flex items-center justify-between p-4 bg-white/5 rounded-xl border border-white/10">
                    <span className="text-white/80 font-medium">Text Glow</span>
                    <button
                      onClick={() => debugUpdateStyle({ textGlow: !style.textGlow })}
                      className={`
                        px-4 py-2 rounded-lg font-medium transition-all duration-200
                        ${style.textGlow 
                          ? 'bg-green-500/30 text-green-200 border border-green-400/50' 
                          : 'bg-white/10 text-white/60 border border-white/20'
                        }
                      `}
                    >
                      {style.textGlow ? 'On' : 'Off'}
                    </button>
                  </div>
                  
                  {style.textGlow && (
                    <div className="flex items-center space-x-3 p-4 bg-white/5 rounded-xl border border-white/10">
                      <span className="text-white/60 text-sm">Color:</span>
                      <input
                        type="color"
                        value={style.textGlowColor}
                        onChange={(e) => debugUpdateStyle({ textGlowColor: e.target.value })}
                        className="w-10 h-10 rounded-lg border border-white/20 bg-transparent cursor-pointer"
                      />
                    </div>
                  )}

                  {/* Background Blur */}
                  <div>
                    <label className="block text-white/80 text-sm font-medium mb-3">
                      Background Blur: <span className="text-blue-300">{style.backgroundBlur}px</span>
                    </label>
                    <input
                      type="range"
                      min="0"
                      max="20"
                      value={style.backgroundBlur}
                      onChange={(e) => debugUpdateStyle({ backgroundBlur: parseInt(e.target.value) })}
                      className="w-full h-2 bg-white/10 rounded-lg appearance-none cursor-pointer slider"
                    />
                  </div>

                  {/* Animation Intensity */}
                  <div>
                    <label className="block text-white/80 text-sm font-medium mb-3">
                      Animation Intensity: <span className="text-blue-300">{style.animationIntensity}%</span>
                    </label>
                    <input
                      type="range"
                      min="0"
                      max="100"
                      value={style.animationIntensity}
                      onChange={(e) => debugUpdateStyle({ animationIntensity: parseInt(e.target.value) })}
                      className="w-full h-2 bg-white/10 rounded-lg appearance-none cursor-pointer slider"
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
                  className="space-y-6"
                >
                  <h2 className="text-lg font-semibold mb-4">Layout Settings</h2>
                  
                  {/* Text Alignment */}
                  <div>
                    <label className="block text-white/80 text-sm font-medium mb-3">Text Alignment</label>
                    <div className="grid grid-cols-3 gap-3">
                      {(['left', 'center', 'right'] as const).map((align) => (
                        <button
                          key={align}
                          onClick={() => debugUpdateStyle({ textAlign: align })}
                          className={`
                            p-3 rounded-xl transition-all duration-200 capitalize font-medium
                            ${style.textAlign === align
                              ? 'bg-blue-500/30 text-blue-200 border border-blue-400/50 shadow-lg'
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
                    <label className="block text-white/80 text-sm font-medium mb-3">
                      Line Height: <span className="text-blue-300">{style.lineHeight}</span>
                    </label>
                    <input
                      type="range"
                      min="1.0"
                      max="3.0"
                      step="0.1"
                      value={style.lineHeight}
                      onChange={(e) => debugUpdateStyle({ lineHeight: parseFloat(e.target.value) })}
                      className="w-full h-2 bg-white/10 rounded-lg appearance-none cursor-pointer slider"
                    />
                  </div>

                  {/* Letter Spacing */}
                  <div>
                    <label className="block text-white/80 text-sm font-medium mb-3">
                      Letter Spacing: <span className="text-blue-300">{style.letterSpacing}em</span>
                    </label>
                    <input
                      type="range"
                      min="-0.1"
                      max="0.5"
                      step="0.01"
                      value={style.letterSpacing}
                      onChange={(e) => debugUpdateStyle({ letterSpacing: parseFloat(e.target.value) })}
                      className="w-full h-2 bg-white/10 rounded-lg appearance-none cursor-pointer slider"
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
                  className="space-y-6"
                >
                  <h2 className="text-lg font-semibold mb-4">Translation Settings</h2>
                  
                  {/* Display Mode */}
                  <div>
                    <label className="block text-white/80 text-sm font-medium mb-3">Display Mode</label>
                    <div className="grid grid-cols-2 gap-3">
                      {(['original', 'translated', 'bilingual', 'side-by-side'] as const).map((mode) => (
                        <button
                          key={mode}
                          onClick={() => updateDisplayMode({ mode })}
                          className={`
                            p-3 rounded-xl transition-all duration-200 capitalize font-medium text-center
                            ${displayMode.mode === mode 
                              ? 'bg-blue-500/30 text-blue-200 border border-blue-400/50 shadow-lg' 
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
                  <div className="grid grid-cols-2 gap-6">
                    <div>
                      <label className="block text-white/80 text-sm font-medium mb-3">
                        Original Size: <span className="text-blue-300">{Math.round(displayMode.fontSize.original * 100)}%</span>
                      </label>
                      <input
                        type="range"
                        min="0.5"
                        max="1.5"
                        step="0.05"
                        value={displayMode.fontSize.original}
                        onChange={(e) => updateDisplayMode({ 
                          fontSize: { ...displayMode.fontSize, original: parseFloat(e.target.value) }
                        })}
                        className="w-full h-2 bg-white/10 rounded-lg appearance-none cursor-pointer slider"
                      />
                    </div>
                    <div>
                      <label className="block text-white/80 text-sm font-medium mb-3">
                        Translation Size: <span className="text-blue-300">{Math.round(displayMode.fontSize.translated * 100)}%</span>
                      </label>
                      <input
                        type="range"
                        min="0.5"
                        max="1.5"
                        step="0.05"
                        value={displayMode.fontSize.translated}
                        onChange={(e) => updateDisplayMode({ 
                          fontSize: { ...displayMode.fontSize, translated: parseFloat(e.target.value) }
                        })}
                        className="w-full h-2 bg-white/10 rounded-lg appearance-none cursor-pointer slider"
                      />
                    </div>
                  </div>

                  {/* Opacity Settings */}
                  <div className="grid grid-cols-2 gap-6">
                    <div>
                      <label className="block text-white/80 text-sm font-medium mb-3">
                        Original Opacity: <span className="text-blue-300">{Math.round(displayMode.opacity.original * 100)}%</span>
                      </label>
                      <input
                        type="range"
                        min="0.3"
                        max="1"
                        step="0.05"
                        value={displayMode.opacity.original}
                        onChange={(e) => updateDisplayMode({ 
                          opacity: { ...displayMode.opacity, original: parseFloat(e.target.value) }
                        })}
                        className="w-full h-2 bg-white/10 rounded-lg appearance-none cursor-pointer slider"
                      />
                    </div>
                    <div>
                      <label className="block text-white/80 text-sm font-medium mb-3">
                        Translation Opacity: <span className="text-blue-300">{Math.round(displayMode.opacity.translated * 100)}%</span>
                      </label>
                      <input
                        type="range"
                        min="0.3"
                        max="1"
                        step="0.05"
                        value={displayMode.opacity.translated}
                        onChange={(e) => updateDisplayMode({ 
                          opacity: { ...displayMode.opacity, translated: parseFloat(e.target.value) }
                        })}
                        className="w-full h-2 bg-white/10 rounded-lg appearance-none cursor-pointer slider"
                      />
                    </div>
                  </div>

                  {/* Colors */}
                  <div className="grid grid-cols-2 gap-6">
                    <div className="flex items-center space-x-3">
                      <div className="text-white/80 font-medium">Original Color</div>
                      <input
                        type="color"
                        value={displayMode.colors.original}
                        onChange={(e) => updateDisplayMode({ 
                          colors: { ...displayMode.colors, original: e.target.value }
                        })}
                        className="w-12 h-12 rounded-lg border-2 border-white/20 bg-transparent cursor-pointer"
                      />
                    </div>
                    <div className="flex items-center space-x-3">
                      <div className="text-white/80 font-medium">Translation Color</div>
                      <input
                        type="color"
                        value={displayMode.colors.translated}
                        onChange={(e) => updateDisplayMode({ 
                          colors: { ...displayMode.colors, translated: e.target.value }
                        })}
                        className="w-12 h-12 rounded-lg border-2 border-white/20 bg-transparent cursor-pointer"
                      />
                    </div>
                  </div>

                  {/* Spacing */}
                  <div>
                    <label className="block text-white/80 text-sm font-medium mb-3">
                      Line Spacing: <span className="text-blue-300">{displayMode.spacing}px</span>
                    </label>
                    <input
                      type="range"
                      min="0"
                      max="20"
                      value={displayMode.spacing}
                      onChange={(e) => updateDisplayMode({ spacing: parseInt(e.target.value) })}
                      className="w-full h-2 bg-white/10 rounded-lg appearance-none cursor-pointer slider"
                    />
                  </div>

                  {/* Provider Selection */}
                  <div>
                    <label className="block text-white/80 text-sm font-medium mb-3">Translation Provider</label>
                    <div className="grid grid-cols-2 gap-3">
                      {(['gemini', 'github'] as const).map((provider) => (
                        <button
                          key={provider}
                          onClick={() => updateConfig({ provider })}
                          className={`
                            p-4 rounded-xl transition-all duration-200 text-left
                            ${config.provider === provider 
                              ? 'bg-blue-500/30 text-blue-200 border border-blue-400/50 shadow-lg' 
                              : 'bg-white/5 text-white/70 border border-white/10 hover:bg-white/10'
                            }
                          `}
                        >
                          <div className="font-semibold">
                            {provider === 'github' ? 'GitHub AI' : 'Google Gemini'}
                            {provider === 'gemini' && (
                              <span className="text-green-400 text-xs ml-2">• Recommended</span>
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
                  <div className="p-4 bg-white/5 rounded-xl border border-white/10">
                    <label className="block text-white/80 text-sm font-medium mb-3">
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
                        className={`w-full px-4 py-3 bg-white/5 border rounded-xl text-white placeholder-white/50 focus:outline-none focus:ring-2 ${
                          currentApiKey && !apiKeyValidation.isValid
                            ? 'border-red-400 focus:border-red-400 focus:ring-red-400/20'
                            : apiKeyValidation.isValid
                            ? 'border-green-400 focus:border-green-400 focus:ring-green-400/20'
                            : 'border-white/20 focus:border-blue-400 focus:ring-blue-400/20'
                        }`}
                      />
                    </div>
                    
                    {/* Validation message */}
                    {currentApiKey && !apiKeyValidation.isValid && (
                      <div className={`text-sm mt-2 ${
                        apiKeyValidation.severity === 'error' ? 'text-red-400' :
                        apiKeyValidation.severity === 'warning' ? 'text-yellow-400' :
                        'text-green-400'
                      }`}>
                        ⚠️ {apiKeyValidation.message}
                      </div>
                    )}
                    
                    {/* Help text and link */}
                    <div className="flex items-center justify-between mt-2">
                      <div className="text-xs text-white/50">
                        {config.provider === 'github' 
                          ? 'Get your token from GitHub Settings → Developer settings'
                          : 'Get your API key from Google AI Studio'
                        }
                      </div>
                      <button
                        onClick={() => window.open(apiKeyLinks.url, '_blank')}
                        className="text-sm text-blue-400 hover:text-blue-300 transition-colors"
                      >
                        {apiKeyLinks.label} ↗
                      </button>
                    </div>
                    
                    {config.provider === 'github' && (
                      <div className="text-xs text-yellow-400/70 mt-2">
                        💡 Your token needs access to GitHub AI models. Select "All repositories" or specific repos with model access.
                      </div>
                    )}
                  </div>

                  {/* Animation Settings */}
                  <div className="space-y-4 p-4 bg-white/5 rounded-xl border border-white/10">
                    <div className="flex items-center justify-between">
                      <span className="text-white/80 font-medium">Animations</span>
                      <button
                        onClick={() => updateDisplayMode({ 
                          animation: { ...displayMode.animation, enabled: !displayMode.animation.enabled }
                        })}
                        className={`
                          px-4 py-2 rounded-lg font-medium transition-all duration-200
                          ${displayMode.animation.enabled 
                            ? 'bg-green-500/30 text-green-200 border border-green-400/50' 
                            : 'bg-white/10 text-white/60 border border-white/20'
                          }
                        `}
                      >
                        {displayMode.animation.enabled ? 'On' : 'Off'}
                      </button>
                    </div>
                    
                    {displayMode.animation.enabled && (
                      <>
                        <div>
                          <label className="block text-white/80 text-sm font-medium mb-3">Animation Type</label>
                          <div className="grid grid-cols-3 gap-3">
                            {(['fade', 'slide', 'scale'] as const).map((type) => (
                              <button
                                key={type}
                                onClick={() => updateDisplayMode({ 
                                  animation: { ...displayMode.animation, type }
                                })}
                                className={`
                                  p-2 rounded-lg transition-all duration-200 capitalize font-medium
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
                          <label className="block text-white/80 text-sm font-medium mb-3">
                            Duration: <span className="text-blue-300">{displayMode.animation.duration}ms</span>
                          </label>
                          <input
                            type="range"
                            min="100"
                            max="1000"
                            step="50"
                            value={displayMode.animation.duration}
                            onChange={(e) => updateDisplayMode({ 
                              animation: { ...displayMode.animation, duration: parseInt(e.target.value) }
                            })}
                            className="w-full h-2 bg-white/10 rounded-lg appearance-none cursor-pointer slider"
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
                  className="space-y-6"
                >
                  <h2 className="text-lg font-semibold mb-4">Hiragana Settings</h2>
                  
                  <HiraganaSettings isVisible={true} />
                  
                  <div className="text-sm text-white/60 bg-white/5 border border-white/10 rounded-xl p-4">
                    <strong className="text-white/80">How it works:</strong> When enabled, hiragana readings will automatically appear for Japanese lyrics containing kanji characters. The autoruby library analyzes the text and provides phonetic readings to help with pronunciation.
                  </div>
                </motion.div>
              )}
            </AnimatePresence>
          </div>
        </div>
      </div>

      <style>{`
        .slider {
          background: linear-gradient(90deg, #3b82f6 0%, #6366f1 100%);
          border-radius: 10px;
        }
        
        .slider::-webkit-slider-thumb {
          appearance: none;
          width: 20px;
          height: 20px;
          background: white;
          cursor: pointer;
          border-radius: 50%;
          box-shadow: 0 2px 6px rgba(0, 0, 0, 0.2);
        }
        
        .slider::-moz-range-thumb {
          width: 20px;
          height: 20px;
          background: white;
          cursor: pointer;
          border-radius: 50%;
          box-shadow: 0 2px 6px rgba(0, 0, 0, 0.2);
          border: none;
        }
      `}</style>
    </div>
  );
};

export default SettingsApp;