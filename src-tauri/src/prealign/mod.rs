//! Pre-alignment: opt-in, experimental word-level timing for YouTube Music.
//! See docs/research/prealign/REPORT.md for the study and docs/prealign.md for
//! how it runs.
//!
//! The extension reports the playing videoId (in the page URL) and the next
//! few that will play (`UPCOMING_TRACKS`). One background worker takes them in
//! order: playing track first, then up to three ahead. For each song it gets
//! the lyrics through the normal lookup, downloads that exact video's audio with
//! yt-dlp, decodes it, deletes the file, aligns the lyric text with
//! echora-kara-300m, and caches the timings by videoId + lyric-text hash.
//! `fetch_lyrics_timed` serves a cached alignment (after the reject rule) as
//! source `"aligned"`; one that finishes while its track plays is pushed as
//! `lyrics-timing-updated`. Any failure leaves today's timing in place.

pub mod align;
mod assets;
mod audio;
#[cfg(test)]
mod bench;
mod decode;
mod head;
mod model;
mod queue;
mod reject;
pub(crate) mod romanize;
mod ytdlp;

use crate::lyrics::{TimingSource, TimingUpdate, TIMING_UPDATED_EVENT};
use crate::lyrics_cache::{cache_aligned, get_aligned, AlignedCache};
use crate::types::LyricLine;
use crate::ytmusic;
use align::{AlignError, AlignedLine};
use assets::Paths;
use log::{debug, info, warn};
use queue::{Job, Lyrics, Queue, Song};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex, MutexGuard, OnceLock};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Notify;

pub const STATUS_EVENT: &str = "prealign-status";
/// Fewer lines than this is not worth a download.
const MIN_LINES: usize = 3;
/// The downloaded audio may differ from the player's length by this much (s).
const MAX_LENGTH_GAP: f64 = 3.0;

/// What the serving hook has for the lyrics being shown.
#[derive(Clone, Debug, PartialEq)]
pub enum Served {
    /// Nothing to add: feature off, not a YouTube Music track, or rejected.
    Off,
    /// The lines re-timed, with word timing where it passed the reject rule.
    Hit(Vec<LyricLine>),
    /// Queued (or running); a `lyrics-timing-updated` will follow if it works out.
    Pending,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    file: &'static str,
    done: u64,
    total: Option<u64>,
}

/// `prealign_status` / `prealign-status` payload.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    enabled: bool,
    /// "off" | "downloading" | "ready" | "error"
    state: &'static str,
    download: Option<DownloadProgress>,
    error: Option<String>,
    /// Model + yt-dlp on disk.
    disk_bytes: u64,
    /// "Title — Artist" being aligned right now.
    working_on: Option<String>,
    /// Songs aligned this session.
    aligned: u32,
}

struct Waiter {
    title: String,
    artist: String,
    hash: String,
    lyrics: Lyrics,
}

#[derive(Default)]
struct Shared {
    queue: Queue,
    /// Requests showing estimate/line timing that an alignment would upgrade.
    waiters: HashMap<String, Waiter>,
    downloading: Option<DownloadProgress>,
    download_running: bool,
    /// The download, so removing the files can stop it first.
    download_task: Option<tauri::async_runtime::JoinHandle<()>>,
    error: Option<String>,
    working_on: Option<String>,
    aligned: u32,
}

struct Prealign {
    enabled: AtomicBool,
    extra_sources: AtomicBool,
    shared: Mutex<Shared>,
    wake: Notify,
    /// Set to stop the running job at its next checkpoint.
    cancel: AtomicBool,
    app: OnceLock<AppHandle>,
    paths: OnceLock<Paths>,
}

/// The app, once `init` ran (always, at startup): for events from code that
/// has no handle of its own.
pub(crate) fn app() -> Option<&'static AppHandle> {
    P.app.get()
}

