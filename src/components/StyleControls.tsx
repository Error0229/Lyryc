import React, { useState } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { useLyricsStyleStore } from '../stores/lyricsStyleStore';

interface StyleControlsProps {
  isOpen: boolean;
  onClose: () => void;
}

const StyleControls: React.FC<StyleControlsProps> = ({ isOpen, onClose }) => {
  const { style, updateStyle, applyPreset, getPresets, resetToDefault } = useLyricsStyleStore();
  const [activeTab, setActiveTab] = useState<'presets' | 'font' | 'colors' | 'effects' | 'layout'>('presets');
  
  const presets = getPresets();
  
  const tabs = [
    { id: 'presets', label: '🎨 Presets', icon: '🎨' },
    { id: 'font', label: '📝 Font', icon: '📝' },
    { id: 'colors', label: '🌈 Colors', icon: '🌈' },
    { id: 'effects', label: '✨ Effects', icon: '✨' },
    { id: 'layout', label: '📐 Layout', icon: '📐' },
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
          <h2 className="text-white text-lg font-medium">Lyrics Style</h2>
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
                      onChange={(e) => updateStyle({ fontFamily: e.target.value })}
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
                      onChange={(e) => updateStyle({ fontSize: parseFloat(e.target.value) })}
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
                        onChange={(e) => updateStyle({ [color.key]: e.target.value })}
                        className="w-12 h-8 rounded border border-white/20 bg-transparent"
                      />
                      <input
                        type="text"
                        value={color.value}
                        onChange={(e) => updateStyle({ [color.key]: e.target.value })}
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
            </AnimatePresence>
          </div>
        </div>
      </div>
    </motion.div>
  );
};

export default StyleControls;