import React, { useState, useEffect } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { useTranslationStore } from '../stores/translationStore';
import { getSupportedLanguages, getTranslationStyleOptions } from '../stores/translationStore';

interface TranslationSettingsProps {
  className?: string;
}

const TranslationSettings: React.FC<TranslationSettingsProps> = ({ className = '' }) => {
  const {
    enabled,
    config,
    displayMode,
    showLanguageIndicator,
    isTranslating,
    translationError,
    setEnabled,
    setAutoTranslate,
    updateConfig,
    updateDisplayMode
  } = useTranslationStore();

  const [showSettings, setShowSettings] = useState(false);
  const [availableLanguages, setAvailableLanguages] = useState<[string, string][]>([]);

  // Load supported languages on mount
  useEffect(() => {
    getSupportedLanguages().then(setAvailableLanguages);
  }, []);

  const styleOptions = getTranslationStyleOptions();

  return (
    <div className={`relative ${className}`}>
      {/* Main Toggle with Settings */}
      <div className="flex items-center space-x-1">
        {/* Enable/Disable Toggle */}
        <button
          onClick={() => {
            const newEnabled = !enabled;
            setEnabled(newEnabled);
            setAutoTranslate(newEnabled); // Enable auto-translate when translation is enabled
          }}
          disabled={isTranslating}
          className={`
            relative inline-flex h-4 w-7 items-center rounded-full transition-all duration-200 text-xs
            ${enabled ? 'bg-blue-500' : 'bg-gray-600'}
            ${isTranslating ? 'opacity-50 cursor-not-allowed' : 'hover:scale-105'}
          `}
          title={enabled ? 'Disable Translation' : 'Enable Translation'}
        >
          <span
            className={`
              inline-block h-2.5 w-2.5 transform rounded-full bg-white transition-transform duration-200
              ${enabled ? 'translate-x-4' : 'translate-x-0.5'}
            `}
          />
          {/* Loading indicator */}
          {isTranslating && (
            <div className="absolute inset-0 flex items-center justify-center">
              <div className="w-1.5 h-1.5 border border-white border-t-transparent rounded-full animate-spin"></div>
            </div>
          )}
        </button>

        {/* Settings Button */}
        <button
          onClick={() => setShowSettings(!showSettings)}
          className={`
            w-4 h-4 rounded text-xs transition-all
            ${showSettings 
              ? 'bg-white/20 text-white' 
              : 'text-white/50 hover:text-white/80 hover:bg-white/10'
            }
          `}
          title="Translation Settings"
        >
          ⚙
        </button>

        {/* Status Indicators */}
        {enabled && (
          <>
            {/* Language Indicator */}
            {config.source_language && config.target_language && (
              <div className="text-xs text-white/60 font-mono bg-black/20 rounded px-1 py-0.5">
                {config.source_language.toUpperCase()} → {config.target_language.toUpperCase()}
              </div>
            )}

            {/* Error Indicator */}
            {translationError && (
              <div className="text-xs text-red-300" title={translationError}>
                ⚠️
              </div>
            )}
          </>
        )}
      </div>

      {/* Settings Panel */}
      <AnimatePresence>
        {showSettings && (
          <motion.div
            initial={{ opacity: 0, y: -5, scale: 0.9 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, y: -5, scale: 0.9 }}
            className="absolute top-full mt-1 right-0 w-56 bg-black/95 backdrop-blur-xl border border-white/20 rounded-lg p-2 shadow-xl z-50"
          >
            <div className="space-y-2 text-xs">
              {/* Language Settings */}
              <div className="space-y-1">
                <div className="text-white/70 font-medium">Languages</div>
                <div className="grid grid-cols-2 gap-1">
                  <div>
                    <label className="text-white/50 text-xs">From</label>
                    <select
                      value={config.source_language}
                      onChange={(e) => updateConfig({ source_language: e.target.value })}
                      className="w-full p-1 bg-white/10 border border-white/20 rounded text-white text-xs focus:outline-none focus:border-blue-400"
                    >
                      {availableLanguages.map(([code, name]) => (
                        <option key={code} value={code} className="bg-gray-800">
                          {name}
                        </option>
                      ))}
                    </select>
                  </div>
                  <div>
                    <label className="text-white/50 text-xs">To</label>
                    <select
                      value={config.target_language}
                      onChange={(e) => updateConfig({ target_language: e.target.value })}
                      className="w-full p-1 bg-white/10 border border-white/20 rounded text-white text-xs focus:outline-none focus:border-blue-400"
                    >
                      {availableLanguages.filter(([code]) => code !== 'auto').map(([code, name]) => (
                        <option key={code} value={code} className="bg-gray-800">
                          {name}
                        </option>
                      ))}
                    </select>
                  </div>
                </div>
              </div>

              {/* Translation Style */}
              <div className="space-y-1">
                <div className="text-white/70 font-medium">Style</div>
                <select
                  value={config.style}
                  onChange={(e) => updateConfig({ style: e.target.value as any })}
                  className="w-full p-1 bg-white/10 border border-white/20 rounded text-white text-xs focus:outline-none focus:border-blue-400"
                >
                  {Object.entries(styleOptions).map(([key, label]) => (
                    <option key={key} value={key} className="bg-gray-800">
                      {label}
                    </option>
                  ))}
                </select>
              </div>

              {/* Translation Options */}
              <div className="space-y-1">
                <div className="text-white/70 font-medium">Options</div>
                <div className="space-y-0.5">
                  <label className="flex items-center space-x-2">
                    <input
                      type="checkbox"
                      checked={config.preserve_rhyme}
                      onChange={(e) => updateConfig({ preserve_rhyme: e.target.checked })}
                      className="w-3 h-3 rounded bg-white/10 border-white/20"
                    />
                    <span className="text-white/60 text-xs">Preserve Rhyme</span>
                  </label>
                  <label className="flex items-center space-x-2">
                    <input
                      type="checkbox"
                      checked={config.preserve_meaning}
                      onChange={(e) => updateConfig({ preserve_meaning: e.target.checked })}
                      className="w-3 h-3 rounded bg-white/10 border-white/20"
                    />
                    <span className="text-white/60 text-xs">Preserve Meaning</span>
                  </label>
                  <label className="flex items-center space-x-2">
                    <input
                      type="checkbox"
                      checked={config.maintain_structure}
                      onChange={(e) => updateConfig({ maintain_structure: e.target.checked })}
                      className="w-3 h-3 rounded bg-white/10 border-white/20"
                    />
                    <span className="text-white/60 text-xs">Keep Structure</span>
                  </label>
                </div>
              </div>

              {/* Display Settings */}
              <div className="space-y-1 pt-1 border-t border-white/10">
                <div className="text-white/70 font-medium">Display</div>
                <div className="space-y-0.5">
                  <div className="flex items-center justify-between">
                    <span className="text-white/60 text-xs">Translation Size</span>
                    <div className="flex items-center space-x-1">
                      <button
                        onClick={() => updateDisplayMode({ 
                          fontSize: { 
                            ...displayMode.fontSize, 
                            translated: Math.max(0.5, displayMode.fontSize.translated - 0.05)
                          }
                        })}
                        className="w-4 h-4 bg-white/10 hover:bg-white/20 rounded text-white/60 text-xs flex items-center justify-center"
                      >
                        -
                      </button>
                      <span className="text-xs font-mono text-white/70 w-8 text-center">
                        {Math.round(displayMode.fontSize.translated * 100)}%
                      </span>
                      <button
                        onClick={() => updateDisplayMode({ 
                          fontSize: { 
                            ...displayMode.fontSize, 
                            translated: Math.min(1.5, displayMode.fontSize.translated + 0.05)
                          }
                        })}
                        className="w-4 h-4 bg-white/10 hover:bg-white/20 rounded text-white/60 text-xs flex items-center justify-center"
                      >
                        +
                      </button>
                    </div>
                  </div>
                  
                  <label className="flex items-center justify-between">
                    <span className="text-white/60 text-xs">Language Tags</span>
                    <input
                      type="checkbox"
                      checked={showLanguageIndicator}
                      onChange={(e) => useTranslationStore.setState({ showLanguageIndicator: e.target.checked })}
                      className="w-3 h-3 rounded bg-white/10 border-white/20"
                    />
                  </label>
                </div>
              </div>

              {/* Custom Prompt (if style is custom) */}
              {config.style === 'custom' && (
                <div className="space-y-1 pt-1 border-t border-white/10">
                  <div className="text-white/70 font-medium">Custom Prompt</div>
                  <textarea
                    value={config.custom_prompt || ''}
                    onChange={(e) => updateConfig({ custom_prompt: e.target.value })}
                    placeholder="Enter custom translation instructions..."
                    className="w-full p-1 bg-white/10 border border-white/20 rounded text-white text-xs focus:outline-none focus:border-blue-400 resize-none"
                    rows={2}
                  />
                </div>
              )}
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
};

export default TranslationSettings;