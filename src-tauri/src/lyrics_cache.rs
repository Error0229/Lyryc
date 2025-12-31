//! Persistent lyrics caching module using SQLite
//!
//! Stores fetched lyrics in a SQLite database to avoid redundant API calls
//! across app restarts. Gracefully degrades to no-cache if DB fails.

use crate::types::LyricLine;
use log::{debug, error, info, warn};
use rusqlite::{params, Connection};
use std::path::PathBuf;
use std::sync::OnceLock;

/// Cache TTL: 30 days in seconds
const CACHE_TTL_SECONDS: i64 = 30 * 24 * 60 * 60;

/// Cached database path (computed once)
static DB_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Get the database path in user's app data directory
fn get_db_path() -> Option<&'static PathBuf> {
    DB_PATH
        .get_or_init(|| {
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
        })
        .as_ref()
}

/// Open a new database connection. Returns None on failure.
fn open_connection() -> Option<Connection> {
    let db_path = get_db_path()?;

    let conn = match Connection::open(db_path) {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to open lyrics cache database: {}", e);
            return None;
        }
    };

    // Enable WAL mode for better concurrent access
    let _ = conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;");

    Some(conn)
}

/// Initialize the database schema (called once on first use)
fn ensure_schema(conn: &Connection) -> bool {
    // Create table if not exists
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
        error!("Failed to create lyrics_cache table: {}", e);
        return false;
    }

    // Create index
    if let Err(e) = conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_artist_track ON lyrics_cache(artist, track)",
        [],
    ) {
        warn!("Failed to create index: {}", e);
    }

    true
}

/// Normalize key for consistent lookups
/// - Lowercase
/// - Unicode NFC normalization (via to_lowercase which handles most cases)
/// - Collapse whitespace
/// - Trim
fn normalize_key(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
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
    /// Lyrics found in cache
    Hit(Vec<LyricLine>),
    /// Cached as "no lyrics available" (negative cache)
    NegativeHit,
    /// Not in cache or cache unavailable
    Miss,
}

/// Get cached lyrics if available. Returns Miss if cache is disabled or entry expired.
pub fn get_cached_lyrics(artist: &str, track: &str) -> CacheResult {
    let conn = match open_connection() {
        Some(c) => c,
        None => return CacheResult::Miss,
    };

    if !ensure_schema(&conn) {
        return CacheResult::Miss;
    }

    let artist_key = normalize_key(artist);
    let track_key = normalize_key(track);
    let min_timestamp = now_timestamp() - CACHE_TTL_SECONDS;

    let result: Result<Option<String>, _> = conn.query_row(
        "SELECT lyrics_json FROM lyrics_cache WHERE artist = ?1 AND track = ?2 AND cached_at > ?3",
        params![artist_key, track_key, min_timestamp],
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
    let conn = match open_connection() {
        Some(c) => c,
        None => return,
    };

    if !ensure_schema(&conn) {
        return;
    }

    let artist_key = normalize_key(artist);
    let track_key = normalize_key(track);

    let lyrics_json = match serde_json::to_string(&lyrics) {
        Ok(json) => json,
        Err(e) => {
            error!("Failed to serialize lyrics: {}", e);
            return;
        }
    };

    if let Err(e) = conn.execute(
        "INSERT OR REPLACE INTO lyrics_cache (artist, track, lyrics_json, cached_at) VALUES (?1, ?2, ?3, ?4)",
        params![artist_key, track_key, lyrics_json, now_timestamp()],
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
    let conn = match open_connection() {
        Some(c) => c,
        None => return,
    };

    if !ensure_schema(&conn) {
        return;
    }

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
    let conn = match open_connection() {
        Some(c) => c,
        None => return,
    };

    match conn.execute("DELETE FROM lyrics_cache", []) {
        Ok(count) => info!("Cleared {} entries from lyrics cache", count),
        Err(e) => error!("Failed to clear lyrics cache: {}", e),
    }
}

/// Get cache statistics
#[allow(dead_code)]
pub fn get_cache_stats() -> Option<(usize, i64)> {
    let conn = open_connection()?;

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
    get_db_path().is_some()
}

/// Clean up expired cache entries (older than 30 days)
#[allow(dead_code)]
pub fn cleanup_expired_entries() {
    let conn = match open_connection() {
        Some(c) => c,
        None => return,
    };

    let min_timestamp = now_timestamp() - CACHE_TTL_SECONDS;
    match conn.execute(
        "DELETE FROM lyrics_cache WHERE cached_at <= ?1",
        params![min_timestamp],
    ) {
        Ok(count) if count > 0 => info!("Cleaned up {} expired cache entries", count),
        Ok(_) => {}
        Err(e) => warn!("Failed to clean up expired cache entries: {}", e),
    }
}
