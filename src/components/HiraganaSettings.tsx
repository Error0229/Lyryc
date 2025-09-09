import React from 'react';
import { useHiraganaStore } from '../stores/hiraganaStore';

interface HiraganaSettingsProps {
  isVisible: boolean;
}

const HiraganaSettings: React.FC<HiraganaSettingsProps> = ({ isVisible }) => {
  const { settings, updateSettings, resetToDefaults } = useHiraganaStore();

  if (!isVisible) return null;

  return (
    <div className="bg-gray-900 rounded-lg p-4 space-y-4 border border-gray-700">
      <div className="flex items-center justify-between">
        <h3 className="text-lg font-semibold text-white">Hiragana Settings</h3>
        <button
          onClick={resetToDefaults}
          className="px-3 py-1 text-sm bg-gray-700 text-gray-300 rounded hover:bg-gray-600 transition-colors"
        >
          Reset
        </button>
      </div>

      <div className="grid grid-cols-1 gap-4">
        {/* Display Mode */}
        <div>
          <label className="block text-sm font-medium text-gray-300 mb-2">
            Display Mode
          </label>
          <select
            value={settings.displayMode}
            onChange={(e) => updateSettings({ displayMode: e.target.value as 'above' | 'below' | 'replace' | 'ruby' })}
            className="w-full px-3 py-2 bg-gray-800 border border-gray-600 rounded-md text-white focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
          >
            <option value="ruby">Ruby text (furigana above kanji) - Recommended</option>
            <option value="above">Above original text</option>
            <option value="below">Below original text</option>
            <option value="replace">Replace original text</option>
          </select>
        </div>

        {/* Ruby Position (only show if ruby mode is selected) */}
        {settings.displayMode === 'ruby' && (
          <div>
            <label className="block text-sm font-medium text-gray-300 mb-2">
              Ruby Position
            </label>
            <select
              value={settings.rubyPosition}
              onChange={(e) => updateSettings({ rubyPosition: e.target.value as 'top' | 'bottom' })}
              className="w-full px-3 py-2 bg-gray-800 border border-gray-600 rounded-md text-white focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
            >
              <option value="top">Above text (traditional)</option>
              <option value="bottom">Below text</option>
            </select>
          </div>
        )}

        {/* Font Size */}
        <div>
          <label className="block text-sm font-medium text-gray-300 mb-2">
            Font Size: {Math.round(settings.fontSize * 100)}%
          </label>
          <input
            type="range"
            min="0.3"
            max="1.2"
            step="0.1"
            value={settings.fontSize}
            onChange={(e) => updateSettings({ fontSize: parseFloat(e.target.value) })}
            className="w-full h-2 bg-gray-700 rounded-lg appearance-none cursor-pointer"
          />
        </div>

        {/* Opacity */}
        <div>
          <label className="block text-sm font-medium text-gray-300 mb-2">
            Opacity: {Math.round(settings.opacity * 100)}%
          </label>
          <input
            type="range"
            min="0.1"
            max="1"
            step="0.1"
            value={settings.opacity}
            onChange={(e) => updateSettings({ opacity: parseFloat(e.target.value) })}
            className="w-full h-2 bg-gray-700 rounded-lg appearance-none cursor-pointer"
          />
        </div>

        {/* Color */}
        <div>
          <label className="block text-sm font-medium text-gray-300 mb-2">
            Color
          </label>
          <input
            type="color"
            value={settings.color}
            onChange={(e) => updateSettings({ color: e.target.value })}
            className="w-16 h-8 rounded border border-gray-600 cursor-pointer"
          />
        </div>

        {/* Spacing (only show if not replacing or using ruby) */}
        {settings.displayMode !== 'replace' && settings.displayMode !== 'ruby' && (
          <div>
            <label className="block text-sm font-medium text-gray-300 mb-2">
              Spacing: {settings.spacing}px
            </label>
            <input
              type="range"
              min="0"
              max="12"
              step="1"
              value={settings.spacing}
              onChange={(e) => updateSettings({ spacing: parseInt(e.target.value) })}
              className="w-full h-2 bg-gray-700 rounded-lg appearance-none cursor-pointer"
            />
          </div>
        )}
      </div>

      <div className="text-xs text-gray-400 border-t border-gray-700 pt-3">
        Hiragana readings will automatically appear for Japanese text containing kanji.
        <strong>Ruby text mode</strong> displays readings above kanji characters using HTML ruby tags for the most authentic furigana experience.
        Make sure to have a stable internet connection for the first use.
      </div>
    </div>
  );
};

export default HiraganaSettings;