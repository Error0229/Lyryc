import { LyricLine } from '../stores/lyricsStore';
import { CacheManager } from './cacheManager';

interface LRCLibResponse {
  id: number;
  trackName: string;
  artistName: string;
  albumName: string;
  duration: number;
  plainLyrics: string;
  syncedLyrics: string;
}

interface LRCLibSearchParams {
  track_name: string;
  artist_name: string;
  album_name?: string;
  duration?: number;
}

const LRCLIB_BASE_URL = 'https://lrclib.net/api';
const FALLBACK_APIS = [
  'https://lrclib.net/api',
  // Add more fallback APIs if available
];

export class LRCLibService {
  private static cacheManager = new CacheManager();
  private static readonly MAX_RETRIES = 3;
  private static readonly RETRY_DELAY = 1000; // 1 second

  static async searchLyrics(params: LRCLibSearchParams): Promise<LyricLine[]> {
    // Check IndexedDB cache first
    const cachedData = await this.cacheManager.get(params.track_name, params.artist_name);
    if (cachedData) {
      console.log('Cache hit for:', `${params.track_name} - ${params.artist_name}`);
      return cachedData.lyrics;
    }

    // Try multiple search strategies
    const searchStrategies = [
      // Exact match
      { track_name: params.track_name, artist_name: params.artist_name, album_name: params.album_name, duration: params.duration },
      // Without album
      { track_name: params.track_name, artist_name: params.artist_name, duration: params.duration },
      // Without duration
      { track_name: params.track_name, artist_name: params.artist_name, album_name: params.album_name },
      // Minimal search
      { track_name: params.track_name, artist_name: params.artist_name },
      // Clean track name (remove feat., remix, etc.)
      { track_name: this.cleanTrackName(params.track_name), artist_name: params.artist_name },
      // Swap artist and track (sometimes they get mixed up)
      { track_name: params.artist_name, artist_name: params.track_name }
    ];

    for (const strategy of searchStrategies) {
      const result = await this.trySearchWithStrategy(strategy);
      if (result.length > 0) {
        // Cache successful result in IndexedDB
        await this.cacheManager.set(
          params.track_name,
          params.artist_name,
          result,
          {
            source: 'lrclib',
            confidence: 0.9,
            method: 'api',
            language: 'auto',
            hasWordTimings: result.some(line => line.words && line.words.length > 0),
            processingTime: 0
          }
        );
        return result;
      }
      
      // Small delay between attempts
      await this.delay(200);
    }

    console.warn('No lyrics found after all strategies for:', params);
    return [];
  }

  private static async trySearchWithStrategy(params: LRCLibSearchParams): Promise<LyricLine[]> {
    for (let attempt = 0; attempt < this.MAX_RETRIES; attempt++) {
      try {
        const result = await this.performSearch(params);
        if (result.length > 0) {
          return result;
        }
      } catch (error) {
        console.warn(`Search attempt ${attempt + 1} failed:`, error);
        if (attempt < this.MAX_RETRIES - 1) {
          await this.delay(this.RETRY_DELAY * (attempt + 1));
        }
      }
    }
    return [];
  }

