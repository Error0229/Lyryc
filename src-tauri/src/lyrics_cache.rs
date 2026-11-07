//! Persistent lyrics caching module using SQLite
//!
//! Stores fetched lyrics in a SQLite database to avoid redundant API calls
//! across app restarts. Gracefully degrades to no-cache if DB fails.

use crate::types::LyricLine;
use log::{debug, error, info, warn};
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Cache TTL: 30 days in seconds
const CACHE_TTL_SECONDS: i64 = 30 * 24 * 60 * 60;

/// The `source` of lyrics the user edited (Settings → Saved lyrics). They
/// never expire: a lookup would throw the user's fixes away.
const EDITED: &str = "edited";

mod editor;
pub use editor::{saved_song, saved_song_change, saved_songs};

lazy_static::lazy_static! {
    static ref DB_PATH: Option<PathBuf> = db_path();
    /// Single database connection protected by mutex.
    /// SQLite with WAL mode handles concurrent reads well, mutex serializes writes.
    static ref DB_CONNECTION: Mutex<Option<Connection>> = Mutex::new(init_db());
}

const DB_FILE: &str = "lyrics_cache.db";

/// Where per-user app data goes (`%LOCALAPPDATA%` on Windows).
fn data_dir() -> Option<PathBuf> {
    std::env::var("LOCALAPPDATA")
        .or_else(|_| std::env::var("XDG_DATA_HOME"))
        .or_else(|_| std::env::var("HOME").map(|h| format!("{}/.local/share", h)))
        .ok()
        .map(PathBuf::from)
}

/// `%LOCALAPPDATA%\com.lyryc.app\lyrics_cache.db`, beside the log and the
/// word-timing files, so it goes with them (the uninstaller's "delete app
/// data" included).
fn db_path() -> Option<PathBuf> {
    let dir = data_dir()?.join("com.lyryc.app");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        error!("Failed to create app data directory: {}. Cache disabled.", e);
        return None;
    }
    Some(dir.join(DB_FILE))
}

/// Where older versions kept the cache; `init_db` moves it from there once.
fn old_db_path() -> Option<PathBuf> {
    Some(data_dir()?.join("Lyryc").join(DB_FILE))
}

/// Copy the old cache to `to` (VACUUM INTO: one consistent file, whatever
/// sat in its WAL), then delete it and its folder. Nothing happens if `to`
/// exists, and a failed copy leaves the old cache where it was.
fn move_old_cache(from: &Path, to: &Path) {
    if !from.exists() || to.exists() {
        return;
    }
    let Some(target) = to.to_str() else {
        warn!("Couldn't move the lyrics cache: SQLite can't take the path {:?}", to);
        return;
    };
    let copied = Connection::open(from).and_then(|c| c.execute("VACUUM INTO ?1", params![target]));
    if let Err(e) = copied {
        warn!("Couldn't move the lyrics cache from {:?}: {}", from, e);
        let _ = std::fs::remove_file(to);
        return;
    }
    if !to.exists() {
        warn!("Couldn't move the lyrics cache: no copy at {:?}", to);
        return;
    }
    info!("Moved the lyrics cache from {:?} to {:?}", from, to);
    for file in db_files(from) {
        let _ = std::fs::remove_file(file);
    }
    if let Some(dir) = from.parent() {
        let _ = std::fs::remove_dir(dir); // only if nothing else is in it
    }
}

/// The database and the WAL files beside it.
fn db_files(db: &Path) -> [PathBuf; 3] {
    let name = db.as_os_str();
    let with = |suffix: &str| {
        let mut n = name.to_owned();
        n.push(suffix);
        PathBuf::from(n)
    };
    [db.to_path_buf(), with("-wal"), with("-shm")]
}

/// Initialize database connection and schema. Called once at startup.
fn init_db() -> Option<Connection> {
    let db_path = DB_PATH.clone()?;
    if let Some(old) = old_db_path() {
        move_old_cache(&old, &db_path);
    }
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

    if let Err(e) = create_lyrics_table(&conn) {
        error!("Failed to create lyrics_cache table: {}. Cache disabled.", e);
        return None;
    }

    create_aligned_table(&conn);
    create_translation_table(&conn);

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
        "DELETE FROM lyrics_cache WHERE cached_at <= ?1 AND source IS NOT ?2",
        params![min_timestamp, EDITED],
    ) {
        Ok(count) if count > 0 => info!("Cleaned up {} expired cache entries", count),
        Ok(_) => {}
        Err(e) => warn!("Failed to clean up expired entries: {}", e),
    }

    info!("Lyrics cache database initialized successfully");
    Some(conn)
}

