import React from 'react';
import { useHiraganaStore } from '../stores/hiraganaStore';

const HiraganaToggle: React.FC = () => {
  const { settings, toggleEnabled } = useHiraganaStore();

  return (
    <button
      onClick={toggleEnabled}
      className={`px-3 py-1 rounded-md text-sm font-medium transition-all duration-200 ${
        settings.enabled
          ? 'bg-blue-600 text-white hover:bg-blue-700'
          : 'bg-gray-700 text-gray-300 hover:bg-gray-600'
      }`}
      title={settings.enabled ? 'Disable hiragana readings' : 'Enable hiragana readings for Japanese kanji'}
    >
      あ
    </button>
  );
};

export default HiraganaToggle;