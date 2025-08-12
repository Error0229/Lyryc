import { LyricLine, WordTiming } from '../stores/lyricsStore';

export interface AlignmentConfig {
  sensitivity: number; // 0.1 to 1.0
  language: string; // Language code for phoneme mapping
  enableWordLevel: boolean;
  confidenceThreshold: number; // 0.5 to 1.0
}

export interface AudioFeatures {
  mfcc: Float32Array[]; // Mel-frequency cepstral coefficients
  energy: Float32Array;
  spectralCentroid: Float32Array;
  zeroCrossingRate: Float32Array;
  frameRate: number; // Frames per second
}

export interface AlignmentResult {
  alignedLyrics: LyricLine[];
  confidence: number;
  processingTime: number;
}

export class LyricsAlignment {
  private audioContext: AudioContext | null = null;
  private analyzer: AnalyserNode | null = null;
  private config: AlignmentConfig;

  // Phoneme mappings for different languages
  private readonly phonemeMappings = {
    'en': this.getEnglishPhonemes(),
    'es': this.getSpanishPhonemes(),
    'fr': this.getFrenchPhonemes(),
    'de': this.getGermanPhonemes(),
    'ja': this.getJapanesePhonemes(),
    'ko': this.getKoreanPhonemes(),
    'zh': this.getChinesePhonemes(),
    'ru': this.getRussianPhonemes(),
    'ar': this.getArabicPhonemes(),
    'hi': this.getHindiPhonemes()
  };

  constructor(config: AlignmentConfig = {
    sensitivity: 0.7,
    language: 'en',
    enableWordLevel: true,
    confidenceThreshold: 0.6
  }) {
    this.config = config;
    this.initializeAudioContext();
  }

  private async initializeAudioContext(): Promise<void> {
    try {
      this.audioContext = new (window.AudioContext || (window as any).webkitAudioContext)();
      this.analyzer = this.audioContext.createAnalyser();
      this.analyzer.fftSize = 2048;
      this.analyzer.smoothingTimeConstant = 0.3;
    } catch (error) {
      console.error('Failed to initialize audio context:', error);
    }
  }

  async alignLyrics(
    audioBuffer: AudioBuffer, 
    lyrics: LyricLine[], 
    originalTimings?: number[]
  ): Promise<AlignmentResult> {
    const startTime = performance.now();
    
    try {
      // Extract audio features
      const audioFeatures = await this.extractAudioFeatures(audioBuffer);
      
      // Generate text features based on language
      const textFeatures = this.extractTextFeatures(lyrics, this.config.language);
      
      // Perform Dynamic Time Warping alignment
      const alignedLyrics = await this.performDTWAlignment(
        audioFeatures, 
        textFeatures, 
        lyrics, 
        originalTimings
      );
      
      // Calculate confidence score
      const confidence = this.calculateAlignmentConfidence(alignedLyrics, originalTimings);
      
      const processingTime = performance.now() - startTime;
      
      return {
        alignedLyrics,
        confidence,
        processingTime
      };
    } catch (error) {
      console.error('Alignment failed:', error);
      return {
        alignedLyrics: lyrics, // Return original lyrics if alignment fails
        confidence: 0,
        processingTime: performance.now() - startTime
      };
    }
  }

  private async extractAudioFeatures(audioBuffer: AudioBuffer): Promise<AudioFeatures> {
    const sampleRate = audioBuffer.sampleRate;
    const frameSize = 1024;
    const hopSize = 512;
    const frameRate = sampleRate / hopSize;
    
    const channelData = audioBuffer.getChannelData(0); // Use first channel
    const numFrames = Math.floor((channelData.length - frameSize) / hopSize) + 1;
    
    const mfcc: Float32Array[] = [];
    const energy = new Float32Array(numFrames);
    const spectralCentroid = new Float32Array(numFrames);
    const zeroCrossingRate = new Float32Array(numFrames);
    
    for (let frame = 0; frame < numFrames; frame++) {
      const start = frame * hopSize;
      const frameData = channelData.slice(start, start + frameSize);
      
      // Calculate MFCC (simplified version)
      const fft = this.computeFFT(frameData);
      const melFilters = this.applyMelFilters(fft, sampleRate);
      const mfccFrame = this.computeMFCC(melFilters);
      mfcc.push(mfccFrame);
      
      // Calculate energy
      energy[frame] = this.calculateEnergy(frameData);
      
      // Calculate spectral centroid
      spectralCentroid[frame] = this.calculateSpectralCentroid(fft, sampleRate);
      
      // Calculate zero crossing rate
      zeroCrossingRate[frame] = this.calculateZeroCrossingRate(frameData);
    }
    
    return {
      mfcc,
      energy,
      spectralCentroid,
      zeroCrossingRate,
      frameRate
    };
  }