static P: LazyLock<Prealign> = LazyLock::new(|| Prealign {
    enabled: AtomicBool::new(false),
    extra_sources: AtomicBool::new(false),
    shared: Mutex::new(Shared::default()),
    wake: Notify::new(),
    cancel: AtomicBool::new(false),
    app: OnceLock::new(),
    paths: OnceLock::new(),
});

fn shared() -> MutexGuard<'static, Shared> {
    P.shared.lock().unwrap_or_else(|e| e.into_inner())
}

fn enabled() -> bool {
    P.enabled.load(Ordering::Relaxed)
}

/// Called once from `setup`: where files go, and the worker.
pub fn init(app: &AppHandle) {
    let Ok(base) = app.path().app_local_data_dir() else {
        warn!("Pre-align: no app data dir; feature unavailable");
        return;
    };
    let paths = Paths { dir: base.join("prealign") };
    // Audio is never kept: clear anything a crash left behind.
    let _ = std::fs::remove_dir_all(paths.tmp());
    paths.remove_stale();
    let _ = P.paths.set(paths);
    let _ = P.app.set(app.clone());
    tauri::async_runtime::spawn(worker());
}

// ---------------------------------------------------------------------------
// Inputs: the extension and the settings.

/// Every TRACK_* message: a new video playing makes the old one's job moot.
pub fn on_track(url: &str) {
    let Some(id) = ytmusic::video_id_from_url(url) else {
        return;
    };
    let mut s = shared();
    s.queue.playing(&id);
    s.waiters.retain(|v, _| *v == id);
}

/// `UPCOMING_TRACKS` from the extension's queue reader.
pub fn on_upcoming(data: &serde_json::Value) {
    if !enabled() {
        return;
    }
    let songs = upcoming(data);
    debug!("Pre-align: up next {:?}", songs.iter().map(|s| &s.title).collect::<Vec<_>>());
    shared().queue.set_upcoming(songs);
    P.wake.notify_one();
}

/// The songs in an `UPCOMING_TRACKS` message, next first.
/// The queue sends at most this many songs; more is not the extension talking.
const UP_NEXT_MAX: usize = 3;

pub fn upcoming(data: &serde_json::Value) -> Vec<Song> {
    let entries = data["upNext"].as_array().map(Vec::as_slice).unwrap_or_default();
    entries.iter().take(UP_NEXT_MAX).filter_map(song_from_json).collect()
}

/// Longer than any title or byline YouTube shows: not the queue talking.
const NAME_MAX: usize = 300;

fn song_from_json(v: &serde_json::Value) -> Option<Song> {
    let video_id = v["videoId"].as_str().filter(|id| ytmusic::is_video_id(id))?.to_string();
    let name = |key: &str| v[key].as_str().filter(|s| s.len() <= NAME_MAX);
    Some(Song {
        video_id,
        title: crate::cleaning::canonical(name("title")?),
        artist: crate::cleaning::canonical(name("artist").unwrap_or("")),
        duration: v["length"].as_str().and_then(parse_length),
    })
}

/// "3:45" / "1:02:03" → seconds.
fn parse_length(s: &str) -> Option<f64> {
    s.trim().split(':').try_fold(0.0, |acc, part| part.trim().parse::<u32>().ok().map(|n| acc * 60.0 + f64::from(n)))
}

/// Which aligner made a cached alignment. Part of the cache key, so another
/// model's alignments (MMS, before this one) are never served as its own.
const ALIGNER: &str = "echora-kara-300m";

/// The lyric text an alignment belongs to (and the aligner that made it).
pub fn text_key(lines: &[LyricLine]) -> String {
    let mut h = Sha256::new();
    h.update(ALIGNER.as_bytes());
    for l in lines {
        let text = l.text.trim();
        h.update((text.len() as u64).to_le_bytes());
        h.update(text.as_bytes());
    }
    assets::hex(&h.finalize()[..16])
}

// ---------------------------------------------------------------------------
// Serving.

