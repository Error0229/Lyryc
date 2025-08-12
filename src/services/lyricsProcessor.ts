import { LyricLine, WordTiming } from '../stores/lyricsStore';
import { AudioSyncService, AudioSyncConfig, SyncResult } from './audioSync';
import { invoke } from '@tauri-apps/api/core';

export interface ProcessorConfig {
  enableAIAlignment: boolean;
  enableWordLevel: boolean;
  language: string;
  confidenceThreshold: number;
  fallbackToOriginal: boolean;
}

export interface ProcessedLyrics {
  lyrics: LyricLine[];
  confidence: number;
  method: 'original' | 'enhanced' | 'ai-aligned';
  processingTime: number;
  language: string;
  hasWordTimings: boolean;
}

export class LyricsProcessor {
  private audioSyncService: AudioSyncService;
  private config: ProcessorConfig;

  constructor(config: ProcessorConfig = {
    enableAIAlignment: true,
    enableWordLevel: true,
    language: 'auto',
    confidenceThreshold: 0.6,
    fallbackToOriginal: true
  }) {
    this.config = config;

    const syncConfig: AudioSyncConfig = {
      enableAIAlignment: config.enableAIAlignment,
      alignmentSensitivity: 0.7,
      language: config.language,
      enableWordLevel: config.enableWordLevel,
      autoDetectLanguage: config.language === 'auto',
      fallbackToOriginal: config.fallbackToOriginal
    };

    this.audioSyncService = new AudioSyncService(syncConfig);
  }

  async processTrackLyrics(
    trackName: string,
    artistName: string,
    audioUrl?: string,
    abortSignal?: AbortSignal
  ): Promise<ProcessedLyrics> {
    const startTime = performance.now();

    try {
      console.log(`[LyricsProcessor] Processing lyrics for: "${trackName}" by "${artistName}"`);

      // Check if cancelled before starting
      if (abortSignal?.aborted) {
        throw new Error('Request was cancelled');
      }

      // Step 1: Fetch lyrics from backend (Tauri)
      const lrclibLyrics = await invoke('fetch_lyrics', {
        trackName: trackName,
        artistName: artistName
      }) as LyricLine[];

      // Check if cancelled after LRCLib fetch
      if (abortSignal?.aborted) {
        throw new Error('Request was cancelled');
      }

      console.log(`[LyricsProcessor] Backend returned ${lrclibLyrics?.length || 0} lyrics lines`);

      if (!lrclibLyrics || lrclibLyrics.length === 0) {
        console.log('[LyricsProcessor] No lyrics found from backend, returning empty result');
        return {
          lyrics: [],
          confidence: 0,
          method: 'original',
          processingTime: performance.now() - startTime,
          language: this.config.language,
          hasWordTimings: false
        };
      }

      // Step 2: Enhance lyrics with AI alignment if enabled
      let processedLyrics = lrclibLyrics;
      let confidence = 0.8; // Base confidence for LRCLIB data
      let method: 'original' | 'enhanced' | 'ai-aligned' = 'original';

      if (this.config.enableAIAlignment) {
        const syncResult = await this.audioSyncService.syncLyrics(
          lrclibLyrics,
          audioUrl,
          { title: trackName, artist: artistName }
        );

        if (syncResult.success && syncResult.confidence >= this.config.confidenceThreshold) {
          processedLyrics = syncResult.lyrics;
          confidence = syncResult.confidence;
          method = syncResult.method as 'original' | 'enhanced' | 'ai-aligned';
        }
      }

      // Step 3: Generate word-level timings if not present
      if (this.config.enableWordLevel) {
        processedLyrics = await this.ensureWordLevelTimings(processedLyrics);
      }

      // Step 4: Apply language-specific enhancements
      processedLyrics = await this.applyLanguageEnhancements(
        processedLyrics,
        this.config.language
      );

      return {
        lyrics: processedLyrics,
        confidence,
        method,
        processingTime: performance.now() - startTime,
        language: this.config.language,
        hasWordTimings: processedLyrics.some(line => line.words && line.words.length > 0)
      };

    } catch (error) {
      console.error('Error processing track lyrics:', error);

      return {
        lyrics: [],
        confidence: 0,
        method: 'original',
        processingTime: performance.now() - startTime,
        language: this.config.language,
        hasWordTimings: false
      };
    }
  }

