//! Settings → Lyrics & sync → Saved lyrics: what the cache holds for each song,
//! and the user's fixes to it.
//!
//! Translations and word timings aren't stored under a song: they're keyed by
//! its lyric text (and the video, for timings). So they're found here the same
//! way the app finds them: by the song's text. Changing the words leaves them
//! behind for good, so a save that changes the words deletes them.

use super::{now_timestamp, song_key, CACHE_TTL_SECONDS, DB_CONNECTION, EDITED, INSTRUMENTAL};
use crate::prealign::text_key;
use crate::types::LyricLine;
use log::info;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter};

/// Tells the overlay a song's saved lyrics changed, so it shows the new ones.
const CHANGED_EVENT: &str = "saved-lyrics-changed";

/// A song in the list.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SavedSong {
    /// Normalized (lowercase), as the cache keys it.
    artist: String,
    track: String,
    /// "lyrics", "missing" (nothing was found) or "instrumental".
    kind: &'static str,
    /// Where the lyrics came from ("lrclib", "ytmusic", "edited", ...).
    source: Option<String>,
    lines: usize,
    cached_at: i64,
    /// When it's looked up again; None for edited lyrics, which are kept.
    expires_at: Option<i64>,
    translations: usize,
    timings: usize,
}

/// A saved translation of the song: one line per lyric line.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SavedTranslation {
    key: String,
    lines: Vec<String>,
}

/// Saved word timing of the song for one YouTube video. `rejected`: aligned
/// once and thrown away; kept so the audio isn't downloaded again.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SavedTiming {
    video_id: String,
    hash: String,
    #[serde(default)]
    rejected: bool,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SavedSongDetail {
    song: SavedSong,
    lines: Vec<LyricLine>,
    translations: Vec<SavedTranslation>,
    timings: Vec<SavedTiming>,
}

/// What the user did to a song.
#[derive(Deserialize, Debug)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Change {
    /// New lines (any order) and edited translation lines. The translations
    /// are dropped instead when the words changed, the word timing when the
    /// lines changed at all: it would replace the new times as the song plays.
    Save { lines: Vec<LyricLine>, translations: Vec<SavedTranslation> },
    /// Forget the lyrics (or "nothing found"): looked up again next time.
    Forget,
    /// A song without lyrics: nothing was found, or it has no words.
    Mark { instrumental: bool },
    ForgetTranslation { key: String },
    ForgetTiming { video_id: String, hash: String },
}

struct Row {
    artist: String,
    track: String,
    lines: Option<Vec<LyricLine>>,
    source: Option<String>,
    cached_at: i64,
}

fn row_of(r: &rusqlite::Row) -> rusqlite::Result<Row> {
    let json: Option<String> = r.get(2)?;
    Ok(Row {
        artist: r.get(0)?,
        track: r.get(1)?,
        lines: json.and_then(|j| serde_json::from_str(&j).ok()),
        source: r.get(3)?,
        cached_at: r.get(4)?,
    })
}

const ROW: &str = "SELECT artist, track, lyrics_json, source, cached_at FROM lyrics_cache";