  private extractTextFeatures(lyrics: LyricLine[], language: string): Float32Array[] {
    const phonemeMapping = this.phonemeMappings[language as keyof typeof this.phonemeMappings] || this.phonemeMappings.en;
    const textFeatures: Float32Array[] = [];
    
    for (const line of lyrics) {
      const words = line.text.toLowerCase().split(/\s+/);
      const lineFeatures = new Float32Array(40); // Feature vector size
      
      let featureIndex = 0;
      for (const word of words) {
        // Convert word to phonemes and extract features
        const phonemes = this.wordToPhonemes(word, phonemeMapping);
        const phonemeFeatures = this.phonemesToFeatures(phonemes);
        
        // Add features to line vector
        for (let i = 0; i < Math.min(phonemeFeatures.length, lineFeatures.length - featureIndex); i++) {
          lineFeatures[featureIndex + i] = phonemeFeatures[i];
        }
        
        featureIndex += phonemeFeatures.length;
        if (featureIndex >= lineFeatures.length) break;
      }
      
      textFeatures.push(lineFeatures);
    }
    
    return textFeatures;
  }

  private async performDTWAlignment(
    audioFeatures: AudioFeatures,
    textFeatures: Float32Array[],
    originalLyrics: LyricLine[],
    originalTimings?: number[]
  ): Promise<LyricLine[]> {
    const frameRate = audioFeatures.frameRate;
    const audioFrames = audioFeatures.mfcc.length;
    const textFrames = textFeatures.length;
    
    // Initialize DTW matrix
    const dtw = Array(audioFrames + 1).fill(null).map(() => 
      Array(textFrames + 1).fill(Infinity)
    );
    dtw[0][0] = 0;
    
    // Fill DTW matrix
    for (let i = 1; i <= audioFrames; i++) {
      for (let j = 1; j <= textFrames; j++) {
        const cost = this.calculateDistance(audioFeatures.mfcc[i - 1], textFeatures[j - 1]);
        dtw[i][j] = cost + Math.min(
          dtw[i - 1][j],     // insertion
          dtw[i][j - 1],     // deletion
          dtw[i - 1][j - 1]  // match
        );
      }
    }
    
    // Backtrack to find optimal path
    const path = this.backtrackDTW(dtw, audioFrames, textFrames);
    
    // First compute new start times for all lines
    const newTimes: number[] = [];
    for (let i = 0; i < originalLyrics.length; i++) {
      const pathPoint = path.find(p => p.textFrame === i);
      const newTime = pathPoint ? pathPoint.audioFrame / frameRate : originalLyrics[i].time;
      newTimes.push(newTime);
    }

    // Compute durations from successive start times; default last to 3s if missing
    const newDurations: number[] = [];
    for (let i = 0; i < originalLyrics.length; i++) {
      const start = newTimes[i];
      const nextStart = i < originalLyrics.length - 1 ? newTimes[i + 1] : undefined;
      const duration = nextStart !== undefined ? Math.max(0.1, nextStart - start) : (originalLyrics[i].duration ?? 3);
      newDurations.push(duration);
    }

    // Generate aligned lyrics with updated times and durations
    const alignedLyrics: LyricLine[] = [];
    for (let i = 0; i < originalLyrics.length; i++) {
      const startTime = newTimes[i];
      const duration = newDurations[i];

      let words: WordTiming[] | undefined;
      if (this.config.enableWordLevel && originalLyrics[i].words) {
        words = await this.alignWordsInLine(originalLyrics[i], startTime, duration, audioFeatures, frameRate);
      }

      alignedLyrics.push({
        ...originalLyrics[i],
        time: startTime,
        duration,
        words
      });
    }
    
    return alignedLyrics;
  }