/// "Search again" replaced the track's lyrics: align the new ones when served.
/// (The aligned cache is keyed by the text, so the old alignment never matches.)
pub fn forget(video_id: Option<&str>) {
    if let Some(id) = video_id {
        shared().queue.forget(id);
    }
}

/// The serving hook for `fetch_lyrics_timed`: cached word timing for this
/// YouTube Music video, or queue it. `video_id`: the player's (None elsewhere).
pub fn serve(video_id: Option<&str>, title: &str, artist: &str, duration: Option<f64>, lines: &[LyricLine], real_timing: bool) -> Served {
    let Some(id) = video_id.filter(|id| ytmusic::is_video_id(id)).map(str::to_string) else {
        return Served::Off;
    };
    if !enabled() || lines.len() < MIN_LINES {
        return Served::Off;
    }
    let mut s = shared();
    let hash = text_key(lines);
    let lyrics = Lyrics { lines: lines.to_vec(), real_timing };
    match get_aligned(&id, &hash) {
        AlignedCache::Hit(json) => apply_cached(&json, &lyrics).map_or(Served::Off, Served::Hit),
        AlignedCache::Rejected => Served::Off,
        AlignedCache::Miss if s.error.is_some() || s.queue.is_done(&id) => Served::Off,
        AlignedCache::Miss => {
            let song = Song { video_id: id.clone(), title: title.into(), artist: artist.into(), duration };
            s.queue.set_current(Job { song, lyrics: Some(lyrics.clone()) });
            s.waiters.insert(id, Waiter { title: title.into(), artist: artist.into(), hash, lyrics });
            if s.queue.should_preempt() {
                debug!("Pre-align: the playing track preempts the lookahead job");
                P.cancel.store(true, Ordering::Relaxed);
            }
            drop(s);
            P.wake.notify_one();
            Served::Pending
        }
    }
}

fn apply_cached(json: &str, lyrics: &Lyrics) -> Option<Vec<LyricLine>> {
    let aligned: Vec<AlignedLine> = serde_json::from_str(json).ok()?;
    reject::apply(&lyrics.lines, lyrics.real_timing, &aligned)
}

/// The playing track's alignment just landed: push it to the overlay.
fn notify_waiter(video_id: &str) {
    let Some(hash) = shared().waiters.get(video_id).map(|w| w.hash.clone()) else {
        return;
    };
    // The waiter may be for newer lyrics than the job that just ended
    // ("Search again"): it waits for theirs.
    let json = match get_aligned(video_id, &hash) {
        AlignedCache::Miss => return,
        AlignedCache::Rejected => None,
        AlignedCache::Hit(json) => Some(json),
    };
    let Some(w) = shared().waiters.remove(video_id) else {
        return;
    };
    let Some(json) = json else {
        return;
    };
    let Some(lines) = apply_cached(&json, &w.lyrics) else {
        return;
    };
    info!("Pre-align: word timing ready for '{}' by '{}'", w.title, w.artist);
    let update = TimingUpdate { title: w.title, artist: w.artist, lines, source: TimingSource::Aligned };
    if let Some(app) = P.app.get() {
        if let Err(e) = app.emit(TIMING_UPDATED_EVENT, update) {
            warn!("Failed to emit {}: {}", TIMING_UPDATED_EVENT, e);
        }
    }
}

// ---------------------------------------------------------------------------
// The worker.

#[derive(Debug)]
enum Failure {
    Cancelled,
    Skip(String),
}

impl From<String> for Failure {
    fn from(e: String) -> Self {
        Failure::Skip(e)
    }
}

impl From<AlignError> for Failure {
    fn from(e: AlignError) -> Self {
        match e {
            AlignError::Cancelled => Failure::Cancelled,
            e => Failure::Skip(e.to_string()),
        }
    }
}

impl From<&str> for Failure {
    fn from(e: &str) -> Self {
        Failure::Skip(e.to_string())
    }
}