  private static async performSearch(params: LRCLibSearchParams): Promise<LyricLine[]> {
    const searchParams = new URLSearchParams({
      track_name: params.track_name,
      artist_name: params.artist_name,
    });

    if (params.album_name) {
      searchParams.append('album_name', params.album_name);
    }

    if (params.duration) {
      searchParams.append('duration', params.duration.toString());
    }

    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), 10000); // 10 second timeout

    try {
      const response = await fetch(`${LRCLIB_BASE_URL}/search?${searchParams}`, {
        signal: controller.signal,
        headers: {
          'User-Agent': 'Lyryc/1.0 (https://github.com/your-repo/lyryc)',
          'Accept': 'application/json',
        }
      });
      
      clearTimeout(timeoutId);
      
      if (!response.ok) {
        throw new Error(`HTTP error! status: ${response.status}`);
      }

      const results: LRCLibResponse[] = await response.json();
      
      if (results.length === 0) {
        return [];
      }

      // Prioritize results with synced lyrics
      const syncedResults = results.filter(r => r.syncedLyrics);
      const bestResult = syncedResults.length > 0 ? syncedResults[0] : results[0];
      
      if (!bestResult.syncedLyrics) {
        console.warn('No synced lyrics found for track');
        return [];
      }

      return this.parseLRCFormat(bestResult.syncedLyrics);
    } finally {
      clearTimeout(timeoutId);
    }
  }

  private static cleanTrackName(trackName: string): string {
    return trackName
      .replace(/\s*\(.*?feat\..*?\)/gi, '') // Remove (feat. ...)
      .replace(/\s*\[.*?feat\..*?\]/gi, '') // Remove [feat. ...]
      .replace(/\s*\(.*?remix.*?\)/gi, '') // Remove (remix)
      .replace(/\s*\[.*?remix.*?\]/gi, '') // Remove [remix]
      .replace(/\s*\(.*?version.*?\)/gi, '') // Remove (version)
      .replace(/\s*-\s*remaster.*$/gi, '') // Remove - remastered
      .trim();
  }

  private static delay(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }

  static parseLRCFormat(lrcContent: string): LyricLine[] {
    const lines: LyricLine[] = [];
    const lrcLines = lrcContent.split('\n');

    for (const line of lrcLines) {
      // Enhanced LRC parser supporting multiple timestamp formats
      // Standard: [mm:ss.xx] or [mm:ss]
      // Enhanced: [mm:ss.xxx] (milliseconds)
      // Word-level: [mm:ss.xx]<mm:ss.xx>word<mm:ss.xx>word
      
      const timestampMatch = line.match(/^\[(\d{1,2}):(\d{2})(?:\.(\d{2,3}))?\](.*)/);
      
      if (timestampMatch) {
        const minutes = parseInt(timestampMatch[1], 10);
        const seconds = parseInt(timestampMatch[2], 10);
        const subseconds = timestampMatch[3] || '0';
        // Handle both centiseconds (2 digits) and milliseconds (3 digits)
        const millisecondsFromSub = subseconds.length === 2 ? parseInt(subseconds, 10) * 10 : parseInt(subseconds, 10);
        const textPart = timestampMatch[4].trim();

        if (textPart) {
          const timeInMs = (minutes * 60 + seconds) * 1000 + millisecondsFromSub;
          
          // Check for word-level timing in Enhanced LRC format
          const wordTimings = this.parseWordLevelTimings(textPart);
          
          lines.push({
            time: timeInMs,
            text: this.cleanTextFromWordTimings(textPart),
            words: wordTimings.length > 0 ? wordTimings : undefined,
          });
        }
      }
    }

    // Sort by time and calculate durations
    lines.sort((a, b) => a.time - b.time);
    
    for (let i = 0; i < lines.length - 1; i++) {
      lines[i].duration = lines[i + 1].time - lines[i].time;
    }

    // Set duration for last line (default 3 seconds)
    if (lines.length > 0) {
      lines[lines.length - 1].duration = 3000;
    }

    return lines;
  }

  private static parseWordLevelTimings(text: string): Array<{ start: number; end: number; word: string }> {
    const wordTimings: Array<{ start: number; end: number; word: string }> = [];
    
    // Enhanced LRC format: <mm:ss.xx>word<mm:ss.xx>word
    const wordPattern = /<(\d{1,2}):(\d{2})(?:\.(\d{2,3}))?>(.*?)(?=<\d|$)/g;
    let match;
    
    while ((match = wordPattern.exec(text)) !== null) {
      const minutes = parseInt(match[1], 10);
      const seconds = parseInt(match[2], 10);
      const subseconds = match[3] || '0';
      const millisecondsFromSub = subseconds.length === 2 ? parseInt(subseconds, 10) * 10 : parseInt(subseconds, 10);
      const word = match[4].trim();
      
      if (word) {
        const startTime = (minutes * 60 + seconds) * 1000 + millisecondsFromSub;
        wordTimings.push({
          start: startTime,
          end: startTime + 500, // Default 500ms per word, will be adjusted
          word: word
        });
      }
    }
    
    // Adjust end times based on next word's start time
    for (let i = 0; i < wordTimings.length - 1; i++) {
      wordTimings[i].end = wordTimings[i + 1].start;
    }
    
    return wordTimings;
  }

  private static cleanTextFromWordTimings(text: string): string {
    // Remove word-level timing markup to get clean text
    return text
      .replace(/<\d{1,2}:\d{2}(?:\.\d{2,3})?>/g, '') // Remove timestamps
      .replace(/\s+/g, ' ') // Normalize whitespace
      .trim();
  }

  static async clearCache(): Promise<void> {
    await this.cacheManager.clear();
  }

  static async getCacheStats() {
    return await this.cacheManager.getCacheStats();
  }

  static async searchCache(query: string, limit?: number) {
    return await this.cacheManager.search(query, limit);
  }

  static async destroyCache(): Promise<void> {
    await this.cacheManager.destroy();
  }
}