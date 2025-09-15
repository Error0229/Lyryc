import React, { useState, useEffect } from 'react';
import { motion } from 'framer-motion';
import { LyricLine, WordTiming } from '../stores/lyricsStore';
import { useLyricsStyleStore } from '../stores/lyricsStyleStore';
import { TranslatedLyricLine } from '../stores/translationStore';
import { useTranslationStore } from '../stores/translationStore';
import { useHiraganaStore } from '../stores/hiraganaStore';
import { invoke } from '@tauri-apps/api/core';
import { isJapanese, hasKanji, isLikelyJapanese } from '../utils/japaneseDetection';
import '../styles/ruby.css';

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
  const style = useLyricsStyleStore((state) => state.style);
  const { enabled, displayMode, showLanguageIndicator } = useTranslationStore();
  const { settings: hiraganaSettings } = useHiraganaStore();
  
  // Debug logging to check if style updates are received
  useEffect(() => {
    console.log('[CleanLyricDisplay] Style updated:', {
      fontFamily: style.fontFamily,
      fontSize: style.fontSize,
      textColor: style.textColor,
      fontWeight: style.fontWeight,
      timestamp: new Date().toLocaleTimeString()
    });
  }, [style]);
  
  // State for hiragana conversion
  const [hiraganaText, setHiraganaText] = useState<string>('');
  const [wordHiragana, setWordHiragana] = useState<Record<number, string>>({});
  const [furiganaData, setFuriganaData] = useState<Array<{text: string, reading?: string}>>([]);
  const [wordFuriganaData, setWordFuriganaData] = useState<Record<number, Array<{text: string, reading?: string}>>>({});
  const [isConverting, setIsConverting] = useState(false);
  
  // Convert text to hiragana when hiragana is enabled and text is Japanese
  useEffect(() => {
    const convertToHiragana = async () => {
      if (!hiraganaSettings.enabled) {
        setHiraganaText('');
        setWordHiragana({});
        setFuriganaData([]);
        setWordFuriganaData({});
        return;
      }

      const textToConvert = 'originalText' in line ? line.originalText : line.text;
      
      if (!isLikelyJapanese(textToConvert) || !hasKanji(textToConvert)) {
        setHiraganaText('');
        setWordHiragana({});
        setFuriganaData([]);
        setWordFuriganaData({});
        return;
      }

      setIsConverting(true);
      
      try {
        // Convert full text
        const furiganaResult = await invoke<{segments: Array<{text: string, reading?: string}>, html: string}>('generate_furigana_command', { text: textToConvert });
        const fullHiragana = furiganaResult.segments.map(segment => segment.reading || segment.text).join('');
        setHiraganaText(fullHiragana);

        // Generate furigana data for ruby display using autoruby
        if (hiraganaSettings.displayMode === 'ruby') {
          setFuriganaData(furiganaResult.segments);
        }

        // Convert individual words if available
        if (line.words && line.words.length > 0) {
          const wordHiraganaMap: Record<number, string> = {};
          const wordFuriganaMap: Record<number, Array<{text: string, reading?: string}>> = {};
          
          for (let i = 0; i < line.words.length; i++) {
            const word = line.words[i];
            if (isLikelyJapanese(word.word) && hasKanji(word.word)) {
              const wordResult = await invoke<{segments: Array<{text: string, reading?: string}>, html: string}>('generate_furigana_command', { text: word.word });
              wordHiraganaMap[i] = wordResult.segments.map(segment => segment.reading || segment.text).join('');
              
              if (hiraganaSettings.displayMode === 'ruby') {
                wordFuriganaMap[i] = wordResult.segments;
              }
            }
          }
          
          setWordHiragana(wordHiraganaMap);
          setWordFuriganaData(wordFuriganaMap);
        }
      } catch (error) {
        console.error('Failed to convert to hiragana:', error);
      } finally {
        setIsConverting(false);
      }
    };

    convertToHiragana();
  }, [line, hiraganaSettings.enabled]);
  
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

  // Helper function to render ruby tags
  const renderRuby = (furiganaData: Array<{text: string, reading?: string}>, additionalStyles?: React.CSSProperties) => {
    const rubyStyle = {
      fontSize: `${style.fontSize * hiraganaSettings.fontSize}rem`,
      color: hiraganaSettings.color,
      opacity: hiraganaSettings.opacity,
      lineHeight: 1,
    };

    const rubyClassName = hiraganaSettings.rubyPosition === 'top' ? 'ruby-top' : 'ruby-bottom';

    return (
      <span style={additionalStyles}>
        {furiganaData.map((part, index) => (
          part.reading ? (
            <ruby key={index} className={rubyClassName} style={{ fontSize: 'inherit' }}>
              {part.text}
              <rt style={rubyStyle}>{part.reading}</rt>
            </ruby>
          ) : (
            <span key={index}>{part.text}</span>
          )
        ))}
      </span>
    );
  };

  // Helper function to render text with hiragana
  const renderTextWithHiragana = (
    originalText: string, 
    hiragana: string, 
    furiganaData?: Array<{text: string, reading?: string}>, 
    additionalStyles?: React.CSSProperties
  ) => {
    if (!hiraganaSettings.enabled) {
      return originalText;
    }

    // Ruby mode - use furigana data if available
    if (hiraganaSettings.displayMode === 'ruby' && furiganaData && furiganaData.length > 0) {
      return renderRuby(furiganaData, additionalStyles);
    }

    // Replace mode
    if (hiraganaSettings.displayMode === 'replace' && hiragana) {
      return hiragana;
    }

    // Above/below modes
    if (!hiragana) {
      return originalText;
    }

    const hiraganaStyle = {
      fontSize: `${style.fontSize * hiraganaSettings.fontSize}rem`,
      color: hiraganaSettings.color,
      opacity: hiraganaSettings.opacity,
      lineHeight: 1.2,
    };

    return (
      <div 
        className="flex flex-col items-center"
        style={{ gap: `${hiraganaSettings.spacing}px` }}
      >
        {hiraganaSettings.displayMode === 'above' && (
          <div style={hiraganaStyle}>
            {hiragana}
          </div>
        )}
        <div style={additionalStyles}>
          {originalText}
        </div>
        {hiraganaSettings.displayMode === 'below' && (
          <div style={hiraganaStyle}>
            {hiragana}
          </div>
        )}
      </div>
    );
  };

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
              {renderTextWithHiragana(translatedLine.originalText, hiraganaText, furiganaData)}
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
                {renderTextWithHiragana(translatedLine.originalText, hiraganaText, furiganaData)}
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
                {renderTextWithHiragana(translatedLine.originalText, hiraganaText, furiganaData)}
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
          {renderTextWithHiragana(line.text, hiraganaText, furiganaData)}
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

            const wordHiraganaText = wordHiragana[wordIndex] || '';
            const wordFurigana = wordFuriganaData[wordIndex] || [];
            
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
                    {wordHiraganaText || wordFurigana.length > 0 ? (
                      // Render word with hiragana/furigana
                      renderTextWithHiragana(
                        wordTiming.word,
                        wordHiraganaText,
                        wordFurigana,
                        { color: `${style.futureWordColor}60` }
                      )
                    ) : (
                      <>
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
                      </>
                    )}
                  </span>
                ) : isPastWord ? (
                  <span style={{ color: style.pastWordColor }}>
                    {wordHiraganaText || wordFurigana.length > 0 ? 
                      renderTextWithHiragana(wordTiming.word, wordHiraganaText, wordFurigana, { color: style.pastWordColor }) :
                      wordTiming.word
                    }
                  </span>
                ) : (
                  <span style={{ color: style.futureWordColor }}>
                    {wordHiraganaText || wordFurigana.length > 0 ? 
                      renderTextWithHiragana(wordTiming.word, wordHiraganaText, wordFurigana, { color: style.futureWordColor }) :
                      wordTiming.word
                    }
                  </span>
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
        {hiraganaText || furiganaData.length > 0 ? (
          renderTextWithHiragana(line.text, hiraganaText, furiganaData)
        ) : (
          <>
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
          </>
        )}
      </div>
    </div>
  );
};

export default CleanLyricDisplay;