async fn worker() {
    let mut model: Option<model::Model> = None;
    let mut verified = false;
    loop {
        let job = {
            let mut s = shared();
            let ready = P.paths.get().is_some_and(Paths::ready);
            if enabled() && ready && s.downloading.is_none() {
                s.queue.next()
            } else {
                None
            }
        };
        let Some(job) = job else {
            model = None; // idle: give the memory back
            P.wake.notified().await;
            continue;
        };
        P.cancel.store(false, Ordering::Relaxed);
        let id = job.song.video_id.clone();
        shared().working_on = Some(format!("{} — {}", job.song.title, job.song.artist));
        emit_status();
        let started = std::time::Instant::now();
        let outcome = run(&job, &mut model, &mut verified).await;
        let finished = !matches!(outcome, Err(Failure::Cancelled));
        match &outcome {
            Ok(note) => info!("Pre-align: '{}' {} in {:.0}s", job.song.title, note, started.elapsed().as_secs_f64()),
            Err(Failure::Cancelled) => debug!("Pre-align: '{}' preempted", job.song.title),
            Err(Failure::Skip(why)) => debug!("Pre-align: skipped '{}': {}", job.song.title, why),
        }
        {
            let mut s = shared();
            s.queue.end(&id, finished);
            s.working_on = None;
            if matches!(outcome, Ok("aligned")) {
                s.aligned += 1;
            }
        }
        emit_status();
    }
}

fn check_cancel() -> Result<(), Failure> {
    if P.cancel.load(Ordering::Relaxed) || !enabled() {
        Err(Failure::Cancelled)
    } else {
        Ok(())
    }
}

async fn run(job: &Job, model: &mut Option<model::Model>, verified: &mut bool) -> Result<&'static str, Failure> {
    let paths = P.paths.get().ok_or("no data dir")?.clone();
    let song = &job.song;
    let lyrics = match &job.lyrics {
        Some(l) => l.clone(),
        None => {
            let extra = P.extra_sources.load(Ordering::Relaxed);
            let (lines, real_timing) = crate::lyrics::lookup_ahead(&song.video_id, &song.title, &song.artist, song.duration, extra)
                .await
                .ok_or("no lyrics")?;
            Lyrics { lines, real_timing }
        }
    };
    if lyrics.lines.len() < MIN_LINES {
        return Err(Failure::Skip("too few lines".into()));
    }
    let hash = text_key(&lyrics.lines);
    if get_aligned(&song.video_id, &hash) != AlignedCache::Miss {
        notify_waiter(&song.video_id);
        return Ok("already cached");
    }
    check_cancel()?;

    if song.duration.is_some_and(|d| d > align::MAX_SECONDS as f64) {
        return Err(AlignError::TooLong.into()); // before downloading it
    }
    assets::update_ytdlp_if_due(&paths).await;
    let fetched = ytdlp::fetch(&paths.ytdlp(), &paths.tmp(), &song.video_id).await?;
    let (path, listed) = (fetched.path.clone(), fetched.duration);
    let wav = tokio::task::spawn_blocking(move || audio::decode_16k(&path)).await.map_err(|e| e.to_string())??;
    drop(fetched); // the file is deleted here; only the samples remain, in memory
    let secs = wav.len() as f64 / align::SAMPLE_RATE as f64;
    if listed.is_some_and(|d| (secs - d).abs() > MAX_LENGTH_GAP) {
        return Err(Failure::Skip(format!("decoded {secs:.0}s of a {:.0}s video", listed.unwrap_or(0.0))));
    }
    if let Some(want) = song.duration {
        if (secs - want).abs() > MAX_LENGTH_GAP {
            // Not the recording the player has (or the id was stale): leave it.
            return Err(Failure::Skip(format!("audio is {secs:.0}s, the player says {want:.0}s")));
        }
    }
    align::check_size(wav.len(), 0)?; // the player may not have known the length
    check_cancel()?;

    if !*verified {
        let p = paths.clone();
        *verified = tokio::task::spawn_blocking(move || assets::verify_models(&p)).await.unwrap_or(false);
        if !*verified {
            shared().error = Some("The alignment model failed its checksum. Turn the feature off and on to download it again.".into());
            return Err(Failure::Skip("model checksum".into()));
        }
    }
    let m = match model.take() {
        Some(m) => m,
        None => {
            let p = paths.clone();
            tokio::task::spawn_blocking(move || model::Model::load(&p, model::threads())).await.map_err(|e| e.to_string())??
        }
    };
    let texts: Vec<String> = lyrics.lines.iter().map(|l| l.text.clone()).collect();
    let (m, result) = tokio::task::spawn_blocking(move || {
        let mut m = m;
        let r = m.align(&wav, &texts, &P.cancel);
        (m, r)
    })
    .await
    .map_err(|e| e.to_string())?;
    *model = Some(m);
    let aligned = result?;
    if reject::track_rejected(&aligned) {
        cache_aligned(&song.video_id, &hash, None);
        return Ok("rejected (low confidence)");
    }
    let json = serde_json::to_string(&aligned).map_err(|e| e.to_string())?;
    cache_aligned(&song.video_id, &hash, Some(&json));
    notify_waiter(&song.video_id);
    Ok("aligned")
}

