import { LyricLine } from '../stores/lyricsStore';
import { LyricsAlignment, AlignmentConfig, AlignmentResult } from './alignment';

export interface AudioSyncConfig {
  enableAIAlignment: boolean;
  alignmentSensitivity: number;
  language: string;
  enableWordLevel: boolean;
  autoDetectLanguage: boolean;
  fallbackToOriginal: boolean;
}

export interface SyncResult {
  success: boolean;
  lyrics: LyricLine[];
  confidence: number;
  processingTime: number;
  method: 'original' | 'ai-aligned' | 'fallback';
  error?: string;
}

export class AudioSyncService {
  private alignment: LyricsAlignment | null = null;
  private config: AudioSyncConfig;
  private isProcessing = false;

  constructor(config: AudioSyncConfig = {
    enableAIAlignment: true,
    alignmentSensitivity: 0.7,
    language: 'auto',
    enableWordLevel: true,
    autoDetectLanguage: true,
    fallbackToOriginal: true
  }) {
    this.config = config;
    this.initializeAlignment();
  }

  private async initializeAlignment(): Promise<void> {
    try {
      const alignmentConfig: AlignmentConfig = {
        sensitivity: this.config.alignmentSensitivity,
        language: this.config.language === 'auto' ? 'en' : this.config.language,
        enableWordLevel: this.config.enableWordLevel,
        confidenceThreshold: 0.6
      };

      this.alignment = new LyricsAlignment(alignmentConfig);
    } catch (error) {
      console.error('Failed to initialize lyrics alignment:', error);
    }
  }

  async syncLyrics(
    lyrics: LyricLine[],
    audioUrl?: string,
    trackInfo?: { title: string; artist: string }
  ): Promise<SyncResult> {
    if (this.isProcessing) {
      return {
        success: false,
        lyrics,
        confidence: 0,
        processingTime: 0,
        method: 'fallback',
        error: 'Already processing another sync request'
      };
    }

    this.isProcessing = true;

    try {
      // If AI alignment is disabled, return original lyrics
      if (!this.config.enableAIAlignment || !this.alignment) {
        return {
          success: true,
          lyrics,
          confidence: 1.0,
          processingTime: 0,
          method: 'original'
        };
      }

      // Auto-detect language if enabled
      if (this.config.autoDetectLanguage && trackInfo) {
        const detectedLanguage = await this.detectLanguage(lyrics, trackInfo);
        if (detectedLanguage !== this.config.language) {
          await this.updateLanguage(detectedLanguage);
        }
      }

      // If we have audio URL, fetch and process the audio
      if (audioUrl) {
        const audioBuffer = await this.fetchAndProcessAudio(audioUrl);
        if (audioBuffer) {
          const alignmentResult = await this.alignment.alignLyrics(
            audioBuffer,
            lyrics,
            lyrics.map(l => l.time)
          );

          // Check if alignment is good enough
          if (alignmentResult.confidence >= 0.6) {
            return {
              success: true,
              lyrics: alignmentResult.alignedLyrics,
              confidence: alignmentResult.confidence,
              processingTime: alignmentResult.processingTime,
              method: 'ai-aligned'
            };
          }
        }
      }

      // Try to improve existing lyrics timing through analysis
      const improvedLyrics = await this.improveLyricsTiming(lyrics, trackInfo);
      
      return {
        success: true,
        lyrics: improvedLyrics,
        confidence: 0.8,
        processingTime: 0,
        method: improvedLyrics !== lyrics ? 'ai-aligned' : 'original'
      };

    } catch (error) {
      console.error('Audio sync failed:', error);
      
      if (this.config.fallbackToOriginal) {
        return {
          success: true,
          lyrics,
          confidence: 0.5,
          processingTime: 0,
          method: 'fallback',
          error: error instanceof Error ? error.message : 'Unknown error'
        };
      }

      return {
        success: false,
        lyrics,
        confidence: 0,
        processingTime: 0,
        method: 'fallback',
        error: error instanceof Error ? error.message : 'Unknown error'
      };
    } finally {
      this.isProcessing = false;
    }
  }