fn rows(conn: &Connection) -> rusqlite::Result<Vec<Row>> {
    let mut q = conn.prepare(&format!("{ROW} ORDER BY cached_at DESC"))?;
    let rows = q.query_map([], row_of)?.collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

fn row(conn: &Connection, artist: &str, track: &str) -> rusqlite::Result<Option<Row>> {
    conn.query_row(&format!("{ROW} WHERE artist = ?1 AND track = ?2"), params![artist, track], row_of).optional()
}

/// A song's words, the way a translation records them.
fn words(lines: &[LyricLine]) -> String {
    lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n")
}

/// Every saved translation: (key, the words it translates, its lines).
fn all_translations(conn: &Connection) -> rusqlite::Result<Vec<(String, String, Vec<String>)>> {
    let mut q = conn.prepare("SELECT key, result_json FROM translation_cache")?;
    let pairs = q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    let mut out = Vec::new();
    for pair in pairs {
        let (key, json) = pair?;
        let Ok(v) = serde_json::from_str::<Value>(&json) else { continue };
        let lines = v["translated_lines"].as_array().map(Vec::as_slice).unwrap_or_default();
        let field = |l: &Value, f: &str| l[f].as_str().unwrap_or_default().to_string();
        let original = lines.iter().map(|l| field(l, "original_text")).collect::<Vec<_>>().join("\n");
        out.push((key, original, lines.iter().map(|l| field(l, "translated_text")).collect()));
    }
    Ok(out)
}

fn all_timings(conn: &Connection) -> rusqlite::Result<Vec<SavedTiming>> {
    let mut q = conn.prepare("SELECT video_id, text_hash, lines_json IS NULL FROM aligned_cache")?;
    let timings = q.query_map([], |r| Ok(SavedTiming { video_id: r.get(0)?, hash: r.get(1)?, rejected: r.get(2)? }))?.collect::<rusqlite::Result<_>>()?;
    Ok(timings)
}

/// The cache, joined: each song with its translations and timings.
struct Index {
    translations: Vec<(String, String, Vec<String>)>,
    timings: Vec<SavedTiming>,
}

impl Index {
    fn load(conn: &Connection) -> rusqlite::Result<Self> {
        Ok(Index { translations: all_translations(conn)?, timings: all_timings(conn)? })
    }

    fn translations(&self, lines: &[LyricLine]) -> Vec<SavedTranslation> {
        let text = words(lines);
        self.translations
            .iter()
            .filter(|(_, original, _)| !lines.is_empty() && *original == text)
            .map(|(key, _, lines)| SavedTranslation { key: key.clone(), lines: lines.clone() })
            .collect()
    }

    fn timings(&self, lines: &[LyricLine]) -> Vec<SavedTiming> {
        let hash = text_key(lines);
        self.timings.iter().filter(|t| !lines.is_empty() && t.hash == hash).cloned().collect()
    }

    fn song(&self, row: &Row) -> SavedSong {
        let lines = row.lines.as_deref().unwrap_or_default();
        let edited = row.source.as_deref() == Some(EDITED);
        SavedSong {
            artist: row.artist.clone(),
            track: row.track.clone(),
            kind: match (&row.lines, row.source.as_deref()) {
                (Some(_), _) => "lyrics",
                (None, Some(INSTRUMENTAL)) => "instrumental",
                (None, _) => "missing",
            },
            source: row.source.clone(),
            lines: lines.len(),
            cached_at: row.cached_at,
            expires_at: (!edited).then_some(row.cached_at + CACHE_TTL_SECONDS),
            translations: self.translations(lines).len(),
            timings: self.timings(lines).len(),
        }
    }

    fn detail(&self, row: Row) -> SavedSongDetail {
        let song = self.song(&row);
        let lines = row.lines.unwrap_or_default();
        SavedSongDetail { song, translations: self.translations(&lines), timings: self.timings(&lines), lines }
    }
}

fn songs_in(conn: &Connection) -> rusqlite::Result<Vec<SavedSong>> {
    let index = Index::load(conn)?;
    Ok(rows(conn)?.iter().map(|r| index.song(r)).collect())
}

fn detail_in(conn: &Connection, artist: &str, track: &str) -> rusqlite::Result<Option<SavedSongDetail>> {
    let (artist, track) = song_key(artist, track);
    let Some(row) = row(conn, &artist, &track)? else { return Ok(None) };
    Ok(Some(Index::load(conn)?.detail(row)))
}

/// Puts edited lines into a saved translation, if it is one of these words.
fn edit_translation(conn: &Connection, key: &str, words_now: &str, lines: &[String]) -> rusqlite::Result<()> {
    let json: Option<String> = conn.query_row("SELECT result_json FROM translation_cache WHERE key = ?1", params![key], |r| r.get(0)).optional()?;
    let Some(mut v) = json.and_then(|j| serde_json::from_str::<Value>(&j).ok()) else { return Ok(()) };
    let Some(saved) = v["translated_lines"].as_array_mut().filter(|s| s.len() == lines.len()) else { return Ok(()) };
    let original = saved.iter().map(|l| l["original_text"].as_str().unwrap_or_default()).collect::<Vec<_>>().join("\n");
    if original != words_now {
        return Ok(());
    }
    for (l, text) in saved.iter_mut().zip(lines) {
        l["translated_text"] = Value::from(text.as_str());
    }
    conn.execute("UPDATE translation_cache SET result_json = ?2 WHERE key = ?1", params![key, v.to_string()])?;
    Ok(())
}

fn delete_timing(conn: &Connection, t: &SavedTiming) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM aligned_cache WHERE video_id = ?1 AND text_hash = ?2", params![t.video_id, t.hash])?;
    Ok(())
}