// ---------------------------------------------------------------------------
// Settings and status.

fn status() -> Status {
    let paths = P.paths.get();
    let s = shared();
    let on = enabled();
    let ready = paths.is_some_and(Paths::ready);
    let state = match () {
        _ if !on => "off",
        _ if s.error.is_some() => "error",
        _ if s.downloading.is_some() || !ready => "downloading",
        _ => "ready",
    };
    Status {
        enabled: on,
        state,
        download: s.downloading.clone(),
        error: s.error.clone(),
        disk_bytes: paths.map_or(0, Paths::disk_bytes),
        working_on: s.working_on.clone(),
        aligned: s.aligned,
    }
}

fn emit_status() {
    if let Some(app) = P.app.get() {
        let _ = app.emit(STATUS_EVENT, status());
    }
}

/// Fetch the model and yt-dlp if missing, reporting progress.
fn start_download() {
    let Some(paths) = P.paths.get().cloned() else {
        return;
    };
    {
        let mut s = shared();
        if s.download_running || paths.ready() {
            return;
        }
        s.download_running = true;
        s.error = None;
        s.downloading = Some(DownloadProgress { file: "model", done: 0, total: None });
    }
    emit_status();
    let task = tauri::async_runtime::spawn(async move {
        let last = Mutex::new(std::time::Instant::now());
        let result = assets::ensure(&paths, |p| {
            shared().downloading = Some(DownloadProgress { file: p.file, done: p.done, total: p.total });
            let mut last = last.lock().unwrap_or_else(|e| e.into_inner());
            if last.elapsed().as_millis() > 250 || Some(p.done) == p.total {
                *last = std::time::Instant::now();
                emit_status();
            }
        })
        .await;
        {
            let mut s = shared();
            s.download_running = false;
            s.downloading = None;
            if let Err(e) = &result {
                warn!("Pre-align download failed: {}", e);
                s.error = Some(format!("Download failed: {e}"));
            }
        }
        emit_status();
        P.wake.notify_one();
    });
    shared().download_task = Some(task);
}

/// The overlay mirrors the user's settings here on start and on every change.
#[tauri::command]
pub fn prealign_configure(enabled: bool, extra_sources: bool) -> Status {
    P.extra_sources.store(extra_sources, Ordering::Relaxed);
    let was = P.enabled.swap(enabled, Ordering::Relaxed);
    if enabled && !was {
        info!("Pre-align enabled");
        if cfg!(windows) {
            shared().error = None;
            start_download();
            P.wake.notify_one();
        } else {
            // The runtime and yt-dlp it downloads are Windows builds.
            shared().error = Some("Word-level timing runs on Windows only for now.".into());
        }
    } else if !enabled && was {
        info!("Pre-align disabled");
        P.cancel.store(true, Ordering::Relaxed);
        let mut s = shared();
        s.queue.clear();
        s.waiters.clear();
    }
    let st = status();
    emit_status();
    st
}

