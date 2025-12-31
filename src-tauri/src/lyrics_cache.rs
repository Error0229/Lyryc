//! Persistent lyrics caching module using SQLite
//!
//! Stores fetched lyrics in a SQLite database to avoid redundant API calls
//! across app restarts.

use crate::types::LyricLine;
use log::{debug, error, info, warn};
use rusqlite::{params, Connection};
use std::path::PathBuf;
use std::sync::Mutex;

lazy_static::lazy_static! {
    static ref DB_CONNECTION: Mutex<Option<Connection>> = Mutex::new(None);
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

    // Create directory if it doesn't exist
    if let Err(e) = std::fs::create_dir_all(&app_dir) {
        warn!("Failed to create app data directory: {}", e);
    }

    app_dir.join("lyrics_cache.db")
}

/// Initialize the database connection and create tables
fn init_db() -> Result<Connection, rusqlite::Error> {
    let db_path = get_db_path();
    info!("Initializing lyrics cache database at: {:?}", db_path);

    let conn = Connection::open(&db_path)?;

    // Create lyrics cache table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS lyrics_cache (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            artist TEXT NOT NULL,
            track TEXT NOT NULL,
            lyrics_json TEXT NOT NULL,
            cached_at INTEGER NOT NULL,
            UNIQUE(artist, track)
        )",
        [],
    )?;

    // Create index for faster lookups
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_artist_track ON lyrics_cache(artist, track)",
        [],
    )?;

    info!("Lyrics cache database initialized successfully");
    Ok(conn)
}

/// Get or create the database connection
fn get_connection() -> Result<std::sync::MutexGuard<'static, Option<Connection>>, String> {
    let mut guard = DB_CONNECTION.lock().map_err(|e| e.to_string())?;

    if guard.is_none() {
        match init_db() {
            Ok(conn) => *guard = Some(conn),
            Err(e) => {
                error!("Failed to initialize lyrics cache database: {}", e);
                return Err(e.to_string());
            }
        }
    }

    Ok(guard)
}

/// Normalize key for consistent lookups (lowercase, trimmed)
fn normalize_key(s: &str) -> String {
    s.to_lowercase().trim().to_string()
}

/// Get cached lyrics if available
pub fn get_cached_lyrics(artist: &str, track: &str) -> Option<Vec<LyricLine>> {
    let artist_key = normalize_key(artist);
    let track_key = normalize_key(track);

    let guard = match get_connection() {
        Ok(g) => g,
        Err(e) => {
            warn!("Failed to get DB connection: {}", e);
            return None;
        }
    };

    let conn = guard.as_ref()?;

    let result: Result<String, _> = conn.query_row(
        "SELECT lyrics_json FROM lyrics_cache WHERE artist = ?1 AND track = ?2",
        params![artist_key, track_key],
        |row| row.get(0),
    );

    match result {
        Ok(lyrics_json) => match serde_json::from_str::<Vec<LyricLine>>(&lyrics_json) {
            Ok(lyrics) => {
                debug!(
                    "Cache HIT for '{}' by '{}' ({} lines)",
                    track,
                    artist,
                    lyrics.len()
                );
                Some(lyrics)
            }
            Err(e) => {
                warn!("Failed to deserialize cached lyrics: {}", e);
                None
            }
        },
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            debug!("Cache MISS for '{}' by '{}'", track, artist);
            None
        }
        Err(e) => {
            warn!("Database query error: {}", e);
            None
        }
    }
}

/// Cache lyrics for future lookups
pub fn cache_lyrics(artist: &str, track: &str, lyrics: Vec<LyricLine>) {
    if lyrics.is_empty() {
        return; // Don't cache empty results
    }

    let artist_key = normalize_key(artist);
    let track_key = normalize_key(track);

    let guard = match get_connection() {
        Ok(g) => g,
        Err(e) => {
            warn!("Failed to get DB connection for caching: {}", e);
            return;
        }
    };

    let conn = match guard.as_ref() {
        Some(c) => c,
        None => return,
    };

    let lyrics_json = match serde_json::to_string(&lyrics) {
        Ok(json) => json,
        Err(e) => {
            warn!("Failed to serialize lyrics: {}", e);
            return;
        }
    };

    let cached_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    // Use INSERT OR REPLACE to handle duplicates
    match conn.execute(
        "INSERT OR REPLACE INTO lyrics_cache (artist, track, lyrics_json, cached_at) VALUES (?1, ?2, ?3, ?4)",
        params![artist_key, track_key, lyrics_json, cached_at],
    ) {
        Ok(_) => {
            debug!("Cached {} lyrics lines for '{}' by '{}'", lyrics.len(), track, artist);
        }
        Err(e) => {
            error!("Failed to cache lyrics: {}", e);
        }
    }
}

/// Clear the entire cache
#[allow(dead_code)]
pub fn clear_cache() {
    let guard = match get_connection() {
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
    let guard = get_connection().ok()?;
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
