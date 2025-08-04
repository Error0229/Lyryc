import { LyricLine } from '../stores/lyricsStore';

export interface CachedLyrics {
  id: string;
  trackName: string;
  artistName: string;
  lyrics: LyricLine[];
  metadata: {
    source: string;
    confidence: number;
    method: string;
    language: string;
    hasWordTimings: boolean;
    processingTime: number;
  };
  timestamp: number;
  expiresAt: number;
}

export interface CacheConfig {
  maxSize: number; // Maximum number of entries
  maxSizeBytes: number; // Maximum size in bytes
  defaultTTL: number; // Time to live in milliseconds
  cleanupInterval: number; // Cleanup interval in milliseconds
}

export class CacheManager {
  private dbName = 'LyrycCache';
  private version = 1;
  private storeName = 'lyrics';
  private db: IDBDatabase | null = null;
  private config: CacheConfig;
  private cleanupTimer: number | null = null;

  constructor(config: CacheConfig = {
    maxSize: 1000,
    maxSizeBytes: 50 * 1024 * 1024, // 50MB
    defaultTTL: 7 * 24 * 60 * 60 * 1000, // 7 days
    cleanupInterval: 60 * 60 * 1000 // 1 hour
  }) {
    this.config = config;
    this.initialize();
    this.startCleanupTimer();
  }

  private async initialize(): Promise<void> {
    try {
      this.db = await this.openDatabase();
      console.log('Cache database initialized');
    } catch (error) {
      console.error('Failed to initialize cache database:', error);
    }
  }

  private openDatabase(): Promise<IDBDatabase> {
    return new Promise((resolve, reject) => {
      const request = indexedDB.open(this.dbName, this.version);

      request.onerror = () => {
        reject(new Error('Failed to open IndexedDB'));
      };

      request.onsuccess = () => {
        resolve(request.result);
      };

      request.onupgradeneeded = (event) => {
        const db = (event.target as IDBOpenDBRequest).result;
        
        if (!db.objectStoreNames.contains(this.storeName)) {
          const store = db.createObjectStore(this.storeName, { keyPath: 'id' });
          store.createIndex('trackArtist', ['trackName', 'artistName'], { unique: false });
          store.createIndex('timestamp', 'timestamp', { unique: false });
          store.createIndex('expiresAt', 'expiresAt', { unique: false });
        }
      };
    });
  }

  async set(
    trackName: string,
    artistName: string,
    lyrics: LyricLine[],
    metadata: CachedLyrics['metadata'],
    ttl?: number
  ): Promise<void> {
    if (!this.db) {
      console.warn('Cache database not initialized');
      return;
    }

    const id = this.generateCacheKey(trackName, artistName);
    const now = Date.now();
    const expiresAt = now + (ttl || this.config.defaultTTL);

    const cachedData: CachedLyrics = {
      id,
      trackName: trackName.toLowerCase().trim(),
      artistName: artistName.toLowerCase().trim(),
      lyrics,
      metadata,
      timestamp: now,
      expiresAt
    };

    try {
      // Check cache size before adding
      await this.enforceCacheLimits();

      const transaction = this.db.transaction([this.storeName], 'readwrite');
      const store = transaction.objectStore(this.storeName);
      
      await new Promise<void>((resolve, reject) => {
        const request = store.put(cachedData);
        request.onsuccess = () => resolve();
        request.onerror = () => reject(request.error);
      });

      console.log(`Cached lyrics for: ${trackName} - ${artistName}`);
    } catch (error) {
      console.error('Failed to cache lyrics:', error);
    }
  }

  async get(trackName: string, artistName: string): Promise<CachedLyrics | null> {
    if (!this.db) {
      console.warn('Cache database not initialized');
      return null;
    }

    const id = this.generateCacheKey(trackName, artistName);

    try {
      const transaction = this.db.transaction([this.storeName], 'readonly');
      const store = transaction.objectStore(this.storeName);

      const cachedData = await new Promise<CachedLyrics | null>((resolve, reject) => {
        const request = store.get(id);
        request.onsuccess = () => resolve(request.result || null);
        request.onerror = () => reject(request.error);
      });

      if (!cachedData) {
        return null;
      }

      // Check if expired
      if (Date.now() > cachedData.expiresAt) {
        await this.delete(trackName, artistName);
        return null;
      }

      console.log(`Cache hit for: ${trackName} - ${artistName}`);
      return cachedData;
    } catch (error) {
      console.error('Failed to get cached lyrics:', error);
      return null;
    }
  }

  async delete(trackName: string, artistName: string): Promise<void> {
    if (!this.db) return;

    const id = this.generateCacheKey(trackName, artistName);

    try {
      const transaction = this.db.transaction([this.storeName], 'readwrite');
      const store = transaction.objectStore(this.storeName);

      await new Promise<void>((resolve, reject) => {
        const request = store.delete(id);
        request.onsuccess = () => resolve();
        request.onerror = () => reject(request.error);
      });
    } catch (error) {
      console.error('Failed to delete cached lyrics:', error);
    }
  }

  async clear(): Promise<void> {
    if (!this.db) return;

    try {
      const transaction = this.db.transaction([this.storeName], 'readwrite');
      const store = transaction.objectStore(this.storeName);

      await new Promise<void>((resolve, reject) => {
        const request = store.clear();
        request.onsuccess = () => resolve();
        request.onerror = () => reject(request.error);
      });

      console.log('Cache cleared');
    } catch (error) {
      console.error('Failed to clear cache:', error);
    }
  }