  private async ensureWordLevelTimings(lyrics: LyricLine[]): Promise<LyricLine[]> {
    const enhancedLyrics: LyricLine[] = [];

    for (const line of lyrics) {
      if (line.words && line.words.length > 0) {
        // Already has word timings
        enhancedLyrics.push(line);
      } else {
        // Generate word timings
        const words = this.generateWordTimings(line);
        enhancedLyrics.push({
          ...line,
          words
        });
      }
    }

    return enhancedLyrics;
  }

  private generateWordTimings(line: LyricLine): WordTiming[] {
    const words = line.text.split(/\s+/).filter(w => w.length > 0);
    const wordTimings: WordTiming[] = [];

    if (words.length === 0) return wordTimings;

    // Use seconds consistently. Default line duration: 3s
    const lineDuration = line.duration ?? 3;

    // Calculate relative durations based on word characteristics
    const wordWeights = words.map(word => this.calculateWordWeight(word));
    const totalWeight = wordWeights.reduce((sum, weight) => sum + weight, 0);

    let currentTime = line.time;

    // First pass: compute base segments
    const baseSegments: Array<{ start: number; end: number; word: string }> = [];
    for (let i = 0; i < words.length; i++) {
      const wordDuration = (wordWeights[i] / totalWeight) * lineDuration;
      const endTime = currentTime + wordDuration;
      baseSegments.push({ start: currentTime, end: endTime, word: words[i] });
      currentTime = endTime;
    }

    // Second pass: apply language adjustments to durations, then normalize back to fit line
    const adjusted: Array<{ start: number; end: number; word: string }> = [];
    for (let i = 0; i < baseSegments.length; i++) {
      const seg = baseSegments[i];
      const adj = this.adjustWordTiming(seg.word, seg.start, seg.end, this.config.language);
      adjusted.push({ start: adj.start, end: adj.end, word: seg.word });
    }

    // Normalize
    if (adjusted.length > 0) {
      const total = adjusted[adjusted.length - 1].end - adjusted[0].start;
      const scale = total > 0 ? lineDuration / total : 1;
      const start0 = adjusted[0].start;
      let prevEnd = line.time;
      for (let i = 0; i < adjusted.length; i++) {
        const relStart = adjusted[i].start - start0;
        const relEnd = adjusted[i].end - start0;
        let s = line.time + relStart * scale;
        let e = line.time + relEnd * scale;
        s = Math.max(prevEnd, s);
        e = Math.max(s, Math.min(e, line.time + lineDuration));
        wordTimings.push({ start: s, end: e, word: words[i] });
        prevEnd = e;
      }
      // Ensure last word ends exactly at line end
      wordTimings[wordTimings.length - 1].end = line.time + lineDuration;
    }

    return wordTimings;
  }

  private calculateWordWeight(word: string): number {
    let weight = 1; // Base weight

    // Length factor
    weight += word.length * 0.1;

    // Syllable estimation (rough)
    const vowels = word.match(/[aeiouAEIOU]/g)?.length || 0;
    const consonantClusters = word.match(/[bcdfghjklmnpqrstvwxyzBCDFGHJKLMNPQRSTVWXYZ]{2,}/g)?.length || 0;
    weight += vowels * 0.3 + consonantClusters * 0.2;

    // Special characters (indicate complex words)
    const specialChars = word.match(/[^\w]/g)?.length || 0;
    weight += specialChars * 0.1;

    // Common short words get less time
    const shortWords = ['a', 'an', 'the', 'in', 'on', 'at', 'to', 'of', 'and', 'or', 'but'];
    if (shortWords.includes(word.toLowerCase())) {
      weight *= 0.7;
    }

    return Math.max(weight, 0.3); // Minimum weight
  }