  private async alignWordsInLine(
    line: LyricLine,
    lineStartTime: number,
    lineDuration: number,
    audioFeatures: AudioFeatures,
    frameRate: number
  ): Promise<WordTiming[]> {
    if (!line.words) return [];
    
    const words = line.text.split(/\s+/);
    const alignedWords: WordTiming[] = [];
    // Use provided duration (already computed)
    const duration = lineDuration > 0 ? lineDuration : (line.duration ?? 3);
    
    for (let i = 0; i < words.length; i++) {
      const word = words[i];
      const estimatedStart = lineStartTime + (i / words.length) * duration;
      const estimatedDuration = duration / words.length;
      
      // Fine-tune word timing using audio features
      const refinedTiming = this.refineWordTiming(
        word, 
        estimatedStart, 
        estimatedDuration, 
        audioFeatures, 
        frameRate
      );
      
      alignedWords.push({
        word,
        start: refinedTiming.start,
        end: refinedTiming.end
      });
    }
    
    // Normalize to exactly fit [lineStartTime, lineStartTime + duration]
    if (alignedWords.length > 0) {
      const total = alignedWords[alignedWords.length - 1].end - alignedWords[0].start;
      if (total > 0) {
        const scale = duration / total;
        const start0 = alignedWords[0].start;
        for (let i = 0; i < alignedWords.length; i++) {
          const relStart = alignedWords[i].start - start0;
          const relEnd = alignedWords[i].end - start0;
          alignedWords[i].start = lineStartTime + relStart * scale;
          alignedWords[i].end = lineStartTime + relEnd * scale;
        }
      }
      // Ensure monotonicity and clamp bounds
      for (let i = 0; i < alignedWords.length; i++) {
        const prevEnd = i > 0 ? alignedWords[i - 1].end : lineStartTime;
        alignedWords[i].start = Math.max(lineStartTime, Math.min(alignedWords[i].start, lineStartTime + duration));
        alignedWords[i].end = Math.max(alignedWords[i].start, Math.min(alignedWords[i].end, lineStartTime + duration));
        if (alignedWords[i].start < prevEnd) alignedWords[i].start = prevEnd;
        if (alignedWords[i].end < alignedWords[i].start) alignedWords[i].end = alignedWords[i].start;
      }
    }

    return alignedWords;
  }

  private refineWordTiming(
    word: string, 
    estimatedStart: number, 
    estimatedDuration: number, 
    audioFeatures: AudioFeatures, 
    frameRate: number
  ): { start: number; end: number } {
    const startFrame = Math.floor(estimatedStart * frameRate);
    const endFrame = Math.floor((estimatedStart + estimatedDuration) * frameRate);
    
    // Find energy peaks around estimated timing
    let bestStart = startFrame;
    let bestEnd = endFrame;
    let maxEnergy = 0;
    
    const searchRange = Math.floor(frameRate * 0.2); // 200ms search range
    
    for (let i = Math.max(0, startFrame - searchRange); 
         i <= Math.min(audioFeatures.energy.length - 1, startFrame + searchRange); i++) {
      if (audioFeatures.energy[i] > maxEnergy) {
        maxEnergy = audioFeatures.energy[i];
        bestStart = i;
      }
    }
    
    // Find corresponding end based on energy drop
    for (let i = bestStart + 1; i < Math.min(audioFeatures.energy.length, bestStart + searchRange * 2); i++) {
      if (audioFeatures.energy[i] < maxEnergy * 0.3) {
        bestEnd = i;
        break;
      }
    }
    
    return {
      start: bestStart / frameRate,
      end: bestEnd / frameRate
    };
  }

  // Audio processing utilities
  private computeFFT(frameData: Float32Array): Float32Array {
    // Simplified FFT implementation (in production, use a proper FFT library)
    const N = frameData.length;
    const result = new Float32Array(N / 2);
    
    for (let k = 0; k < N / 2; k++) {
      let real = 0, imag = 0;
      for (let n = 0; n < N; n++) {
        const angle = -2 * Math.PI * k * n / N;
        real += frameData[n] * Math.cos(angle);
        imag += frameData[n] * Math.sin(angle);
      }
      result[k] = Math.sqrt(real * real + imag * imag);
    }
    
    return result;
  }

