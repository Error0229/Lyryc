import React from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { useTranslationStore } from '../stores/translationStore';

interface TranslationToggleProps {
  className?: string;
}

const TranslationToggle: React.FC<TranslationToggleProps> = ({ className = '' }) => {
  const { 
    enabled, 
    isTranslating, 
    translationError, 
    config,
    setEnabled 
  } = useTranslationStore();

  return (
    <div className={`flex items-center space-x-2 ${className}`}>
      {/* Main Toggle Button */}
      <button
        onClick={() => setEnabled(!enabled)}
        disabled={isTranslating}
        className={`
          relative inline-flex h-6 w-11 items-center rounded-full transition-all duration-200
          ${enabled ? 'bg-blue-500' : 'bg-gray-600'}
          ${isTranslating ? 'opacity-50 cursor-not-allowed' : 'hover:scale-105'}
        `}
        title={enabled ? 'Disable Translation' : 'Enable Translation'}
      >
        <span
          className={`
            inline-block h-4 w-4 transform rounded-full bg-white transition-transform duration-200
            ${enabled ? 'translate-x-6' : 'translate-x-1'}
          `}
        />
        {/* Loading indicator */}
        {isTranslating && (
          <div className="absolute inset-0 flex items-center justify-center">
            <div className="w-3 h-3 border-2 border-white border-t-transparent rounded-full animate-spin"></div>
          </div>
        )}
      </button>

      {/* Language Indicator */}
      {enabled && config.source_language && config.target_language && (
        <motion.div
          initial={{ opacity: 0, scale: 0.8 }}
          animate={{ opacity: 1, scale: 1 }}
          className="text-xs text-white/70 font-mono bg-black/20 rounded px-2 py-1 border border-white/10"
        >
          {config.source_language.toUpperCase()} → {config.target_language.toUpperCase()}
        </motion.div>
      )}

      {/* Translation Status */}
      {enabled && (
        <AnimatePresence>
          {isTranslating && (
            <motion.div
              initial={{ opacity: 0, x: -10 }}
              animate={{ opacity: 1, x: 0 }}
              exit={{ opacity: 0, x: -10 }}
              className="text-xs text-blue-300 animate-pulse"
            >
              Translating...
            </motion.div>
          )}
          
          {translationError && (
            <motion.div
              initial={{ opacity: 0, x: -10 }}
              animate={{ opacity: 1, x: 0 }}
              exit={{ opacity: 0, x: -10 }}
              className="text-xs text-red-300"
              title={translationError}
            >
              ⚠️ Error
            </motion.div>
          )}
        </AnimatePresence>
      )}
    </div>
  );
};

export default TranslationToggle;