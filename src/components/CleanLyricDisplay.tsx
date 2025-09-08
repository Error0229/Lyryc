import React from 'react';
import { motion } from 'framer-motion';
import { LyricLine, WordTiming } from '../stores/lyricsStore';
import { useLyricsStyleStore } from '../stores/lyricsStyleStore';
import { TranslatedLyricLine } from '../stores/translationStore';
import { useTranslationStore } from '../stores/translationStore';

interface CleanLyricDisplayProps {
  line: LyricLine | TranslatedLyricLine;
  currentTime: number;
  adjustedTime: number;
  fontFamily: string;
  viewMode?: 'highlighted' | 'plain';
}

const CleanLyricDisplay: React.FC<CleanLyricDisplayProps> = ({
  line,
  currentTime,
  adjustedTime,
  fontFamily, // Keep for backward compatibility, but will be overridden by style store
  viewMode = 'highlighted'
}) => {
  const { style } = useLyricsStyleStore();
  const { enabled, displayMode, showLanguageIndicator } = useTranslationStore();
  
  // Calculate dynamic line height based on content type and display mode
  const calculateLineHeight = (isTranslated: boolean, mode?: string): number => {
    const baseFontSize = style.fontSize;
    const animationBuffer = style.animationIntensity > 0 ? 0.2 : 0; // Extra space for animations
    
    if (!isTranslated) {
      // Single language - use standard line height with animation buffer
      return style.lineHeight + animationBuffer;
    }
    
    // For bilingual display, we need to account for:
    // 1. Two lines of text stacked vertically
    // 2. Different font sizes and custom spacing from displayMode
    // 3. User-defined spacing between lines
    // 4. Animation buffer for movement effects
    
    switch (displayMode.mode) {
      case 'bilingual':
        // Stack original and translated vertically
        // Calculate based on actual font sizes used
        const originalFontSize = baseFontSize * displayMode.fontSize.original;
        const translatedFontSize = baseFontSize * displayMode.fontSize.translated;
        
        const originalHeight = originalFontSize * 1.2;
        const translatedHeight = translatedFontSize * 1.2;
        const userSpacing = displayMode.spacing / 16; // Convert px to rem (assuming 16px base)
        const breathingRoom = 0.3 + animationBuffer; // Base space + animation buffer
        
        return (originalHeight + translatedHeight + userSpacing + breathingRoom) / baseFontSize;
        
      case 'side-by-side':
        // Horizontal layout - use max of both font sizes with animation buffer
        const maxFontMultiplier = Math.max(displayMode.fontSize.original, displayMode.fontSize.translated);
        return Math.max(style.lineHeight * maxFontMultiplier, 1.4) + animationBuffer;
        
      case 'original':
        // Single line display with original font size
        return (style.lineHeight * displayMode.fontSize.original) + animationBuffer;
        
      case 'translated':
        // Single line display with translated font size  
        return (style.lineHeight * displayMode.fontSize.translated) + animationBuffer;
        
      default:
        // Fallback bilingual mode
        return Math.max(style.lineHeight * 1.8, 2.2) + animationBuffer;
    }
  };
  
  // Check if this is a translated line for dynamic calculations
  const isTranslatedLine = (line: LyricLine | TranslatedLyricLine): line is TranslatedLyricLine => {
    return 'translatedText' in line;
  };

  const isTranslated = enabled && isTranslatedLine(line);
  const dynamicLineHeight = calculateLineHeight(isTranslated, displayMode.mode);

  // Create dynamic styles based on user preferences
  const baseTextStyle = {
    fontFamily: style.fontFamily,
    fontSize: `${style.fontSize}rem`,
    fontWeight: style.fontWeight,
    color: style.textColor,
    lineHeight: dynamicLineHeight,
    letterSpacing: `${style.letterSpacing}em`,
    textAlign: style.textAlign as 'left' | 'center' | 'right',
    textShadow: style.textShadow 
      ? `0 0 ${style.textShadowBlur}px ${style.textShadowColor}` 
      : 'none',
    filter: style.textGlow 
      ? `drop-shadow(0 0 10px ${style.textGlowColor})` 
      : 'none',
  };

  const backgroundStyle = style.backgroundOpacity > 0 ? {
    backgroundColor: `${style.backgroundColor}${Math.round(style.backgroundOpacity * 2.55).toString(16).padStart(2, '0')}`,
    backdropFilter: style.backgroundBlur > 0 ? `blur(${style.backgroundBlur}px)` : 'none',
    borderRadius: '12px',
    padding: '16px 24px',
  } : {};

  // Enhanced bilingual display for translated lines with proper layout handling
  if (isTranslated) {
    const translatedLine = line as TranslatedLyricLine;
    
    // Apply display mode customization from translation store
    const originalStyle = {
      ...baseTextStyle,
      fontSize: `${style.fontSize * displayMode.fontSize.original}rem`,
      color: displayMode.colors.original,
      opacity: displayMode.opacity.original,
      lineHeight: 1.2, // Override for individual text elements within container
    };
    
    const translatedStyle = {
      ...baseTextStyle,
      fontSize: `${style.fontSize * displayMode.fontSize.translated}rem`,
      color: displayMode.colors.translated,
      opacity: displayMode.opacity.translated,
      lineHeight: 1.2, // Override for individual text elements within container
    };

    // Handle different display modes
    const renderBilingualContent = () => {
      switch (displayMode.mode) {
        case 'original':
          return (
            <div style={originalStyle}>
              {translatedLine.originalText}
            </div>
          );
          
        case 'translated':
          return translatedLine.translatedText && (
            <div style={translatedStyle}>
              {translatedLine.translatedText}
            </div>
          );
          
        case 'side-by-side':
          return (
            <div className="flex items-center justify-center gap-4">
              <div style={originalStyle} className="flex-1 text-right">
                {translatedLine.originalText}
              </div>
              <div className="text-white/30">•</div>
              {translatedLine.translatedText && (
                <div style={translatedStyle} className="flex-1 text-left">
                  {translatedLine.translatedText}
                </div>
              )}
            </div>
          );
          
        case 'bilingual':
        default:
          // Apply animation settings if enabled
          const animationProps = displayMode.animation.enabled ? {
            transition: `all ${displayMode.animation.duration}ms ease-in-out`,
            transform: style.animationIntensity > 0 ? `scale(${1 + style.animationIntensity / 1000})` : undefined,
          } : {};

          return (
            <div 
              className="transition-all duration-300"
              style={{ 
                textAlign: style.textAlign,
                gap: `${displayMode.spacing}px`,
                display: 'flex',
                flexDirection: 'column',
                alignItems: style.textAlign === 'center' ? 'center' : 
                          style.textAlign === 'right' ? 'flex-end' : 'flex-start',
                ...animationProps
              }}
            >
              {/* Original text */}
              <div style={originalStyle}>
                {translatedLine.originalText}
              </div>
              
              {/* Translated text (if available and different) */}
              {translatedLine.translatedText && translatedLine.translatedText !== translatedLine.originalText && (
                <div style={translatedStyle}>
                  {translatedLine.translatedText}
                </div>
              )}
            </div>
          );
      }
    };

    return (
      <div style={{ ...backgroundStyle }}>
        {renderBilingualContent()}
      </div>
    );
  }

  // If plain mode, just render the text without any highlighting
  if (viewMode === 'plain') {
    return (
      <div style={{ ...backgroundStyle }}>
        <div style={baseTextStyle}>
          {line.text}
        </div>
      </div>
    );
  }
  // Calculate word progress for clean word-by-word highlighting
  const calculateWordProgress = (word: WordTiming, currentTime: number): number => {
    if (currentTime < word.start) return 0;
    if (currentTime >= word.end) return 1;
    
    const wordDuration = word.end - word.start;
    if (wordDuration <= 0) return 1;
    
    return (currentTime - word.start) / wordDuration;
  };

  // Get current word index based on timing
  const getCurrentWordIndex = (words: WordTiming[], currentTime: number): number => {
    for (let i = 0; i < words.length; i++) {
      const word = words[i];
      if (currentTime >= word.start && currentTime < word.end) {
        return i;
      }
    }
    
    // If we're past all words, highlight the last one
    if (currentTime >= words[words.length - 1].end) {
      return words.length - 1;
    }
    
    return -1;
  };

  // Render with word-level highlighting if available
  if (line.words && line.words.length > 0) {
    const currentWordIndex = getCurrentWordIndex(line.words, adjustedTime);
    
    // Debug offset application
    if (Math.abs(currentTime - adjustedTime) > 0.1) {
      console.log(`[CleanLyricDisplay] Offset active: currentTime=${currentTime.toFixed(2)}s, adjustedTime=${adjustedTime.toFixed(2)}s, offset=${(adjustedTime - currentTime).toFixed(2)}s`);
    }
    
    return (
      <div style={{ ...backgroundStyle }}>
        <div 
          className="inline-flex flex-wrap gap-2"
          style={{ justifyContent: style.textAlign }}
        >
          {line.words.map((wordTiming, wordIndex) => {
            const isPastWord = wordIndex < currentWordIndex;
            const isCurrentWord = wordIndex === currentWordIndex;
            const wordProgress = isCurrentWord ? calculateWordProgress(wordTiming, adjustedTime) : 0;
            
            const animationScale = isCurrentWord ? 1 + (style.animationIntensity / 1000) : 1;
            const animationY = isCurrentWord ? -(style.animationIntensity / 25) : 0;

            return (
              <motion.span
                key={wordIndex}
                className="relative transition-all duration-200"
                animate={{
                  scale: animationScale,
                  y: animationY,
                }}
                transition={{ duration: 0.2, ease: "easeOut" }}
                style={baseTextStyle}
              >
                {isCurrentWord ? (
                  <span className="relative">
                    {/* Background word */}
                    <span style={{ color: `${style.futureWordColor}60` }}>{wordTiming.word}</span>
                    {/* Progressive highlight */}
                    <span
                      className="absolute inset-0 overflow-hidden"
                      style={{
                        color: style.highlightColor,
                        clipPath: `inset(0 ${Math.max(0, (1 - wordProgress) * 100)}% 0 0)`,
                        transition: wordProgress > 0.05 ? 'clip-path 0.1s ease-out' : 'none',
                        filter: style.textGlow 
                          ? `drop-shadow(0 0 10px ${style.textGlowColor})` 
                          : 'none',
                      }}
                    >
                      {wordTiming.word}
                    </span>
                  </span>
                ) : isPastWord ? (
                  <span style={{ color: style.pastWordColor }}>{wordTiming.word}</span>
                ) : (
                  <span style={{ color: style.futureWordColor }}>{wordTiming.word}</span>
                )}
              </motion.span>
            );
          })}
        </div>
      </div>
    );
  }

  // Fallback to simple line highlighting
  const lineProgress = Math.min((adjustedTime - line.time) / 3, 1); // Assume 3s duration

  return (
    <div style={{ ...backgroundStyle }}>
      <div className="relative" style={baseTextStyle}>
        {/* Background text */}
        <div style={{ color: `${style.futureWordColor}60` }}>{line.text}</div>
        {/* Progressive highlight */}
        <div
          className="absolute inset-0 overflow-hidden transition-all duration-200"
          style={{
            color: style.highlightColor,
            clipPath: `inset(0 ${(1 - lineProgress) * 100}% 0 0)`,
            filter: style.textGlow 
              ? `drop-shadow(0 0 10px ${style.textGlowColor})` 
              : 'none',
          }}
        >
          {line.text}
        </div>
      </div>
    </div>
  );
};

export default CleanLyricDisplay;