  private async fetchAndProcessAudio(audioUrl: string): Promise<AudioBuffer | null> {
    try {
      // For security reasons, we can't directly access audio from streaming services
      // This would work with local files or CORS-enabled audio sources
      const response = await fetch(audioUrl);
      const arrayBuffer = await response.arrayBuffer();
      
      const audioContext = new (window.AudioContext || (window as any).webkitAudioContext)();
      const audioBuffer = await audioContext.decodeAudioData(arrayBuffer);
      
      return audioBuffer;
    } catch (error) {
      console.warn('Could not fetch audio for analysis:', error);
      return null;
    }
  }

  private async detectLanguage(
    lyrics: LyricLine[], 
    trackInfo: { title: string; artist: string }
  ): Promise<string> {
    // Simple language detection based on text analysis
    const text = lyrics.map(l => l.text).join(' ').toLowerCase();
    const title = trackInfo.title.toLowerCase();
    const artist = trackInfo.artist.toLowerCase();
    const combinedText = `${text} ${title} ${artist}`;

    // Language patterns (simplified detection)
    const languagePatterns = {
      'ja': /[\u3040-\u309f\u30a0-\u30ff\u4e00-\u9faf]/,
      'ko': /[\uac00-\ud7af\u1100-\u11ff\u3130-\u318f]/,
      'zh': /[\u4e00-\u9fff]/,
      'ru': /[\u0400-\u04ff]/,
      'ar': /[\u0600-\u06ff]/,
      'hi': /[\u0900-\u097f]/,
      'th': /[\u0e00-\u0e7f]/,
      'es': /\b(el|la|los|las|un|una|de|del|y|con|en|por|para|que|es|son|está|están)\b/,
      'fr': /\b(le|la|les|un|une|de|du|des|et|avec|dans|par|pour|que|est|sont)\b/,
      'de': /\b(der|die|das|ein|eine|und|mit|in|von|für|dass|ist|sind|aber|oder)\b/,
      'it': /\b(il|la|lo|gli|le|un|una|di|del|della|e|con|in|per|che|è|sono)\b/,
      'pt': /\b(o|a|os|as|um|uma|de|do|da|e|com|em|por|para|que|é|são)\b/,
    };

    for (const [lang, pattern] of Object.entries(languagePatterns)) {
      if (pattern.test(combinedText)) {
        console.log(`Detected language: ${lang}`);
        return lang;
      }
    }

    // Default to English
    return 'en';
  }

  private async updateLanguage(language: string): Promise<void> {
    if (this.alignment) {
      this.alignment.updateConfig({ language });
      this.config.language = language;
    }
  }

  private async improveLyricsTiming(
    lyrics: LyricLine[], 
    trackInfo?: { title: string; artist: string }
  ): Promise<LyricLine[]> {
    // Implement timing improvements based on text analysis
    const improvedLyrics = [...lyrics];
    
    for (let i = 0; i < improvedLyrics.length; i++) {
      const line = improvedLyrics[i];
      
      // Adjust timing based on text length and complexity
      const textLength = line.text.length;
      const wordCount = line.text.split(/\s+/).length;
      const complexity = this.calculateTextComplexity(line.text);
      
      // Estimate more accurate duration
      const estimatedDuration = this.estimateDuration(textLength, wordCount, complexity);
      
      // Adjust timing if it seems off
      if (line.duration && Math.abs(line.duration - estimatedDuration) > 1000) {
        improvedLyrics[i] = {
          ...line,
          duration: estimatedDuration
        };
      }

      // Generate word-level timing if enabled and not present
      if (this.config.enableWordLevel && !line.words && wordCount > 1) {
        improvedLyrics[i] = {
          ...improvedLyrics[i],
          words: this.generateWordTiming(line.text, line.time, estimatedDuration)
        };
      }
    }

    return improvedLyrics;
  }