#[tauri::command]
pub fn prealign_status() -> Status {
    status()
}

/// Delete the model, yt-dlp and any audio, and switch the feature off.
/// Cached timings stay (they are tiny and contain no audio).
#[tauri::command]
pub async fn prealign_remove_files() -> Result<Status, String> {
    prealign_configure(false, P.extra_sources.load(Ordering::Relaxed));
    let paths = P.paths.get().cloned().ok_or("no data dir")?;
    // A download still running writes into the folder about to go.
    let download = shared().download_task.take();
    if let Some(task) = download {
        task.abort();
        let _ = task.await;
        let mut s = shared();
        s.download_running = false;
        s.downloading = None;
    }
    // The worker lets go of the model as soon as it sees the cancel.
    let mut result = Ok(());
    for _ in 0..20 {
        result = assets::remove_all(&paths);
        if result.is_ok() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    shared().error = None;
    emit_status();
    result.map(|_| status())
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_queue_longer_than_the_extension_sends_is_cut_to_three() {
        let entry = |n: usize| serde_json::json!({ "videoId": format!("abcdefghij{n}"), "title": "Song", "artist": "Band" });
        let data = serde_json::json!({ "upNext": (0..9).map(entry).collect::<Vec<_>>() });
        assert_eq!(super::upcoming(&data).len(), 3);
    }

    #[test]
    fn an_overlong_queue_title_is_not_a_song() {
        let long = "x".repeat(301);
        let data = serde_json::json!({ "upNext": [
            { "videoId": "abcdefghij0", "title": long, "artist": "Band" },
            { "videoId": "abcdefghij1", "title": "Song", "artist": long },
        ] });
        let songs = super::upcoming(&data);
        assert_eq!(songs.len(), 1);
        assert_eq!((songs[0].title.as_str(), songs[0].artist.as_str()), ("Song", ""));
    }

    use super::*;

    #[test]
    fn a_cancelled_alignment_stays_cancelled_and_the_rest_are_skips() {
        // A skip marks the song done for the session; a cancel lets it run again.
        assert!(matches!(Failure::from(AlignError::Cancelled), Failure::Cancelled));
        assert!(matches!(Failure::from(AlignError::TooLong), Failure::Skip(why) if why == "track too long to align"));
    }

    fn line(text: &str) -> LyricLine {
        LyricLine { time: 0.0, text: text.into(), duration: None, words: None }
    }

    #[test]
    fn the_cache_key_is_the_lyric_text() {
        let a = [line("one"), line("two"), line("three")];
        let same = [line(" one"), line("two "), line("three")];
        let other = [line("one"), line("two"), line("four")];
        let joined = [line("one\ntwo"), line("three")];
        assert_eq!(text_key(&a), text_key(&same), "surrounding spaces don't count");
        assert_ne!(text_key(&a), text_key(&other));
        assert_ne!(text_key(&a), text_key(&joined), "line breaks do");
        assert_eq!(text_key(&a).len(), 32);
    }

    #[test]
    fn queue_entries_parse_lengths() {
        let v = serde_json::json!({"videoId": "xTvyyoF_LZY", "title": "Shape of You", "artist": "Ed Sheeran", "length": "3:54"});
        assert_eq!(song_from_json(&v).unwrap().duration, Some(234.0));
        assert_eq!(parse_length("1:02:03"), Some(3723.0));
        assert_eq!(song_from_json(&serde_json::json!({"videoId": "nope", "title": "x"})), None);
    }
}
