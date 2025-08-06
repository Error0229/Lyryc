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
  track_name?: string;
  artist_name?: string;
  album_name?: string;
  duration?: number;
  q?: string; // Wildcard search parameter (exclusive with track_name/artist_name)
}

const LRCLIB_BASE_URL = 'https://lrclib.net/api';

export class LRCLibService {
  private static cacheManager = new CacheManager();
  private static readonly MAX_RETRIES = 3;
  private static readonly RETRY_DELAY = 1000; // 1 second

  static async searchLyrics(params: LRCLibSearchParams, abortSignal?: AbortSignal): Promise<LyricLine[]> {
    console.log(`[LRCLib] Searching lyrics for: "${params.track_name}" by "${params.artist_name}"`);
    
    // Check if cancelled before starting
    if (abortSignal?.aborted) {
      throw new Error('Request was cancelled');
    }
    
    // Check IndexedDB cache first (only for non-wildcard searches)
    if (params.track_name && params.artist_name) {
      const cachedData = await this.cacheManager.get(params.track_name, params.artist_name);
      if (cachedData) {
        console.log('Cache hit for:', `${params.track_name} - ${params.artist_name}`);
        return cachedData.lyrics;
      }
    }
    
    // Check if cancelled after cache lookup
    if (abortSignal?.aborted) {
      throw new Error('Request was cancelled');
    }

    // Try multiple search strategies
    const trackName = params.track_name || '';
    const artistName = params.artist_name || '';
    
    const cleanedTrackName = this.cleanTrackName(trackName);
    const trackWithoutArtist = this.removeArtistFromTrack(trackName, artistName);
    const cleanedTrackWithoutArtist = this.cleanTrackName(trackWithoutArtist);

    const searchStrategies = [
      // Wildcard searches (often most effective for complex track names)
      { q: `${trackName} ${artistName}` },
      { q: `${cleanedTrackName} ${artistName}` },
      { q: `${trackWithoutArtist} ${artistName}` },
      { q: `${cleanedTrackWithoutArtist} ${artistName}` },
      { q: trackName },
      { q: cleanedTrackName },
      { q: trackWithoutArtist },
      { q: cleanedTrackWithoutArtist },
      
      // Exact match strategies
      { track_name: trackName, artist_name: artistName, album_name: params.album_name, duration: params.duration },
      // Without album
      { track_name: trackName, artist_name: artistName, duration: params.duration },
      // Without duration
      { track_name: trackName, artist_name: artistName, album_name: params.album_name },
      // Minimal search
      { track_name: trackName, artist_name: artistName },

      // Clean track name strategies
      { track_name: cleanedTrackName, artist_name: artistName, duration: params.duration },
      { track_name: cleanedTrackName, artist_name: artistName },

      // Track name with artist removed
      { track_name: trackWithoutArtist, artist_name: artistName },
      { track_name: cleanedTrackWithoutArtist, artist_name: artistName },

      // Fallback searches without artist name
      { track_name: trackName, artist_name: '', duration: params.duration },
      { track_name: trackName, artist_name: '' },
      { track_name: cleanedTrackName, artist_name: '', duration: params.duration },
      { track_name: cleanedTrackName, artist_name: '' },
      { track_name: trackWithoutArtist, artist_name: '' },
      { track_name: cleanedTrackWithoutArtist, artist_name: '' },

      // Swap artist and track (sometimes they get mixed up)
      { track_name: artistName, artist_name: trackName }
    ].filter(strategy => {
      // Filter out strategies with empty search terms
      if (strategy.q) {
        return strategy.q.trim().length > 0;
      }
      return strategy.track_name && strategy.track_name.trim().length > 0;
    });

    for (const strategy of searchStrategies) {
      // Check if cancelled before each strategy
      if (abortSignal?.aborted) {
        throw new Error('Request was cancelled');
      }
      
      const strategyDesc = strategy.q 
        ? `wildcard: "${strategy.q}"` 
        : `exact: "${strategy.track_name}" by "${strategy.artist_name}"`;
      console.log(`[LRCLib] Trying strategy: ${strategyDesc}`);
      const result = await this.trySearchWithStrategy(strategy, abortSignal);
      if (result.length > 0) {
        console.log(`[LRCLib] Success with strategy: ${strategyDesc} - found ${result.length} lines`);
        // Cache successful result in IndexedDB (only for non-wildcard searches)
        if (trackName && artistName) {
          await this.cacheManager.set(
            trackName,
            artistName,
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
        }
        return result;
      }
      
      // Check if cancelled before delay
      if (abortSignal?.aborted) {
        throw new Error('Request was cancelled');
      }

      // Small delay between attempts
      await this.delay(200);
    }

    console.warn('No lyrics found after all strategies for:', params);
    return [];
  }

  private static async trySearchWithStrategy(params: LRCLibSearchParams, abortSignal?: AbortSignal): Promise<LyricLine[]> {
    for (let attempt = 0; attempt < this.MAX_RETRIES; attempt++) {
      // Check if cancelled before each attempt
      if (abortSignal?.aborted) {
        throw new Error('Request was cancelled');
      }
      
      try {
        const result = await this.performSearch(params, abortSignal);
        if (result.length > 0) {
          return result;
        } else {
          const strategyDesc = params.q 
            ? `wildcard: "${params.q}"` 
            : `exact: "${params.track_name}" by "${params.artist_name}"`;
          console.log(`[LRCLib] No results for strategy: ${strategyDesc}`);
        }
      } catch (error) {
        const strategyDesc = params.q 
          ? `wildcard: "${params.q}"` 
          : `exact: "${params.track_name}" by "${params.artist_name}"`;
        console.warn(`[LRCLib] Search attempt ${attempt + 1} failed for ${strategyDesc}:`, error);
        if (attempt < this.MAX_RETRIES - 1) {
          await this.delay(this.RETRY_DELAY * (attempt + 1));
        }
      }
    }
    return [];
  }

  private static async performSearch(params: LRCLibSearchParams, abortSignal?: AbortSignal): Promise<LyricLine[]> {
    const searchParams = new URLSearchParams();

    // Use wildcard search OR specific parameters (mutually exclusive)
    if (params.q && params.q.trim()) {
      // Wildcard search using ?q= parameter
      searchParams.append('q', params.q.trim());
    } else {
      // Specific search using track_name and artist_name
      if (params.track_name) {
        searchParams.append('track_name', params.track_name);
      }

      // Only add artist name if it's not empty
      if (params.artist_name && params.artist_name.trim()) {
        searchParams.append('artist_name', params.artist_name);
      }

      if (params.album_name) {
        searchParams.append('album_name', params.album_name);
      }

      if (params.duration) {
        searchParams.append('duration', params.duration.toString());
      }
    }

    // Create combined abort controller for timeout and external cancellation
    const timeoutController = new AbortController();
    const timeoutId = setTimeout(() => timeoutController.abort(), 10000); // 10 second timeout
    
    // Handle external abort signal
    if (abortSignal?.aborted) {
      clearTimeout(timeoutId);
      throw new Error('Request was cancelled');
    }
    
    const abortHandler = () => timeoutController.abort();
    abortSignal?.addEventListener('abort', abortHandler);

    try {
      const response = await fetch(`${LRCLIB_BASE_URL}/search?${searchParams}`, {
        signal: timeoutController.signal,
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
      abortSignal?.removeEventListener('abort', abortHandler);
    }
  }

  private static cleanTrackName(trackName: string): string {
    return trackName
      // Remove common YouTube Music additions
      .replace(/\s*\(.*?MV.*?\)/gi, '') // Remove (MV)
      .replace(/\s*\[.*?MV.*?\]/gi, '') // Remove [MV]
      .replace(/\s*\(.*?Official.*?Video.*?\)/gi, '') // Remove (Official Video)
      .replace(/\s*\[.*?Official.*?Video.*?\]/gi, '') // Remove [Official Video]
      .replace(/\s*\(.*?Official.*?Music.*?Video.*?\)/gi, '') // Remove (Official Music Video)
      .replace(/\s*\[.*?Official.*?Music.*?Video.*?\]/gi, '') // Remove [Official Music Video]
      .replace(/\s*\(.*?Audio.*?\)/gi, '') // Remove (Audio)
      .replace(/\s*\[.*?Audio.*?\]/gi, '') // Remove [Audio]
      .replace(/\s*\(.*?Lyric.*?Video.*?\)/gi, '') // Remove (Lyric Video)
      .replace(/\s*\[.*?Lyric.*?Video.*?\]/gi, '') // Remove [Lyric Video]

      // Remove translations and language indicators
      .replace(/\s*\(.*?中文.*?\)/gi, '') // Remove Chinese translations
      .replace(/\s*\[.*?中文.*?\]/gi, '') // Remove Chinese translations
      .replace(/\s*\(.*?日本語.*?\)/gi, '') // Remove Japanese translations
      .replace(/\s*\[.*?日本語.*?\]/gi, '') // Remove Japanese translations
      .replace(/\s*\(.*?한국어.*?\)/gi, '') // Remove Korean translations
      .replace(/\s*\[.*?한국어.*?\]/gi, '') // Remove Korean translations

      // Remove featuring and remix info
      .replace(/\s*\(.*?feat\..*?\)/gi, '') // Remove (feat. ...)
      .replace(/\s*\[.*?feat\..*?\]/gi, '') // Remove [feat. ...]
      .replace(/\s*\(.*?ft\..*?\)/gi, '') // Remove (ft. ...)
      .replace(/\s*\[.*?ft\..*?\]/gi, '') // Remove [ft. ...]
      .replace(/\s*\(.*?featuring.*?\)/gi, '') // Remove (featuring ...)
      .replace(/\s*\[.*?featuring.*?\]/gi, '') // Remove [featuring ...]
      .replace(/\s*\(.*?remix.*?\)/gi, '') // Remove (remix)
      .replace(/\s*\[.*?remix.*?\]/gi, '') // Remove [remix]
      .replace(/\s*\(.*?version.*?\)/gi, '') // Remove (version)
      .replace(/\s*\[.*?version.*?\]/gi, '') // Remove [version]
      .replace(/\s*-\s*remaster.*$/gi, '') // Remove - remastered

      // Remove live performance indicators
      .replace(/\s*\(.*?Live.*?\)/gi, '') // Remove (Live ...)
      .replace(/\s*\[.*?Live.*?\]/gi, '') // Remove [Live ...)

      // Clean up multiple spaces and trim
      .replace(/\s+/g, ' ')
      .trim();
  }

  private static removeArtistFromTrack(trackName: string, artistName: string): string {
    if (!artistName) return trackName;

    // Create pattern to match artist name at the beginning with common separators
    const artistPattern = new RegExp(`^${this.escapeRegExp(artistName)}\\s*[-–—]\\s*`, 'gi');
    const result = trackName.replace(artistPattern, '').trim();

    // Also try removing from end with separators
    const artistEndPattern = new RegExp(`\\s*[-–—]\\s*${this.escapeRegExp(artistName)}$`, 'gi');
    const endResult = result.replace(artistEndPattern, '').trim();

    // Return the result that changed the most (likely removed the artist)
    if (endResult.length < result.length) {
      return endResult;
    }
    return result.length < trackName.length ? result : trackName;
  }

  private static escapeRegExp(string: string): string {
    return string.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
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