  private adjustWordTiming(
    word: string,
    startTime: number,
    endTime: number,
    language: string
  ): { start: number; end: number } {
    // Language-specific timing adjustments
    const adjustments = this.getLanguageTimingAdjustments(language);

    let duration = endTime - startTime;

    // Apply language-specific multipliers
    if (this.isComplexWord(word, language)) {
      duration *= adjustments.complexWordMultiplier;
    }

    if (this.isPunctuatedWord(word)) {
      duration *= adjustments.punctuationMultiplier;
    }

    // Ensure minimum and maximum durations (seconds)
    duration = Math.max(
      adjustments.minWordDuration,
      Math.min(adjustments.maxWordDuration, duration)
    );

    return {
      start: startTime,
      end: startTime + duration
    };
  }

  private getLanguageTimingAdjustments(language: string) {
    const defaults = {
      complexWordMultiplier: 1.2,
      punctuationMultiplier: 1.1,
      // seconds
      minWordDuration: 0.15,
      maxWordDuration: 4.0,
    };

    switch (language) {
      case 'ja': // Japanese
        return {
          ...defaults,
          complexWordMultiplier: 1.4, // Kanji takes longer
          minWordDuration: 0.3
        };
      case 'zh': // Chinese
        return {
          ...defaults,
          complexWordMultiplier: 1.3,
          minWordDuration: 0.25
        };
      case 'de': // German
        return {
          ...defaults,
          complexWordMultiplier: 1.5, // Compound words
          maxWordDuration: 3.0
        };
      case 'fi': // Finnish
        return {
          ...defaults,
          complexWordMultiplier: 1.4,
          maxWordDuration: 2.5
        };
      default:
        return defaults;
    }
  }

  private isComplexWord(word: string, language: string): boolean {
    // Language-specific complexity detection
    switch (language) {
      case 'ja':
        return /[\u4e00-\u9faf]/.test(word); // Contains Kanji
      case 'zh':
        return /[\u4e00-\u9fff]/.test(word); // Contains Chinese characters
      case 'de':
        return word.length > 8; // Likely compound word
      case 'ru':
        return word.length > 7; // Complex Russian words
      default:
        return word.length > 6 || /[^a-zA-Z0-9\s]/.test(word);
    }
  }

  private isPunctuatedWord(word: string): boolean {
    return /[.,!?;:()[\]{}"""''']/.test(word);
  }

  private async applyLanguageEnhancements(
    lyrics: LyricLine[],
    language: string
  ): Promise<LyricLine[]> {
    // Apply language-specific enhancements
    return lyrics.map(line => ({
      ...line,
      text: this.enhanceTextForLanguage(line.text, language),
      words: line.words?.map(word => ({
        ...word,
        word: this.enhanceTextForLanguage(word.word, language)
      }))
    }));
  }

  private enhanceTextForLanguage(text: string, language: string): string {
    // Language-specific text processing
    switch (language) {
      case 'ja':
        // Add spacing for better readability
        return text.replace(/([ひらがな])([カタカナ])/g, '$1 $2');
      case 'zh':
        // Add subtle spacing for Chinese text
        return text.replace(/([。！？])([^。！？])/g, '$1 $2');
      default:
        return text;
    }
  }

  // Public configuration methods
  updateConfig(newConfig: Partial<ProcessorConfig>): void {
    this.config = { ...this.config, ...newConfig };

    // Update audio sync service config
    this.audioSyncService.updateConfig({
      enableAIAlignment: this.config.enableAIAlignment,
      language: this.config.language,
      enableWordLevel: this.config.enableWordLevel,
      fallbackToOriginal: this.config.fallbackToOriginal
    });
  }

  getConfig(): ProcessorConfig {
    return { ...this.config };
  }

  getSupportedLanguages(): string[] {
    return this.audioSyncService.getSupportedLanguages();
  }

  async destroy(): Promise<void> {
    if (this.audioSyncService) {
      await this.audioSyncService.destroy();
    }
  }
}
