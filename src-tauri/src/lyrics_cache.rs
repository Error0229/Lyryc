//! Persistent lyrics caching module using SQLite
//!
//! Stores fetched lyrics in a SQLite database to avoid redundant API calls
//! across app restarts. Gracefully degrades to no-cache if DB fails.

use crate::types::LyricLine;
use log::{debug, error, info, warn};
use rusqlite::{params, Connection};
use std::path::PathBuf;
use std::sync::Mutex;

lazy_static::lazy_static! {
    static ref DB_CONNECTION: Mutex<Option<Connection>> = Mutex::new(try_init_db());
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

/// Try to initialize the database. Returns None on failure (graceful degradation).
fn try_init_db() -> Option<Connection> {
    let db_path = get_db_path()?;
    info!("Initializing lyrics cache database at: {:?}", db_path);

    let conn = match Connection::open(&db_path) {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to open lyrics cache database: {}. Cache disabled.", e);
            return None;
        }
    };

    // Create lyrics cache table
    // lyrics_json can be NULL to indicate "no lyrics found" (negative cache)
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

    // Create index for faster lookups
    if let Err(e) = conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_artist_track ON lyrics_cache(artist, track)",
        [],
    ) {
        warn!("Failed to create index on lyrics_cache: {}. Continuing without index.", e);
    }

    info!("Lyrics cache database initialized successfully");
    Some(conn)
}

/// Normalize key for consistent lookups (lowercase, trimmed)
fn normalize_key(s: &str) -> String {
    s.to_lowercase().trim().to_string()
}

/// Cache lookup result
pub enum CacheResult {
    /// Lyrics found in cache
    Hit(Vec<LyricLine>),
    /// Cached as "no lyrics available" (negative cache)
    NegativeHit,
    /// Not in cache or cache unavailable
    Miss,
}

/// Get cached lyrics if available. Returns Miss if cache is disabled.
pub fn get_cached_lyrics(artist: &str, track: &str) -> CacheResult {
    let artist_key = normalize_key(artist);
    let track_key = normalize_key(track);

    let guard = match DB_CONNECTION.lock() {
        Ok(g) => g,
        Err(e) => {
            warn!("Cache mutex poisoned: {}", e);
            return CacheResult::Miss;
        }
    };

    let conn = match guard.as_ref() {
        Some(c) => c,
        None => return CacheResult::Miss, // Cache disabled
    };

    let result: Result<Option<String>, _> = conn.query_row(
        "SELECT lyrics_json FROM lyrics_cache WHERE artist = ?1 AND track = ?2",
        params![artist_key, track_key],
        |row| row.get(0),
    );

    match result {
        Ok(Some(lyrics_json)) => {
            match serde_json::from_str::<Vec<LyricLine>>(&lyrics_json) {
                Ok(lyrics) => {
                    debug!(
                        "Cache HIT for '{}' by '{}' ({} lines)",
                        track,
                        artist,
                        lyrics.len()
                    );
                    CacheResult::Hit(lyrics)
                }
                Err(e) => {
                    error!("Failed to deserialize cached lyrics: {}", e);
                    CacheResult::Miss
                }
            }
        }
        Ok(None) => {
            debug!(
                "Cache NEGATIVE HIT for '{}' by '{}' (instrumental/no lyrics)",
                track, artist
            );
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
pub fn cache_lyrics(artist: &str, track: &str, lyrics: Vec<LyricLine>) {
    let artist_key = normalize_key(artist);
    let track_key = normalize_key(track);

    let guard = match DB_CONNECTION.lock() {
        Ok(g) => g,
        Err(_) => return,
    };

    let conn = match guard.as_ref() {
        Some(c) => c,
        None => return, // Cache disabled
    };

    let lyrics_json = match serde_json::to_string(&lyrics) {
        Ok(json) => json,
        Err(e) => {
            error!("Failed to serialize lyrics: {}", e);
            return;
        }
    };

    let cached_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    if let Err(e) = conn.execute(
        "INSERT OR REPLACE INTO lyrics_cache (artist, track, lyrics_json, cached_at) VALUES (?1, ?2, ?3, ?4)",
        params![artist_key, track_key, lyrics_json, cached_at],
    ) {
        error!("Failed to cache lyrics: {}", e);
    } else {
        debug!(
            "Cached {} lyrics lines for '{}' by '{}'",
            lyrics.len(),
            track,
            artist
        );
    }
}

/// Cache a negative result (no lyrics found) to avoid repeated API calls.
pub fn cache_no_lyrics(artist: &str, track: &str) {
    let artist_key = normalize_key(artist);
    let track_key = normalize_key(track);

    let guard = match DB_CONNECTION.lock() {
        Ok(g) => g,
        Err(_) => return,
    };

    let conn = match guard.as_ref() {
        Some(c) => c,
        None => return, // Cache disabled
    };

    let cached_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    if let Err(e) = conn.execute(
        "INSERT OR REPLACE INTO lyrics_cache (artist, track, lyrics_json, cached_at) VALUES (?1, ?2, NULL, ?3)",
        params![artist_key, track_key, cached_at],
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
            Ok(count) => {
                info!("Cleared {} entries from lyrics cache", count);
            }
            Err(e) => {
                error!("Failed to clear lyrics cache: {}", e);
            }
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
        .query_row("SELECT MIN(cached_at) FROM lyrics_cache", [], |row| {
            row.get(0)
        })
        .unwrap_or(0);

    Some((count, oldest))
}

/// Check if cache is available
#[allow(dead_code)]
pub fn is_cache_available() -> bool {
    DB_CONNECTION
        .lock()
        .map(|g| g.is_some())
        .unwrap_or(false)
}