/// A change the cache won't make; the message reaches the user.
fn refuse(why: &str) -> rusqlite::Error {
    rusqlite::Error::ToSqlConversionFailure(why.into())
}

/// Applies a change, all of it or none; returns the timings it deleted,
/// whose videos get aligned again (`prealign::forget`).
fn change_in(conn: &Connection, artist: &str, track: &str, change: Change) -> rusqlite::Result<Vec<String>> {
    let tx = conn.unchecked_transaction()?;
    let realign = apply(&tx, artist, track, change)?;
    tx.commit()?;
    Ok(realign)
}

fn apply(conn: &Connection, artist: &str, track: &str, change: Change) -> rusqlite::Result<Vec<String>> {
    let (artist, track) = song_key(artist, track);
    let old = row(conn, &artist, &track)?.and_then(|r| r.lines).unwrap_or_default();
    let mut realign = Vec::new();
    match change {
        Change::Save { lines, .. } if lines.is_empty() => return Err(refuse("there are no lines to save; delete the lyrics instead")),
        Change::Mark { .. } if !old.is_empty() => return Err(refuse("this song has lyrics; delete them first")),
        Change::Save { mut lines, translations } => {
            lines.sort_by(|a, b| a.time.total_cmp(&b.time));
            let index = Index::load(conn)?;
            let same_words = words(&lines) == words(&old);
            if lines != old {
                if !same_words {
                    for t in index.translations(&old) {
                        conn.execute("DELETE FROM translation_cache WHERE key = ?1", params![t.key])?;
                    }
                }
                for t in index.timings(&old) {
                    delete_timing(conn, &t)?;
                    realign.push(t.video_id);
                }
                let json = serde_json::to_string(&lines).map_err(|e| rusqlite::Error::ToSqlConversionFailure(e.into()))?;
                conn.execute(
                    "INSERT OR REPLACE INTO lyrics_cache (artist, track, lyrics_json, cached_at, source) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![artist, track, json, now_timestamp(), EDITED],
                )?;
            }
            if same_words {
                for t in translations {
                    edit_translation(conn, &t.key, &words(&lines), &t.lines)?;
                }
            }
        }
        Change::Forget => {
            conn.execute("DELETE FROM lyrics_cache WHERE artist = ?1 AND track = ?2", params![artist, track])?;
        }
        Change::Mark { instrumental } => {
            conn.execute(
                "INSERT OR REPLACE INTO lyrics_cache (artist, track, lyrics_json, cached_at, source) VALUES (?1, ?2, NULL, ?3, ?4)",
                params![artist, track, now_timestamp(), instrumental.then_some(INSTRUMENTAL)],
            )?;
        }
        Change::ForgetTranslation { key } => {
            conn.execute("DELETE FROM translation_cache WHERE key = ?1", params![key])?;
        }
        Change::ForgetTiming { video_id, hash } => {
            delete_timing(conn, &SavedTiming { video_id: video_id.clone(), hash, rejected: false })?;
            realign.push(video_id);
        }
    }
    Ok(realign)
}

