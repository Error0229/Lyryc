/**
 * Utility functions for detecting Japanese text
 */

// Unicode ranges for Japanese characters
const HIRAGANA_RANGE = /[\u3040-\u309F]/;
const KATAKANA_RANGE = /[\u30A0-\u30FF]/;
const KANJI_RANGE = /[\u4E00-\u9FAF]/;

// Japanese-specific punctuation and symbols
const JAPANESE_PUNCTUATION = /[\u3000-\u303F]/;

/**
 * Detects if text contains Japanese characters (hiragana/katakana only to avoid Chinese false positives)
 */
export const isJapanese = (text: string): boolean => {
  return HIRAGANA_RANGE.test(text) || KATAKANA_RANGE.test(text);
};

/**
 * Detects if text contains Kanji characters
 */
export const hasKanji = (text: string): boolean => {
  return KANJI_RANGE.test(text);
};

/**
 * More sophisticated Japanese detection that considers character combinations
 */
export const isLikelyJapanese = (text: string): boolean => {
  // Must have hiragana or katakana to be considered Japanese
  const hasKana = HIRAGANA_RANGE.test(text) || KATAKANA_RANGE.test(text);
  if (!hasKana) return false;

  // Calculate ratios
  const hiraganaChars = (text.match(/[\u3040-\u309F]/g) || []).length;
  const katakanaChars = (text.match(/[\u30A0-\u30FF]/g) || []).length;
  const kanjiChars = (text.match(/[\u4E00-\u9FAF]/g) || []).length;
  const totalChars = text.length;

  const kanaRatio = (hiraganaChars + katakanaChars) / totalChars;
  
  // Japanese text typically has a good mix of hiragana with some kanji/katakana
  // If it's mostly kanji with very little kana, it's more likely Chinese
  return kanaRatio > 0.3; // At least 30% should be kana for Japanese
};

/**
 * Detects if text is likely Chinese (has kanji but little to no kana)
 */
export const isLikelyChinese = (text: string): boolean => {
  const hasKanji = KANJI_RANGE.test(text);
  const hasKana = HIRAGANA_RANGE.test(text) || KATAKANA_RANGE.test(text);
  
  if (!hasKanji) return false;
  
  const kanjiChars = (text.match(/[\u4E00-\u9FAF]/g) || []).length;
  const kanaChars = (text.match(/[\u3040-\u309F\u30A0-\u30FF]/g) || []).length;
  const totalChars = text.length;
  
  const kanjiRatio = kanjiChars / totalChars;
  const kanaRatio = kanaChars / totalChars;
  
  // Chinese text is mostly kanji with very little or no kana
  return kanjiRatio > 0.5 && kanaRatio < 0.2;
};

/**
 * Calculates the percentage of Japanese kana characters in the text
 */
export const getJapaneseKanaRatio = (text: string): number => {
  if (!text.length) return 0;
  
  const kanaChars = text.match(/[\u3040-\u309F\u30A0-\u30FF]/g) || [];
  return kanaChars.length / text.length;
};