  private applyMelFilters(fft: Float32Array, sampleRate: number): Float32Array {
    const numFilters = 26;
    const melFilters = new Float32Array(numFilters);
    
    // Simplified mel filter bank
    for (let i = 0; i < numFilters; i++) {
      const startFreq = (i * sampleRate) / (2 * numFilters);
      const endFreq = ((i + 2) * sampleRate) / (2 * numFilters);
      
      let sum = 0;
      let count = 0;
      
      for (let j = 0; j < fft.length; j++) {
        const freq = (j * sampleRate) / (2 * fft.length);
        if (freq >= startFreq && freq <= endFreq) {
          sum += fft[j];
          count++;
        }
      }
      
      melFilters[i] = count > 0 ? sum / count : 0;
    }
    
    return melFilters;
  }

  private computeMFCC(melFilters: Float32Array): Float32Array {
    const numCoeffs = 13;
    const mfcc = new Float32Array(numCoeffs);
    
    // Apply DCT to log mel filters
    for (let i = 0; i < numCoeffs; i++) {
      let sum = 0;
      for (let j = 0; j < melFilters.length; j++) {
        sum += Math.log(melFilters[j] + 1e-10) * Math.cos(Math.PI * i * (j + 0.5) / melFilters.length);
      }
      mfcc[i] = sum;
    }
    
    return mfcc;
  }

  private calculateEnergy(frameData: Float32Array): number {
    let energy = 0;
    for (let i = 0; i < frameData.length; i++) {
      energy += frameData[i] * frameData[i];
    }
    return energy / frameData.length;
  }

  private calculateSpectralCentroid(fft: Float32Array, sampleRate: number): number {
    let weightedSum = 0;
    let sum = 0;
    
    for (let i = 0; i < fft.length; i++) {
      const freq = (i * sampleRate) / (2 * fft.length);
      weightedSum += freq * fft[i];
      sum += fft[i];
    }
    
    return sum > 0 ? weightedSum / sum : 0;
  }

  private calculateZeroCrossingRate(frameData: Float32Array): number {
    let crossings = 0;
    for (let i = 1; i < frameData.length; i++) {
      if (frameData[i] * frameData[i - 1] < 0) {
        crossings++;
      }
    }
    return crossings / frameData.length;
  }

  private calculateDistance(audioFeature: Float32Array, textFeature: Float32Array): number {
    let distance = 0;
    const length = Math.min(audioFeature.length, textFeature.length);
    
    for (let i = 0; i < length; i++) {
      const diff = audioFeature[i] - textFeature[i];
      distance += diff * diff;
    }
    
    return Math.sqrt(distance / length);
  }

  private backtrackDTW(dtw: number[][], audioFrames: number, textFrames: number): Array<{audioFrame: number, textFrame: number}> {
    const path: Array<{audioFrame: number, textFrame: number}> = [];
    let i = audioFrames;
    let j = textFrames;
    
    while (i > 0 && j > 0) {
      path.unshift({audioFrame: i - 1, textFrame: j - 1});
      
      // Find the minimum of the three predecessors
      const options = [
        { cost: dtw[i - 1][j], move: [-1, 0] },
        { cost: dtw[i][j - 1], move: [0, -1] },
        { cost: dtw[i - 1][j - 1], move: [-1, -1] }
      ];
      
      const bestOption = options.reduce((min, curr) => curr.cost < min.cost ? curr : min);
      i += bestOption.move[0];
      j += bestOption.move[1];
    }
    
    return path;
  }

  private calculateAlignmentConfidence(alignedLyrics: LyricLine[], originalTimings?: number[]): number {
    if (!originalTimings || originalTimings.length !== alignedLyrics.length) {
      return 0.5; // Default confidence when no reference
    }
    
    let totalDeviation = 0;
    for (let i = 0; i < alignedLyrics.length; i++) {
      const deviation = Math.abs(alignedLyrics[i].time - originalTimings[i]);
      totalDeviation += deviation;
    }
    
    const averageDeviation = totalDeviation / alignedLyrics.length;
    const confidence = Math.max(0, 1 - (averageDeviation / 5000)); // 5 second max deviation
    
    return confidence;
  }

  // Phoneme mapping functions for different languages
  private wordToPhonemes(word: string, phonemeMapping: Map<string, string[]>): string[] {
    // Simplified phoneme conversion (in production, use a proper phoneme dictionary)
    const phonemes: string[] = [];
    for (const char of word.toLowerCase()) {
      const charPhonemes = phonemeMapping.get(char) || [char];
      phonemes.push(...charPhonemes);
    }
    return phonemes;
  }