fn create_lyrics_table(conn: &Connection) -> rusqlite::Result<()> {
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
    )?;
    // Where the timing came from ("lrclib", "qq", ...). Added later; NULL rows
    // predate it and are LRCLIB. Fails harmlessly once the column exists.
    let _ = conn.execute("ALTER TABLE lyrics_cache ADD COLUMN source TEXT", []);
    Ok(())
}

/// Normalize key for consistent lookups
/// - Collapse whitespace
/// - Lowercase
fn normalize_key(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// The cache's identity for a song: (artist, track), normalized.
pub fn song_key(artist: &str, track: &str) -> (String, String) {
    (normalize_key(artist), normalize_key(track))
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
    /// Known to have no words at all: nothing to look for.
    Instrumental,
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
        "SELECT lyrics_json, source FROM lyrics_cache WHERE artist = ?1 AND track = ?2 AND (cached_at > ?3 OR source IS ?4)",
        params![artist_key, track_key, min_timestamp, EDITED],
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
        Ok((None, Some(source))) if source == INSTRUMENTAL => {
            debug!("Cache HIT for '{}' by '{}': instrumental", track, artist);
            CacheResult::Instrumental
        }
        Ok((None, _)) => {
            debug!("Cache NEGATIVE HIT for '{}' by '{}'", track, artist);
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

/// The `source` of a lyric-less row for an instrumental; other lyric-less rows
/// (source NULL) mean "nothing found".
const INSTRUMENTAL: &str = "instrumental";

/// Cache a negative result (no lyrics found) to avoid repeated API calls.
pub fn cache_no_lyrics(artist: &str, track: &str) {
    cache_without_lyrics(artist, track, None);
}

/// Cache that the song has no words: every later lookup ends here.
pub fn cache_instrumental(artist: &str, track: &str) {
    cache_without_lyrics(artist, track, Some(INSTRUMENTAL));
}

fn cache_without_lyrics(artist: &str, track: &str, source: Option<&str>) {
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
        "INSERT OR REPLACE INTO lyrics_cache (artist, track, lyrics_json, cached_at, source) VALUES (?1, ?2, NULL, ?3, ?4)",
        params![artist_key, track_key, now_timestamp(), source],
    ) {
        error!("Failed to cache negative result: {}", e);
    } else {
        debug!("Cached no lyrics ({}) for '{}' by '{}'", source.unwrap_or("not found"), track, artist);
    }
}

/// Drop the song's entry, found or not: the user says it is wrong.
pub fn forget_lyrics(artist: &str, track: &str) {
    let guard = match DB_CONNECTION.lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    let Some(conn) = guard.as_ref() else {
        return;
    };
    if let Err(e) = conn.execute(
        "DELETE FROM lyrics_cache WHERE artist = ?1 AND track = ?2",
        params![normalize_key(artist), normalize_key(track)],
    ) {
        error!("Failed to forget cached lyrics: {}", e);
    }
}

// ---------------------------------------------------------------------------
// Pre-aligned word timing (src/prealign). Keyed by the YouTube videoId and a
// hash of the lyric text it was aligned to; kept until the user clears it.
// `lines_json` NULL means the alignment was rejected: don't download again.

/// A cached alignment for (videoId, text hash).
#[derive(Debug, PartialEq)]
pub enum AlignedCache {
    /// The aligner's raw output (JSON), to be checked against the lyrics on use.
    Hit(String),
    /// Aligned before and rejected as a whole.
    Rejected,
    Miss,
}

fn create_aligned_table(conn: &Connection) {
    if let Err(e) = conn.execute(
        "CREATE TABLE IF NOT EXISTS aligned_cache (
            video_id TEXT NOT NULL,
            text_hash TEXT NOT NULL,
            lines_json TEXT,
            cached_at INTEGER NOT NULL,
            PRIMARY KEY (video_id, text_hash)
        )",
        [],
    ) {
        warn!("Failed to create aligned_cache table: {}", e);
    }
}

fn get_aligned_in(conn: &Connection, video_id: &str, text_hash: &str) -> AlignedCache {
    let row: Result<Option<String>, _> = conn.query_row(
        "SELECT lines_json FROM aligned_cache WHERE video_id = ?1 AND text_hash = ?2",
        params![video_id, text_hash],
        |row| row.get(0),
    );
    match row {
        Ok(Some(json)) => AlignedCache::Hit(json),
        Ok(None) => AlignedCache::Rejected,
        Err(rusqlite::Error::QueryReturnedNoRows) => AlignedCache::Miss,
        Err(e) => {
            warn!("aligned_cache query error: {}", e);
            AlignedCache::Miss
        }
    }
}

fn cache_aligned_in(conn: &Connection, video_id: &str, text_hash: &str, lines_json: Option<&str>) {
    if let Err(e) = conn.execute(
        "INSERT OR REPLACE INTO aligned_cache (video_id, text_hash, lines_json, cached_at) VALUES (?1, ?2, ?3, ?4)",
        params![video_id, text_hash, lines_json, now_timestamp()],
    ) {
        error!("Failed to cache alignment: {}", e);
    }
}

pub fn get_aligned(video_id: &str, text_hash: &str) -> AlignedCache {
    match DB_CONNECTION.lock() {
        Ok(guard) => guard.as_ref().map_or(AlignedCache::Miss, |c| get_aligned_in(c, video_id, text_hash)),
        Err(_) => AlignedCache::Miss,
    }
}

/// Store an alignment (`None`: rejected).
pub fn cache_aligned(video_id: &str, text_hash: &str, lines_json: Option<&str>) {
    if let Ok(guard) = DB_CONNECTION.lock() {
        if let Some(conn) = guard.as_ref() {
            cache_aligned_in(conn, video_id, text_hash, lines_json);
        }
    }
}

fn create_translation_table(conn: &Connection) {
    if let Err(e) = conn.execute(
        "CREATE TABLE IF NOT EXISTS translation_cache (
            key TEXT PRIMARY KEY,
            result_json TEXT NOT NULL,
            cached_at INTEGER NOT NULL
        )",
        [],
    ) {
        warn!("Failed to create translation_cache table: {}", e);
    }
}

