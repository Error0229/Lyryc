//! Persistent lyrics caching module using SQLite
//!
//! Stores fetched lyrics in a SQLite database to avoid redundant API calls
//! across app restarts. Gracefully degrades to no-cache if DB fails.

use crate::types::LyricLine;
use log::{debug, error, info, warn};
use rusqlite::{params, Connection};
use std::path::PathBuf;
use std::sync::Mutex;

/// Cache TTL: 30 days in seconds
const CACHE_TTL_SECONDS: i64 = 30 * 24 * 60 * 60;

lazy_static::lazy_static! {
    /// Single database connection protected by mutex.
    /// SQLite with WAL mode handles concurrent reads well, mutex serializes writes.
    static ref DB_CONNECTION: Mutex<Option<Connection>> = Mutex::new(init_db());
}

/// Get the database path in user's app data directory
fn get_db_path() -> Option<PathBuf> {
    let base_dir = std::env::var("LOCALAPPDATA")
        .or_else(|_| std::env::var("XDG_DATA_HOME"))
        .or_else(|_| std::env::var("HOME").map(|h| format!("{}/.local/share", h)))
        .ok()?;

    let app_dir = PathBuf::from(base_dir).join("Lyryc");

    if let Err(e) = std::fs::create_dir_all(&app_dir) {
        error!("Failed to create app data directory: {}. Cache disabled.", e);
        return None;
    }

    Some(app_dir.join("lyrics_cache.db"))
}

/// Initialize database connection and schema. Called once at startup.
fn init_db() -> Option<Connection> {
    let db_path = get_db_path()?;
    info!("Initializing lyrics cache database at: {:?}", db_path);

    let conn = match Connection::open(&db_path) {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to open lyrics cache database: {}. Cache disabled.", e);
            return None;
        }
    };

    // Enable WAL mode once at startup for better concurrent access
    if let Err(e) = conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;") {
        warn!("Failed to set WAL mode: {}", e);
    }

    // Create table
    if let Err(e) = conn.execute(
        "CREATE TABLE IF NOT EXISTS lyrics_cache (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            artist TEXT NOT NULL,
            track TEXT NOT NULL,
            lyrics_json TEXT,
            cached_at INTEGER NOT NULL,
            UNIQUE(artist, track)
        )",
        [],
    ) {
        error!("Failed to create lyrics_cache table: {}. Cache disabled.", e);
        return None;
    }

    // Where the timing came from ("lrclib", "qq", ...). Added later; NULL rows
    // predate it and are LRCLIB. Fails harmlessly once the column exists.
    let _ = conn.execute("ALTER TABLE lyrics_cache ADD COLUMN source TEXT", []);

    // Create composite index for optimal lookups with TTL filtering
    if let Err(e) = conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_cache_lookup ON lyrics_cache(artist, track, cached_at DESC)",
        [],
    ) {
        warn!("Failed to create composite index: {}", e);
    }

    // Clean up expired entries on startup
    let min_timestamp = now_timestamp() - CACHE_TTL_SECONDS;
    match conn.execute(
        "DELETE FROM lyrics_cache WHERE cached_at <= ?1",
        params![min_timestamp],
    ) {
        Ok(count) if count > 0 => info!("Cleaned up {} expired cache entries", count),
        Ok(_) => {}
        Err(e) => warn!("Failed to clean up expired entries: {}", e),
    }

    info!("Lyrics cache database initialized successfully");
    Some(conn)
}

