import { invoke } from '@tauri-apps/api/core';
import { isLikelyJapanese, hasKanji } from '../utils/japaneseDetection';

interface FuriganaSegment {
  text: string;
  reading?: string;
}

interface FuriganaResult {
  segments: FuriganaSegment[];
  html: string;
}

// Autoruby-based Japanese processing service using Rust backend
export class KuroshiroService {
  private isInitialized = true; // No initialization needed

  /**
   * Initialize method for compatibility (no-op since we use Rust backend)
   */
  async initialize(): Promise<void> {
    console.log('Autoruby Japanese processor ready');
  }

  /**
   * Convert Japanese text to hiragana readings using autoruby
   */
  async convertToHiragana(text: string): Promise<string> {
    if (!isLikelyJapanese(text)) {
      return text; // Return original text if not Japanese
    }

    try {
      const result = await invoke<FuriganaResult>('generate_furigana_command', { text });
      // Extract just the hiragana readings from segments
      return result.segments.map(segment => segment.reading || segment.text).join('');
    } catch (error) {
      console.error('Failed to convert to hiragana:', error);
      return text; // Return original text on error
    }
  }

  /**
   * Convert Japanese text to furigana using autoruby
   */
  async convertToFurigana(text: string): Promise<string> {
    if (!isLikelyJapanese(text)) {
      return text;
    }

    if (!hasKanji(text)) {
      return text;
    }

    try {
      const result = await invoke<FuriganaResult>('generate_furigana_command', { text });
      return result.html; // Return the HTML with ruby tags
    } catch (error) {
      console.error('Failed to generate furigana:', error);
      return text; // Return original text on error
    }
  }

  /**
   * Parse furigana text and convert to structured data for ruby rendering
   */
  parseFuriganaToRuby(furiganaText: string): Array<{text: string, reading?: string}> {
    // Since autoruby already provides structured data, we'll get it directly from the service
    // This method is kept for backward compatibility but will be replaced
    const segments: Array<{text: string, reading?: string}> = [];
    
    // Parse HTML ruby tags if present
    const rubyRegex = /<ruby>([^<]+)<rt>([^<]+)<\/rt><\/ruby>/g;
    let lastIndex = 0;
    let match;

    while ((match = rubyRegex.exec(furiganaText)) !== null) {
      // Add any text before this ruby tag
      if (match.index > lastIndex) {
        const beforeText = furiganaText.substring(lastIndex, match.index);
        if (beforeText) {
          segments.push({ text: beforeText });
        }
      }

      // Add the ruby segment
      segments.push({
        text: match[1], // kanji
        reading: match[2] // hiragana reading
      });

      lastIndex = match.index + match[0].length;
    }

    // Add any remaining text after the last ruby tag
    if (lastIndex < furiganaText.length) {
      const remainingText = furiganaText.substring(lastIndex);
      if (remainingText) {
        segments.push({ text: remainingText });
      }
    }

    // If no ruby tags found, treat as plain text
    if (segments.length === 0) {
      segments.push({ text: furiganaText });
    }

    return segments;
  }

  /**
   * Get furigana segments directly from autoruby (new method)
   */
  async getFuriganaSegments(text: string): Promise<Array<{text: string, reading?: string}>> {
    if (!isLikelyJapanese(text)) {
      return [{ text }];
    }

    try {
      const result = await invoke<FuriganaResult>('generate_furigana_command', { text });
      return result.segments;
    } catch (error) {
      console.error('Failed to get furigana segments:', error);
      return [{ text }]; // Return original text as single segment on error
    }
  }
}

// Create a singleton instance
export const kuroshiroService = new KuroshiroService();