fn get_translation_in(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row("SELECT result_json FROM translation_cache WHERE key = ?1", params![key], |row| row.get(0))
        .map_err(|e| {
            if !matches!(e, rusqlite::Error::QueryReturnedNoRows) {
                warn!("translation_cache query error: {}", e);
            }
        })
        .ok()
}

fn cache_translation_in(conn: &Connection, key: &str, result_json: &str) {
    if let Err(e) = conn.execute(
        "INSERT OR REPLACE INTO translation_cache (key, result_json, cached_at) VALUES (?1, ?2, ?3)",
        params![key, result_json, now_timestamp()],
    ) {
        error!("Failed to cache translation: {}", e);
    }
}

/// A stored translation (JSON) by `translation::cache::key`.
pub fn get_translation(key: &str) -> Option<String> {
    DB_CONNECTION.lock().ok()?.as_ref().and_then(|c| get_translation_in(c, key))
}

pub fn cache_translation(key: &str, result_json: &str) {
    if let Ok(guard) = DB_CONNECTION.lock() {
        if let Some(conn) = guard.as_ref() {
            cache_translation_in(conn, key, result_json);
        }
    }
}

/// What's saved: entries across the three tables, and bytes on disk.
#[derive(serde::Serialize)]
pub struct CacheSize {
    entries: u64,
    bytes: u64,
}

/// Opens the cache first (moving an old one), so the bytes are the real file's.
fn size() -> CacheSize {
    let entries = DB_CONNECTION.lock().ok().and_then(|g| g.as_ref().map(entries_in)).unwrap_or(0);
    let bytes = DB_PATH.as_ref().map_or(0, |p| db_files(p).iter().filter_map(|f| std::fs::metadata(f).ok()).map(|m| m.len()).sum());
    CacheSize { entries, bytes }
}

fn entries_in(conn: &Connection) -> u64 {
    conn.query_row(
        "SELECT (SELECT COUNT(*) FROM lyrics_cache) + (SELECT COUNT(*) FROM translation_cache) + (SELECT COUNT(*) FROM aligned_cache)",
        [],
        |r| r.get(0),
    )
    .unwrap_or(0)
}

/// Forget everything: lyrics, translations and word timings.
fn clear_in(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "DELETE FROM lyrics_cache; DELETE FROM translation_cache; DELETE FROM aligned_cache;
         VACUUM; PRAGMA wal_checkpoint(TRUNCATE);",
    )
}

fn clear() -> Result<CacheSize, String> {
    {
        let guard = DB_CONNECTION.lock().map_err(|e| e.to_string())?;
        let conn = guard.as_ref().ok_or("the cache isn't available")?;
        clear_in(conn).map_err(|e| e.to_string())?;
    }
    info!("Cleared the lyrics cache");
    Ok(size())
}

/// Settings → Lyrics & sync: how much is saved. It waits for the cache's
/// lock and reads file sizes, so it runs off the main thread.
#[tauri::command]
pub async fn cache_size() -> Result<CacheSize, String> {
    tokio::task::spawn_blocking(size).await.map_err(|e| e.to_string())
}