  async getCacheStats(): Promise<{
    totalEntries: number;
    totalSizeBytes: number;
    oldestEntry: number | null;
    newestEntry: number | null;
  }> {
    if (!this.db) {
      return { totalEntries: 0, totalSizeBytes: 0, oldestEntry: null, newestEntry: null };
    }

    try {
      const transaction = this.db.transaction([this.storeName], 'readonly');
      const store = transaction.objectStore(this.storeName);

      const allEntries = await new Promise<CachedLyrics[]>((resolve, reject) => {
        const request = store.getAll();
        request.onsuccess = () => resolve(request.result);
        request.onerror = () => reject(request.error);
      });

      let totalSizeBytes = 0;
      let oldestEntry: number | null = null;
      let newestEntry: number | null = null;

      for (const entry of allEntries) {
        const entrySize = this.calculateEntrySize(entry);
        totalSizeBytes += entrySize;

        if (oldestEntry === null || entry.timestamp < oldestEntry) {
          oldestEntry = entry.timestamp;
        }
        if (newestEntry === null || entry.timestamp > newestEntry) {
          newestEntry = entry.timestamp;
        }
      }

      return {
        totalEntries: allEntries.length,
        totalSizeBytes,
        oldestEntry,
        newestEntry
      };
    } catch (error) {
      console.error('Failed to get cache stats:', error);
      return { totalEntries: 0, totalSizeBytes: 0, oldestEntry: null, newestEntry: null };
    }
  }

  private async enforceCacheLimits(): Promise<void> {
    const stats = await this.getCacheStats();

    // Check if we need to cleanup
    if (stats.totalEntries >= this.config.maxSize || stats.totalSizeBytes >= this.config.maxSizeBytes) {
      await this.performCleanup();
    }
  }

  private async performCleanup(): Promise<void> {
    if (!this.db) return;

    try {
      const transaction = this.db.transaction([this.storeName], 'readwrite');
      const store = transaction.objectStore(this.storeName);
      const index = store.index('timestamp');

      // Get all entries sorted by timestamp (oldest first)
      const allEntries = await new Promise<CachedLyrics[]>((resolve, reject) => {
        const request = index.getAll();
        request.onsuccess = () => resolve(request.result);
        request.onerror = () => reject(request.error);
      });

      const now = Date.now();
      let deletedCount = 0;
      let currentSize = 0;

      // Calculate current total size
      for (const entry of allEntries) {
        currentSize += this.calculateEntrySize(entry);
      }

      // Delete expired entries first
      for (const entry of allEntries) {
        if (entry.expiresAt < now) {
          await this.deleteEntry(entry.id);
          deletedCount++;
          currentSize -= this.calculateEntrySize(entry);
        }
      }

      // If still over limits, delete oldest entries
      if (allEntries.length - deletedCount >= this.config.maxSize || 
          currentSize >= this.config.maxSizeBytes) {
        
        const remainingEntries = allEntries.filter(entry => entry.expiresAt >= now);
        remainingEntries.sort((a, b) => a.timestamp - b.timestamp);

        for (const entry of remainingEntries) {
          if (remainingEntries.length - deletedCount < this.config.maxSize * 0.8 && 
              currentSize < this.config.maxSizeBytes * 0.8) {
            break;
          }

          await this.deleteEntry(entry.id);
          deletedCount++;
          currentSize -= this.calculateEntrySize(entry);
        }
      }

      if (deletedCount > 0) {
        console.log(`Cache cleanup: removed ${deletedCount} entries`);
      }
    } catch (error) {
      console.error('Failed to perform cache cleanup:', error);
    }
  }

  private async deleteEntry(id: string): Promise<void> {
    if (!this.db) return;

    const transaction = this.db.transaction([this.storeName], 'readwrite');
    const store = transaction.objectStore(this.storeName);

    return new Promise<void>((resolve, reject) => {
      const request = store.delete(id);
      request.onsuccess = () => resolve();
      request.onerror = () => reject(request.error);
    });
  }

  private calculateEntrySize(entry: CachedLyrics): number {
    // Rough calculation of entry size in bytes
    const jsonString = JSON.stringify(entry);
    return new Blob([jsonString]).size;
  }

  private generateCacheKey(trackName: string, artistName: string): string {
    const normalized = `${trackName.toLowerCase().trim()}-${artistName.toLowerCase().trim()}`;
    return btoa(normalized).replace(/[+/=]/g, ''); // Base64 encode and remove special chars
  }

  private startCleanupTimer(): void {
    this.cleanupTimer = window.setInterval(() => {
      this.performCleanup();
    }, this.config.cleanupInterval);
  }

  async destroy(): Promise<void> {
    if (this.cleanupTimer) {
      clearInterval(this.cleanupTimer);
      this.cleanupTimer = null;
    }

    if (this.db) {
      this.db.close();
      this.db = null;
    }
  }

  // Search functionality
  async search(query: string, limit: number = 10): Promise<CachedLyrics[]> {
    if (!this.db) return [];

    try {
      const transaction = this.db.transaction([this.storeName], 'readonly');
      const store = transaction.objectStore(this.storeName);

      const allEntries = await new Promise<CachedLyrics[]>((resolve, reject) => {
        const request = store.getAll();
        request.onsuccess = () => resolve(request.result);
        request.onerror = () => reject(request.error);
      });

      const queryLower = query.toLowerCase();
      const results = allEntries
        .filter(entry => {
          return entry.trackName.includes(queryLower) || 
                 entry.artistName.includes(queryLower);
        })
        .filter(entry => Date.now() <= entry.expiresAt) // Filter out expired
        .sort((a, b) => b.timestamp - a.timestamp) // Sort by newest first
        .slice(0, limit);

      return results;
    } catch (error) {
      console.error('Failed to search cache:', error);
      return [];
    }
  }
}