fn with_db<T>(f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> Result<T, String> {
    let guard = DB_CONNECTION.lock().map_err(|e| e.to_string())?;
    let conn = guard.as_ref().ok_or("the cache isn't available")?;
    f(conn).map_err(|e| e.to_string())
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tokio::task::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

/// Every saved song, newest first.
#[tauri::command]
pub async fn saved_songs() -> Result<Vec<SavedSong>, String> {
    blocking(|| with_db(songs_in)).await
}

/// One song with its lines, translations and word timings; None once it's gone.
#[tauri::command]
pub async fn saved_song(artist: String, track: String) -> Result<Option<SavedSongDetail>, String> {
    blocking(move || with_db(|c| detail_in(c, &artist, &track))).await
}

/// Applies the user's change and returns the song as it is now.
#[tauri::command]
pub async fn saved_song_change(app: AppHandle, artist: String, track: String, change: Change) -> Result<Option<SavedSongDetail>, String> {
    match &change {
        Change::Save { lines, translations } => info!("Saved lyrics of '{}' by '{}': save {} lines, {} translations", track, artist, lines.len(), translations.len()),
        other => info!("Saved lyrics of '{}' by '{}': {:?}", track, artist, other),
    }
    let (a, t) = (artist.clone(), track.clone());
    let (realign, detail) = blocking(move || with_db(|c| Ok((change_in(c, &a, &t, change)?, detail_in(c, &a, &t)?)))).await?;
    for id in &realign {
        crate::prealign::forget(Some(id));
    }
    let (artist, track) = song_key(&artist, &track);
    let _ = app.emit(CHANGED_EVENT, serde_json::json!({ "artist": artist, "title": track }));
    Ok(detail)
}

#[cfg(test)]
mod tests {
    use super::super::{cache_aligned_in, cache_translation_in, create_aligned_table, create_lyrics_table, create_translation_table};
    use super::*;

    fn line(time: f64, text: &str) -> LyricLine {
        LyricLine { time, text: text.into(), duration: None, words: None }
    }

    fn translation(lines: &[LyricLine], translated: &[&str]) -> String {
        let tl: Vec<Value> = lines
            .iter()
            .zip(translated)
            .map(|(l, t)| serde_json::json!({ "time": l.time, "text": l.text, "original_text": l.text, "translated_text": t }))
            .collect();
        serde_json::json!({ "success": true, "translated_lines": tl }).to_string()
    }

    /// A cache with one song, its translation and its word timing.
    fn cache() -> (Connection, Vec<LyricLine>) {
        let conn = Connection::open_in_memory().unwrap();
        create_lyrics_table(&conn).unwrap();
        create_aligned_table(&conn);
        create_translation_table(&conn);
        let lines = vec![line(1.0, "夜明けの街を"), line(4.0, "君と歩いた")];
        conn.execute(
            "INSERT INTO lyrics_cache (artist, track, lyrics_json, cached_at, source) VALUES ('a', 't', ?1, 0, 'lrclib')",
            params![serde_json::to_string(&lines).unwrap()],
        )
        .unwrap();
        cache_translation_in(&conn, "k", &translation(&lines, &["The town at dawn", "I walked with you"]));
        cache_aligned_in(&conn, "vid", &text_key(&lines), Some("[]"));
        (conn, lines)
    }

    fn detail(conn: &Connection) -> SavedSongDetail {
        detail_in(conn, "A", "T").unwrap().unwrap()
    }

    fn save(lines: Vec<LyricLine>, translations: Vec<SavedTranslation>) -> Change {
        Change::Save { lines, translations }
    }

    #[test]
    fn a_song_is_shown_with_its_translation_and_timing() {
        let (conn, lines) = cache();
        let d = detail(&conn);
        assert_eq!((d.song.kind, d.song.translations, d.song.timings), ("lyrics", 1, 1));
        assert_eq!(d.lines, lines);
        assert_eq!(d.translations[0].lines, ["The town at dawn", "I walked with you"]);
        assert_eq!(songs_in(&conn).unwrap(), vec![d.song]);
    }

    #[test]
    fn editing_a_translation_line_keeps_everything_else() {
        let (conn, lines) = cache();
        let fixed = SavedTranslation { key: "k".into(), lines: vec!["Dawn's town".into(), "I walked with you".into()] };
        assert!(change_in(&conn, "a", "t", save(lines.clone(), vec![fixed])).unwrap().is_empty());
        let d = detail(&conn);
        assert_eq!(d.translations[0].lines[0], "Dawn's town");
        assert_eq!((d.song.source.as_deref(), d.song.timings), (Some("lrclib"), 1), "the lyrics weren't touched");
    }

    #[test]
    fn new_times_keep_the_translation_and_drop_the_timing() {
        let (conn, mut lines) = cache();
        lines[1].time = 4.5;
        assert_eq!(change_in(&conn, "a", "t", save(lines.clone(), vec![])).unwrap(), ["vid"]);
        let d = detail(&conn);
        assert_eq!((d.song.source.as_deref(), d.song.expires_at), (Some(EDITED), None));
        assert_eq!((d.lines, d.song.translations, d.song.timings), (lines, 1, 0));
    }

    #[test]
    fn new_words_drop_the_translation_and_the_timing() {
        let (conn, _) = cache();
        let lines = vec![line(4.0, "君と歩いた"), line(1.0, "夜明けの町を")];
        assert_eq!(change_in(&conn, "a", "t", save(lines, vec![])).unwrap(), ["vid"]);
        let d = detail(&conn);
        assert_eq!(d.lines[0].text, "夜明けの町を", "sorted by time");
        assert_eq!((d.song.translations, d.song.timings), (0, 0));
        let left: i64 = conn.query_row("SELECT COUNT(*) FROM translation_cache", [], |r| r.get(0)).unwrap();
        assert_eq!(left, 0, "deleted, not just unlinked");
    }

    #[test]
    fn a_song_without_lyrics_can_be_marked_and_given_some() {
        let (conn, lines) = cache();
        change_in(&conn, "a", "t", Change::Forget).unwrap();
        change_in(&conn, "a", "t", Change::Mark { instrumental: true }).unwrap();
        assert_eq!(detail(&conn).song.kind, "instrumental");
        change_in(&conn, "a", "t", Change::Mark { instrumental: false }).unwrap();
        assert_eq!(detail(&conn).song.kind, "missing");
        change_in(&conn, "a", "t", save(lines, vec![])).unwrap();
        let d = detail(&conn);
        assert_eq!((d.song.kind, d.song.translations), ("lyrics", 1), "the same words find their translation again");
    }

    #[test]
    fn parts_are_forgotten_one_at_a_time() {
        let (conn, lines) = cache();
        change_in(&conn, "a", "t", Change::ForgetTranslation { key: "k".into() }).unwrap();
        let hash = text_key(&lines);
        assert_eq!(change_in(&conn, "a", "t", Change::ForgetTiming { video_id: "vid".into(), hash }).unwrap(), ["vid"]);
        assert_eq!((detail(&conn).song.translations, detail(&conn).song.timings), (0, 0));
        change_in(&conn, "a", "t", Change::Forget).unwrap();
        assert!(detail_in(&conn, "a", "t").unwrap().is_none());
    }

    #[test]
    fn a_song_with_lyrics_isnt_marked_or_emptied() {
        let (conn, _) = cache();
        assert!(change_in(&conn, "a", "t", Change::Mark { instrumental: true }).is_err());
        assert!(change_in(&conn, "a", "t", save(vec![], vec![])).is_err());
        let d = detail(&conn);
        assert_eq!((d.song.kind, d.lines.len(), d.song.translations), ("lyrics", 2, 1));
    }

    #[test]
    fn a_save_that_fails_halfway_changes_nothing() {
        let (conn, _) = cache();
        // The lyrics can't be written, after the translation and timing were deleted.
        conn.execute_batch("CREATE TRIGGER no_write BEFORE INSERT ON lyrics_cache BEGIN SELECT RAISE(ABORT, 'locked'); END").unwrap();
        assert!(change_in(&conn, "a", "t", save(vec![line(1.0, "new words")], vec![])).is_err());
        let d = detail(&conn);
        assert_eq!((d.lines[0].text.as_str(), d.song.translations, d.song.timings), ("夜明けの街を", 1, 1));
    }

    #[test]
    fn a_change_arrives_as_the_frontend_sends_it() {
        let c: Change = serde_json::from_str(r#"{"op":"forgetTiming","videoId":"v","hash":"h"}"#).unwrap();
        assert!(matches!(c, Change::ForgetTiming { video_id, .. } if video_id == "v"));
        let c: Change = serde_json::from_str(r#"{"op":"save","lines":[{"time":1,"text":"x","duration":null}],"translations":[{"key":"k","lines":["y"]}]}"#).unwrap();
        assert!(matches!(c, Change::Save { lines, .. } if lines.len() == 1));
    }
}