  private phonemesToFeatures(phonemes: string[]): Float32Array {
    const features = new Float32Array(phonemes.length);
    for (let i = 0; i < phonemes.length; i++) {
      // Convert phoneme to numeric feature (simplified)
      features[i] = phonemes[i].charCodeAt(0) / 127; // Normalize to 0-1
    }
    return features;
  }

  // Language-specific phoneme mappings (simplified)
  private getEnglishPhonemes(): Map<string, string[]> {
    return new Map([
      ['a', ['æ', 'ə', 'ɑ']],
      ['e', ['ɛ', 'i', 'ə']],
      ['i', ['ɪ', 'aɪ']],
      ['o', ['ɔ', 'oʊ', 'ə']],
      ['u', ['ʊ', 'u', 'ʌ']],
      // Add more comprehensive mappings
    ]);
  }

  private getSpanishPhonemes(): Map<string, string[]> {
    return new Map([
      ['a', ['a']],
      ['e', ['e']],
      ['i', ['i']],
      ['o', ['o']],
      ['u', ['u']],
      ['r', ['r', 'rr']],
      ['ñ', ['ɲ']],
    ]);
  }

  private getFrenchPhonemes(): Map<string, string[]> {
    return new Map([
      ['a', ['a', 'ɑ']],
      ['e', ['e', 'ɛ', 'ə']],
      ['i', ['i']],
      ['o', ['o', 'ɔ']],
      ['u', ['u']],
      ['é', ['e']],
      ['è', ['ɛ']],
    ]);
  }

  private getGermanPhonemes(): Map<string, string[]> {
    return new Map([
      ['a', ['a', 'ɑ']],
      ['e', ['e', 'ɛ', 'ə']],
      ['i', ['i', 'ɪ']],
      ['o', ['o', 'ɔ']],
      ['u', ['u', 'ʊ']],
      ['ü', ['y', 'ʏ']],
      ['ö', ['ø', 'œ']],
      ['ä', ['ɛ']],
    ]);
  }

  private getJapanesePhonemes(): Map<string, string[]> {
    return new Map([
      ['あ', ['a']],
      ['い', ['i']],
      ['う', ['u']],
      ['え', ['e']],
      ['お', ['o']],
      ['か', ['ka']],
      ['が', ['ga']],
      // Add hiragana/katakana mappings
    ]);
  }

  private getKoreanPhonemes(): Map<string, string[]> {
    return new Map([
      ['ㅏ', ['a']],
      ['ㅓ', ['ʌ']],
      ['ㅗ', ['o']],
      ['ㅜ', ['u']],
      ['ㅡ', ['ɯ']],
      ['ㅣ', ['i']],
      // Add more Korean phonemes
    ]);
  }

  private getChinesePhonemes(): Map<string, string[]> {
    return new Map([
      ['a', ['a']],
      ['e', ['ə']],
      ['i', ['i']],
      ['o', ['o']],
      ['u', ['u']],
      ['ü', ['y']],
      // Add pinyin mappings
    ]);
  }

  private getRussianPhonemes(): Map<string, string[]> {
    return new Map([
      ['а', ['a']],
      ['е', ['je', 'e']],
      ['и', ['i']],
      ['о', ['o']],
      ['у', ['u']],
      ['ы', ['ɨ']],
      ['э', ['e']],
      ['ю', ['ju']],
      ['я', ['ja']],
    ]);
  }

  private getArabicPhonemes(): Map<string, string[]> {
    return new Map([
      ['ا', ['a', 'ɑ']],
      ['ي', ['i', 'j']],
      ['و', ['u', 'w']],
      ['ة', ['a', 'at']],
      // Add more Arabic phonemes
    ]);
  }

  private getHindiPhonemes(): Map<string, string[]> {
    return new Map([
      ['अ', ['ə']],
      ['आ', ['a']],
      ['इ', ['ɪ']],
      ['ई', ['i']],
      ['उ', ['ʊ']],
      ['ऊ', ['u']],
      // Add more Devanagari phonemes
    ]);
  }

  // Public utility methods
  updateConfig(newConfig: Partial<AlignmentConfig>): void {
    this.config = { ...this.config, ...newConfig };
  }

  getSupportedLanguages(): string[] {
    return Object.keys(this.phonemeMappings);
  }

  async destroy(): Promise<void> {
    if (this.audioContext && this.audioContext.state !== 'closed') {
      await this.audioContext.close();
    }
  }
}
