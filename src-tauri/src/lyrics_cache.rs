//! Persistent lyrics caching module using SQLite
//!
//! Stores fetched lyrics in a SQLite database to avoid redundant API calls
//! across app restarts.

use crate::types::LyricLine;
use log::{debug, error, info};
use rusqlite::{params, Connection};
use std::path::PathBuf;
use std::sync::Mutex;

lazy_static::lazy_static! {
    static ref DB_CONNECTION: Mutex<Connection> = Mutex::new(init_db());
}

/// Get the database path in user's app data directory
fn get_db_path() -> PathBuf {
    // Use platform-specific app data directory via environment variables
    let base_dir = std::env::var("LOCALAPPDATA") // Windows
        .or_else(|_| std::env::var("XDG_DATA_HOME")) // Linux
        .or_else(|_| std::env::var("HOME").map(|h| format!("{}/.local/share", h))) // Linux fallback
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));

    let app_dir = base_dir.join("Lyryc");

    // Create directory if it doesn't exist - fail fast if we can't
    std::fs::create_dir_all(&app_dir)
        .expect("Failed to create app data directory for lyrics cache");

    app_dir.join("lyrics_cache.db")
}

/// Initialize the database connection and create tables.
/// Panics on failure - fail fast rather than silent degradation.
fn init_db() -> Connection {
    let db_path = get_db_path();
    info!("Initializing lyrics cache database at: {:?}", db_path);

    let conn = Connection::open(&db_path)
        .expect("Failed to open lyrics cache database");

    // Create lyrics cache table
    // lyrics_json can be NULL to indicate "no lyrics found" (negative cache)
    conn.execute(
        "CREATE TABLE IF NOT EXISTS lyrics_cache (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            artist TEXT NOT NULL,
            track TEXT NOT NULL,
            lyrics_json TEXT,
            cached_at INTEGER NOT NULL,
            UNIQUE(artist, track)
        )",
        [],
    )
    .expect("Failed to create lyrics_cache table");

    // Create index for faster lookups
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_artist_track ON lyrics_cache(artist, track)",
        [],
    )
    .expect("Failed to create index on lyrics_cache");

    info!("Lyrics cache database initialized successfully");
    conn
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
    /// Not in cache
    Miss,
}

/// Get cached lyrics if available
pub fn get_cached_lyrics(artist: &str, track: &str) -> CacheResult {
    let artist_key = normalize_key(artist);
    let track_key = normalize_key(track);

    let conn = DB_CONNECTION.lock().expect("DB mutex poisoned");

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
            // NULL lyrics_json means "no lyrics found" (negative cache)
            debug!("Cache NEGATIVE HIT for '{}' by '{}' (instrumental/no lyrics)", track, artist);
            CacheResult::NegativeHit
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            debug!("Cache MISS for '{}' by '{}'", track, artist);
            CacheResult::Miss
        }
        Err(e) => {
            error!("Database query error: {}", e);
            CacheResult::Miss
        }
    }
}

/// Cache lyrics for future lookups
pub fn cache_lyrics(artist: &str, track: &str, lyrics: Vec<LyricLine>) {
    let artist_key = normalize_key(artist);
    let track_key = normalize_key(track);

    let conn = DB_CONNECTION.lock().expect("DB mutex poisoned");

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

    // Use INSERT OR REPLACE to handle duplicates
    if let Err(e) = conn.execute(
        "INSERT OR REPLACE INTO lyrics_cache (artist, track, lyrics_json, cached_at) VALUES (?1, ?2, ?3, ?4)",
        params![artist_key, track_key, lyrics_json, cached_at],
    ) {
        error!("Failed to cache lyrics: {}", e);
    } else {
        debug!("Cached {} lyrics lines for '{}' by '{}'", lyrics.len(), track, artist);
    }
}

/// Cache a negative result (no lyrics found) to avoid repeated API calls
pub fn cache_no_lyrics(artist: &str, track: &str) {
    let artist_key = normalize_key(artist);
    let track_key = normalize_key(track);

    let conn = DB_CONNECTION.lock().expect("DB mutex poisoned");

    let cached_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    // Insert with NULL lyrics_json to indicate "no lyrics found"
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
    let conn = DB_CONNECTION.lock().expect("DB mutex poisoned");

    match conn.execute("DELETE FROM lyrics_cache", []) {
        Ok(count) => {
            info!("Cleared {} entries from lyrics cache", count);
        }
        Err(e) => {
            error!("Failed to clear lyrics cache: {}", e);
        }
    }
}

/// Get cache statistics
#[allow(dead_code)]
pub fn get_cache_stats() -> Option<(usize, i64)> {
    let conn = DB_CONNECTION.lock().ok()?;

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