/// Settings → Lyrics & sync → Clear: empties the cache (VACUUM included, off
/// the main thread) and returns what's left.
#[tauri::command]
pub async fn cache_clear() -> Result<CacheSize, String> {
    tokio::task::spawn_blocking(clear).await.map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translations_are_kept_by_key() {
        let conn = Connection::open_in_memory().unwrap();
        create_translation_table(&conn);
        assert_eq!(get_translation_in(&conn, "k"), None);
        cache_translation_in(&conn, "k", "first");
        cache_translation_in(&conn, "k", "second");
        assert_eq!(get_translation_in(&conn, "k").as_deref(), Some("second"), "the newest answer wins");
    }

    #[test]
    fn alignments_are_keyed_by_video_and_text() {
        let conn = Connection::open_in_memory().unwrap();
        create_aligned_table(&conn);
        assert_eq!(get_aligned_in(&conn, "vid1", "h1"), AlignedCache::Miss);
        cache_aligned_in(&conn, "vid1", "h1", Some("[1]"));
        cache_aligned_in(&conn, "vid1", "h2", None);
        assert_eq!(get_aligned_in(&conn, "vid1", "h1"), AlignedCache::Hit("[1]".into()));
        assert_eq!(get_aligned_in(&conn, "vid1", "h2"), AlignedCache::Rejected, "other lyrics, same video");
        assert_eq!(get_aligned_in(&conn, "vid2", "h1"), AlignedCache::Miss, "same lyrics, other upload");
        cache_aligned_in(&conn, "vid1", "h1", Some("[2]"));
        assert_eq!(get_aligned_in(&conn, "vid1", "h1"), AlignedCache::Hit("[2]".into()));
    }

    #[test]
    fn clearing_forgets_lyrics_translations_and_timings() {
        let conn = Connection::open_in_memory().unwrap();
        create_lyrics_table(&conn).unwrap();
        create_aligned_table(&conn);
        create_translation_table(&conn);
        conn.execute("INSERT INTO lyrics_cache (artist, track, cached_at) VALUES ('a', 't', 0)", []).unwrap();
        cache_translation_in(&conn, "k", "x");
        cache_aligned_in(&conn, "v", "h", Some("[1]"));
        assert_eq!(entries_in(&conn), 3);
        clear_in(&conn).unwrap();
        assert_eq!(entries_in(&conn), 0);
        assert_eq!(get_translation_in(&conn, "k"), None);
        assert_eq!(get_aligned_in(&conn, "v", "h"), AlignedCache::Miss);
    }

    #[test]
    fn the_old_cache_moves_once() {
        let root = std::env::temp_dir().join(format!("lyryc-cache-move-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (old, new) = (root.join("Lyryc").join(DB_FILE), root.join("com.lyryc.app").join(DB_FILE));
        std::fs::create_dir_all(old.parent().unwrap()).unwrap();
        std::fs::create_dir_all(new.parent().unwrap()).unwrap();
        {
            let c = Connection::open(&old).unwrap();
            c.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
            create_translation_table(&c);
            cache_translation_in(&c, "k", "kept");
        }
        move_old_cache(&old, &new);
        let c = Connection::open(&new).unwrap();
        assert_eq!(get_translation_in(&c, "k").as_deref(), Some("kept"));
        assert!(!old.exists() && !old.parent().unwrap().exists(), "the old cache and its folder are gone");

        // A cache already in the new place is never overwritten.
        std::fs::create_dir_all(old.parent().unwrap()).unwrap();
        Connection::open(&old).unwrap().execute_batch("CREATE TABLE stale (x)").unwrap();
        move_old_cache(&old, &new);
        assert_eq!(get_translation_in(&c, "k").as_deref(), Some("kept"));
        drop(c);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_old_cache_that_cannot_be_read_stays_where_it_is() {
        let root = std::env::temp_dir().join(format!("lyryc-cache-bad-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (old, new) = (root.join("Lyryc").join(DB_FILE), root.join("com.lyryc.app").join(DB_FILE));
        std::fs::create_dir_all(old.parent().unwrap()).unwrap();
        std::fs::create_dir_all(new.parent().unwrap()).unwrap();
        std::fs::write(&old, b"not a database, only bytes that look nothing like a SQLite header").unwrap();
        move_old_cache(&old, &new);
        assert!(old.exists(), "the old file is kept");
        assert!(!new.exists(), "no half-made copy");
        let _ = std::fs::remove_dir_all(&root);
    }
}