  private calculateTextComplexity(text: string): number {
    // Simple complexity scoring based on various factors
    let complexity = 0;
    
    // Length factor
    complexity += Math.min(text.length / 100, 1);
    
    // Word count factor
    const wordCount = text.split(/\s+/).length;
    complexity += Math.min(wordCount / 20, 1);
    
    // Special characters (indicates non-English or complex words)
    const specialChars = text.match(/[^\w\s]/g)?.length || 0;
    complexity += Math.min(specialChars / text.length, 0.5);
    
    // Syllable estimation (rough)
    const vowels = text.match(/[aeiouAEIOU]/g)?.length || 0;
    const syllableEstimate = Math.max(vowels * 0.8, wordCount);
    complexity += Math.min(syllableEstimate / 30, 1);
    
    return Math.min(complexity, 2); // Cap at 2
  }

  private estimateDuration(textLength: number, wordCount: number, complexity: number): number {
    // Base duration calculation (average reading/singing speed)
    const baseWordsPerMinute = 150; // Average speaking rate
    const singingMultiplier = 0.7; // Singing is typically slower
    
    const adjustedWPM = baseWordsPerMinute * singingMultiplier * (2 - complexity);
    const estimatedMinutes = wordCount / adjustedWPM;
    const estimatedMs = estimatedMinutes * 60 * 1000;
    
    // Add minimum and maximum bounds
    const minDuration = Math.max(wordCount * 300, 1000); // Min 300ms per word, 1s minimum
    const maxDuration = wordCount * 2000; // Max 2s per word
    
    return Math.max(minDuration, Math.min(maxDuration, estimatedMs));
  }

  private generateWordTiming(text: string, startTime: number, duration: number): Array<{ start: number; end: number; word: string }> {
    const words = text.split(/\s+/).filter(w => w.length > 0);
    const wordTimings: Array<{ start: number; end: number; word: string }> = [];
    
    if (words.length === 0) return wordTimings;
    
    // Calculate relative durations based on word length and complexity
    const wordWeights = words.map(word => {
      const lengthWeight = word.length;
      const complexityWeight = (word.match(/[^\w]/g)?.length || 0) * 0.5;
      return lengthWeight + complexityWeight + 1; // Base weight of 1
    });
    
    const totalWeight = wordWeights.reduce((sum, weight) => sum + weight, 0);
    let currentTime = startTime;
    
    for (let i = 0; i < words.length; i++) {
      const wordDuration = (wordWeights[i] / totalWeight) * duration;
      const endTime = currentTime + wordDuration;
      
      wordTimings.push({
        start: Math.round(currentTime),
        end: Math.round(endTime),
        word: words[i]
      });
      
      currentTime = endTime;
    }
    
    return wordTimings;
  }

  // Public configuration methods
  updateConfig(newConfig: Partial<AudioSyncConfig>): void {
    this.config = { ...this.config, ...newConfig };
    
    if (this.alignment && (
      newConfig.alignmentSensitivity !== undefined ||
      newConfig.language !== undefined ||
      newConfig.enableWordLevel !== undefined
    )) {
      this.alignment.updateConfig({
        sensitivity: this.config.alignmentSensitivity,
        language: this.config.language === 'auto' ? 'en' : this.config.language,
        enableWordLevel: this.config.enableWordLevel
      });
    }
  }

  getConfig(): AudioSyncConfig {
    return { ...this.config };
  }

  getSupportedLanguages(): string[] {
    return this.alignment?.getSupportedLanguages() || ['en'];
  }

  isProcessingSync(): boolean {
    return this.isProcessing;
  }

  async destroy(): Promise<void> {
    if (this.alignment) {
      await this.alignment.destroy();
    }
  }
}