/// Normalize key for consistent lookups
/// - Collapse whitespace
/// - Lowercase
fn normalize_key(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// Get current Unix timestamp
fn now_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Cache lookup result
pub enum CacheResult {
    /// Lyrics found in cache, and where their timing came from (None: LRCLIB,
    /// or a row written before sources were recorded)
    Hit(Vec<LyricLine>, Option<String>),
    /// Cached as "no lyrics available" (negative cache)
    NegativeHit,
    /// Not in cache or cache unavailable
    Miss,
}

/// Get cached lyrics if available. Returns Miss if cache is disabled or entry expired.
pub fn get_cached_lyrics(artist: &str, track: &str) -> CacheResult {
    let guard = match DB_CONNECTION.lock() {
        Ok(g) => g,
        Err(e) => {
            warn!("Cache mutex poisoned: {}", e);
            return CacheResult::Miss;
        }
    };

    let conn = match guard.as_ref() {
        Some(c) => c,
        None => return CacheResult::Miss,
    };

    let artist_key = normalize_key(artist);
    let track_key = normalize_key(track);
    let min_timestamp = now_timestamp() - CACHE_TTL_SECONDS;

    let result: Result<(Option<String>, Option<String>), _> = conn.query_row(
        "SELECT lyrics_json, source FROM lyrics_cache WHERE artist = ?1 AND track = ?2 AND cached_at > ?3",
        params![artist_key, track_key, min_timestamp],
        |row| Ok((row.get(0)?, row.get(1)?)),
    );

    match result {
        Ok((Some(lyrics_json), source)) => {
            match serde_json::from_str::<Vec<LyricLine>>(&lyrics_json) {
                Ok(lyrics) => {
                    debug!("Cache HIT for '{}' by '{}' ({} lines)", track, artist, lyrics.len());
                    CacheResult::Hit(lyrics, source)
                }
                Err(e) => {
                    error!("Failed to deserialize cached lyrics: {}", e);
                    CacheResult::Miss
                }
            }
        }
        Ok((None, _)) => {
            debug!("Cache NEGATIVE HIT for '{}' by '{}' (instrumental)", track, artist);
            CacheResult::NegativeHit
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            debug!("Cache MISS for '{}' by '{}'", track, artist);
            CacheResult::Miss
        }
        Err(e) => {
            warn!("Database query error: {}", e);
            CacheResult::Miss
        }
    }
}

/// Cache lyrics for future lookups. Silently fails if cache is disabled.
pub fn cache_lyrics(artist: &str, track: &str, lyrics: &[LyricLine], source: &str) {
    let guard = match DB_CONNECTION.lock() {
        Ok(g) => g,
        Err(_) => return,
    };

    let conn = match guard.as_ref() {
        Some(c) => c,
        None => return,
    };

    let artist_key = normalize_key(artist);
    let track_key = normalize_key(track);

    let lyrics_json = match serde_json::to_string(lyrics) {
        Ok(json) => json,
        Err(e) => {
            error!("Failed to serialize lyrics: {}", e);
            return;
        }
    };

    if let Err(e) = conn.execute(
        "INSERT OR REPLACE INTO lyrics_cache (artist, track, lyrics_json, cached_at, source) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![artist_key, track_key, lyrics_json, now_timestamp(), source],
    ) {
        error!("Failed to cache lyrics: {}", e);
    } else {
        debug!("Cached {} lyrics lines for '{}' by '{}'", lyrics.len(), track, artist);
    }
}

/// Cache a negative result (no lyrics found) to avoid repeated API calls.
pub fn cache_no_lyrics(artist: &str, track: &str) {
    let guard = match DB_CONNECTION.lock() {
        Ok(g) => g,
        Err(_) => return,
    };

    let conn = match guard.as_ref() {
        Some(c) => c,
        None => return,
    };

    let artist_key = normalize_key(artist);
    let track_key = normalize_key(track);

    if let Err(e) = conn.execute(
        "INSERT OR REPLACE INTO lyrics_cache (artist, track, lyrics_json, cached_at) VALUES (?1, ?2, NULL, ?3)",
        params![artist_key, track_key, now_timestamp()],
    ) {
        error!("Failed to cache negative result: {}", e);
    } else {
        debug!("Cached NEGATIVE result for '{}' by '{}'", track, artist);
    }
}

/// Clear the entire cache
#[allow(dead_code)]
pub fn clear_cache() {
    let guard = match DB_CONNECTION.lock() {
        Ok(g) => g,
        Err(_) => return,
    };

    if let Some(conn) = guard.as_ref() {
        match conn.execute("DELETE FROM lyrics_cache", []) {
            Ok(count) => info!("Cleared {} entries from lyrics cache", count),
            Err(e) => error!("Failed to clear lyrics cache: {}", e),
        }
    }
}

/// Get cache statistics
#[allow(dead_code)]
pub fn get_cache_stats() -> Option<(usize, i64)> {
    let guard = DB_CONNECTION.lock().ok()?;
    let conn = guard.as_ref()?;

    let count: usize = conn
        .query_row("SELECT COUNT(*) FROM lyrics_cache", [], |row| row.get(0))
        .ok()?;

    let oldest: i64 = conn
        .query_row("SELECT MIN(cached_at) FROM lyrics_cache", [], |row| row.get(0))
        .unwrap_or(0);

    Some((count, oldest))
}

/// Check if cache is available
#[allow(dead_code)]
pub fn is_cache_available() -> bool {
    DB_CONNECTION.lock().map(|g| g.is_some()).unwrap_or(false)
}
