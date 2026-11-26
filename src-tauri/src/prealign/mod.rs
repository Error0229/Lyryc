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
//!
//! The saved-lyrics editor uses the same worker (`prealign_align`): for lyrics
//! the user pasted without times, which are saved with the times it hears, and
//! for a saved song's own lines, which get word timing. Those jobs go first,
//! in the order asked, and run on their own: Settings hears how each ended
//! (`prealign-timed`).

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
use crate::lyrics_cache::SavedRow;
use crate::lyrics_cache::{cache_aligned, get_aligned, video_of, AlignedCache};
use crate::types::LyricLine;
use crate::ytmusic;
use align::{AlignError, AlignedLine};
use assets::Paths;
use log::{debug, info, warn};
use queue::{Job, Lyrics, Queue, Song};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex, MutexGuard, OnceLock};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Notify;

pub const STATUS_EVENT: &str = "prealign-status";
/// An editor's job ended (`Timed`).
pub const TIMED_EVENT: &str = "prealign-timed";
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
    /// The saved songs being timed for the editor: the running one first, then the waiting ones.
    timing: Vec<SongRef>,
    /// Songs on screen whose word timing is on its way (queued or running),
    /// named as the player named them. One that leaves without word timing
    /// arriving could not be synced.
    waiting: Vec<SongRef>,
}

/// A saved song, named as the cache keys it (`lyrics_cache::song_key`).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SongRef {
    artist: String,
    track: String,
}

/// `prealign-timed` payload: an editor's job ended; `error` says why it didn't work.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Timed {
    artist: String,
    track: String,
    pasted: bool,
    error: Option<String>,
}

struct Waiter {
    title: String,
    artist: String,
    hash: String,
    lyrics: Lyrics,
}

/// Who asked for a job in the editor (`prealign_align`), and what to do with
/// its answer.
#[derive(Clone, Debug, PartialEq)]
struct Asked {
    song: SongRef,
    purpose: Purpose,
}

#[derive(Clone, Debug, PartialEq)]
enum Purpose {
    /// Lyrics pasted without times: saved with the times the aligner hears,
    /// over the song's row as it was when asked (`was`) and nothing newer.
    Pasted { was: Option<SavedRow> },
    /// The song's saved lines: they keep their times and get word timing.
    Saved,
}

struct Ask {
    asked: Asked,
    job: Job,
}

#[derive(Default)]
struct Shared {
    queue: Queue,
    /// The editor's jobs, in the order asked: the worker takes them before the queue.
    asked: VecDeque<Ask>,
    /// The editor's job running now.
    running: Option<SongRef>,
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

impl Shared {
    /// The worker's next job, and who asked for it if the editor did.
    fn next(&mut self) -> Option<(Job, Option<Asked>)> {
        match self.asked.pop_front() {
            Some(ask) => Some((ask.job, Some(ask.asked))),
            None => self.queue.next().map(|job| (job, None)),
        }
    }

    /// An editor's job joins the line. Asked again while it waits, the newer
    /// job takes the older one's place in the line; asked again while it
    /// runs, the running one stops and the newer one waits its turn. Either
    /// way the song stays in `timing`, and only the newer job's
    /// `prealign-timed` is heard: the editor hears about the song, not the
    /// ask. It also stops a queue job that is running (that one runs again
    /// later), never another song's editor job.
    fn ask(&mut self, ask: Ask) {
        let song = ask.asked.song.clone();
        match self.asked.iter_mut().find(|a| a.asked.song == song) {
            Some(waiting) => *waiting = ask,
            None => self.asked.push_back(ask),
        }
        // A job past its last checkpoint finishes anyway; the worker clears the
        // flag, under this lock, when it picks the next one.
        if self.working_on.is_some() && self.running.as_ref().is_none_or(|r| *r == song) {
            P.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// An editor's job that ended with a newer one for its song waiting:
    /// that one answers for the song, so this one keeps nothing. Even one that
    /// finished past its last checkpoint: its save would make the newer job's
    /// "nothing changed since" check fail, and lose the user's newer lines.
    fn superseded(&self, asked: &Asked) -> bool {
        self.asked.iter().any(|a| a.asked.song == asked.song)
    }

    /// The songs being timed for the editor, the running one first.
    fn timing(&self) -> Vec<SongRef> {
        self.running.iter().cloned().chain(self.asked.iter().map(|a| a.asked.song.clone())).collect()
    }
}

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
    let before = s.waiters.len();
    s.waiters.retain(|v, _| *v == id);
    let changed = s.waiters.len() != before;
    drop(s);
    if changed {
        emit_status();
    }
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
            emit_status();
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
        let work = {
            let mut s = shared();
            let work = if runnable(&s) { s.next() } else { None };
            if let Some((job, asked)) = &work {
                // Under the lock, with the pick: a cancel after it is meant for this job.
                P.cancel.store(false, Ordering::Relaxed);
                s.working_on = Some(format!("{} — {}", job.song.title, job.song.artist));
                s.running = asked.as_ref().map(|a| a.song.clone());
            }
            work
        };
        let Some((job, asked)) = work else {
            model = None; // idle: give the memory back
            P.wake.notified().await;
            continue;
        };
        let id = job.song.video_id.clone();
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
            // The editor's job was never the queue's: the song's turn there still comes.
            if asked.is_none() {
                s.queue.end(&id, finished);
            }
            s.working_on = None;
            s.running = None;
            if matches!(outcome, Ok("aligned")) {
                s.aligned += 1;
            }
            // Skipped (a failed download, too long, ...): no word timing is coming.
            // A preempted job runs again; a finished one answered its waiter already.
            if matches!(outcome, Err(Failure::Skip(_))) {
                s.waiters.remove(&id);
            }
        }
        if let (Some(asked), Some(lyrics)) = (asked, &job.lyrics) {
            if !shared().superseded(&asked) {
                let cached = get_aligned(&id, &text_key(&lyrics.lines));
                finish(asked, answer(lyrics, &outcome, cached));
            }
        }
        emit_status();
    }
}

/// The editor's job ended without an answer: switched off while it ran.
const STOPPED: &str = "it was stopped before it finished";
/// The editor's job never ran: switched off while it waited.
const TURNED_OFF: &str = "word-level timing was turned off";

type SaveTimed = fn(&str, &str, &Option<SavedRow>, Vec<LyricLine>) -> Result<(), String>;

/// What an ended job leaves: pasted lyrics are saved with their times
/// (`save`); a saved song's word timing is in the cache already. A job that
/// failed or was stopped saves nothing and says why.
fn keep(asked: &Asked, timed: Result<Vec<LyricLine>, String>, save: SaveTimed) -> Result<(), String> {
    let lines = timed?;
    match &asked.purpose {
        Purpose::Pasted { was } => save(&asked.song.artist, &asked.song.track, was, lines),
        Purpose::Saved => Ok(()),
    }
}

/// An editor's job ended: keep what it was for, let the overlay show it, and
/// tell Settings how it went.
fn finish(asked: Asked, timed: Result<Vec<LyricLine>, String>) {
    let saved = keep(&asked, timed, crate::lyrics_cache::save_timed);
    let Asked { song, purpose } = asked;
    let pasted = matches!(purpose, Purpose::Pasted { .. });
    match &saved {
        Ok(()) => info!("Pre-align: timed '{}' by '{}' for the editor", song.track, song.artist),
        Err(why) => info!("Pre-align: couldn't time '{}' by '{}' for the editor: {}", song.track, song.artist, why),
    }
    let Some(app) = P.app.get() else { return };
    if saved.is_ok() {
        crate::lyrics_cache::announce(app, &song.artist, &song.track);
    }
    let timed = Timed { artist: song.artist, track: song.track, pasted, error: saved.err() };
    if let Err(e) = app.emit(TIMED_EVENT, timed) {
        warn!("Failed to emit {}: {}", TIMED_EVENT, e);
    }
}

/// What the editor gets for the lines it asked about: the lines timed, or why not.
fn answer(lyrics: &Lyrics, outcome: &Result<&'static str, Failure>, cached: AlignedCache) -> Result<Vec<LyricLine>, String> {
    const NOT_HEARD: &str = "these words weren't heard in the song. Check that they are its lyrics";
    match (outcome, cached) {
        (Err(Failure::Cancelled), _) => Err(STOPPED.into()),
        (Err(Failure::Skip(why)), _) => Err(why.clone()),
        (Ok(_), AlignedCache::Hit(json)) => apply_cached(&json, lyrics).ok_or_else(|| NOT_HEARD.into()),
        (Ok(_), AlignedCache::Rejected) => Err(NOT_HEARD.into()),
        (Ok(_), AlignedCache::Miss) => Err("the timing couldn't be saved".into()),
    }
}

/// The video's audio. One that plays only for the account (the user's own
/// upload) is fetched again with the browser's sign-in.
async fn fetch(paths: &assets::Paths, video_id: &str) -> Result<ytdlp::Audio, String> {
    let (exe, tmp) = (paths.ytdlp(), paths.tmp());
    match ytdlp::fetch(&exe, &tmp, video_id, None).await {
        Err(e) if ytdlp::is_private(&e) => {
            let jar = crate::cookies::youtube(&tmp, video_id).await.map_err(|why| format!("it's a private video, and {why}"))?;
            ytdlp::fetch(&exe, &tmp, video_id, Some(&jar.path)).await.map_err(|e| match ytdlp::is_private(&e) {
                true => "it's a private video that the account signed in to YouTube in the browser can't play".into(),
                false => e,
            })
        }
        other => other,
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
    let fetched = fetch(&paths, &song.video_id).await?;
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
        notify_waiter(&song.video_id); // nothing to send: it stops waiting
        return Ok("rejected (low confidence)");
    }
    let json = serde_json::to_string(&aligned).map_err(|e| e.to_string())?;
    cache_aligned(&song.video_id, &hash, Some(&json));
    notify_waiter(&song.video_id);
    Ok("aligned")
}

// ---------------------------------------------------------------------------
// Settings and status.

/// Can the worker take a job: switched on, the files there, nothing downloading.
fn runnable(s: &Shared) -> bool {
    enabled() && s.downloading.is_none() && P.paths.get().is_some_and(Paths::ready)
}

/// "off" | "downloading" | "ready" | "error", as the settings page shows it.
fn state(s: &Shared) -> &'static str {
    match () {
        _ if !enabled() => "off",
        _ if s.error.is_some() => "error",
        _ if !runnable(s) => "downloading",
        _ => "ready",
    }
}

fn status() -> Status {
    let paths = P.paths.get();
    let s = shared();
    Status {
        enabled: enabled(),
        state: state(&s),
        download: s.downloading.clone(),
        error: s.error.clone(),
        disk_bytes: paths.map_or(0, Paths::disk_bytes),
        working_on: s.working_on.clone(),
        aligned: s.aligned,
        timing: s.timing(),
        waiting: s.waiters.values().map(|w| SongRef { artist: w.artist.clone(), track: w.title.clone() }).collect(),
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
        let dropped = {
            let mut s = shared();
            s.queue.clear();
            s.waiters.clear();
            std::mem::take(&mut s.asked)
        };
        for ask in dropped {
            finish(ask.asked, Err(TURNED_OFF.into()));
        }
    }
    let st = status();
    emit_status();
    st
}

#[tauri::command]
pub fn prealign_status() -> Status {
    status()
}

/// Settings → Saved lyrics: time a saved song by listening to it, in the
/// background. `artist` and `track` name the song, whose audio is the YouTube
/// Music video it was last seen as (`lyrics_cache::video_of`). `lines`: lyrics
/// pasted without times, one sung line each, saved with the times heard once
/// they're timed; None: the song's saved lines, which keep their times and get
/// word timing. Resolves as soon as the job is queued, or with what to fix
/// first; how it ends comes as `prealign-timed` (a minute or so a song). The
/// editor's jobs go ahead of the queue, in the order asked.
#[tauri::command]
pub async fn prealign_align(artist: String, track: String, lines: Option<Vec<String>>) -> Result<(), String> {
    // What the user can fix here first, before anything about the song (and
    // checked again below, under the lock that queues it).
    ready(&shared())?;
    // The cache is read before taking the lock: the worker and `status` wait on it.
    let (artist, track) = crate::lyrics_cache::song_key(&artist, &track);
    let row = crate::lyrics_cache::saved_row(&artist, &track)?;
    let (lyrics, purpose) = match lines {
        Some(pasted) => (Lyrics { lines: untimed(&pasted), real_timing: false }, Purpose::Pasted { was: row }),
        None => {
            let row = row.ok_or("this song has no saved lyrics to time")?;
            let real_timing = crate::lyrics::times_this_recording(row.source.as_deref());
            (Lyrics { lines: row.lines.ok_or("this song has no saved lyrics to time")?, real_timing }, Purpose::Saved)
        }
    };
    if lyrics.lines.len() < MIN_LINES {
        return Err(format!("it takes at least {MIN_LINES} lines"));
    }
    let video_id = video_of(&artist, &track).ok_or("this song hasn't played on YouTube Music yet. Play it there once, then try again")?;
    // The aligner hears the same words in the same audio the same way every time.
    if purpose == Purpose::Saved && get_aligned(&video_id, &text_key(&lyrics.lines)) != AlignedCache::Miss {
        return Err("these lyrics were already aligned with this song's video. Delete that word timing to align them again".into());
    }
    info!("Pre-align: timing {} {:?} lines of '{}' by '{}' ({})", lyrics.lines.len(), purpose, track, artist, video_id);
    let song = SongRef { artist: artist.clone(), track: track.clone() };
    let job = Job { song: Song { video_id, title: track, artist, duration: None }, lyrics: Some(lyrics) };
    {
        let mut s = shared();
        ready(&s)?; // switched off meanwhile
        s.ask(Ask { asked: Asked { song, purpose }, job });
    }
    P.wake.notify_one();
    emit_status();
    Ok(())
}

/// Can the editor's job run: on, downloaded, no error. Else what to do about it.
fn ready(s: &Shared) -> Result<(), String> {
    match state(s) {
        "ready" => Ok(()),
        "off" => Err("word-level timing is off. Turn it on under Lyrics & sync".into()),
        "downloading" => Err("word-level timing is still downloading its files. Try again when it's ready".into()),
        _ => Err(s.error.clone().unwrap_or_else(|| "word-level timing is unavailable".into())),
    }
}

/// Pasted lines as lyrics that have no times yet. A blank line is no line.
fn untimed(lines: &[String]) -> Vec<LyricLine> {
    let line = |text: &str| LyricLine { time: 0.0, text: text.into(), duration: None, words: None };
    lines.iter().map(|l| l.trim()).filter(|l| !l.is_empty()).map(line).collect()
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

    fn job(id: &str) -> Job {
        let song = Song { video_id: id.into(), title: id.into(), artist: "a".into(), duration: None };
        Job { song, lyrics: Some(Lyrics { lines: vec![line("one"), line("two"), line("three")], real_timing: false }) }
    }

    fn ask(id: &str, track: &str) -> Ask {
        let song = SongRef { artist: "a".into(), track: track.into() };
        Ask { asked: Asked { song, purpose: Purpose::Pasted { was: None } }, job: job(id) }
    }

    #[test]
    fn an_ended_job_saves_pasted_lyrics_and_only_what_was_timed() {
        fn saved(_: &str, _: &str, _: &Option<SavedRow>, _: Vec<LyricLine>) -> Result<(), String> {
            Err("saved".into()) // marks that it was called
        }
        let pasted = ask("a", "t").asked;
        let own = Asked { purpose: Purpose::Saved, ..pasted.clone() };
        let lines = || Ok(vec![line("one")]);
        assert_eq!(keep(&pasted, lines(), saved), Err("saved".into()), "pasted lines are saved");
        assert_eq!(keep(&own, lines(), saved), Ok(()), "a saved song's timing is in the cache already");
        // Switched off while it ran: stopped, and nothing written.
        let stopped = answer(&job("a").lyrics.unwrap(), &Err(Failure::Cancelled), AlignedCache::Miss);
        assert_eq!(keep(&pasted, stopped, saved), Err(STOPPED.into()));
        assert_eq!(keep(&pasted, Err("not heard".into()), saved), Err("not heard".into()));
    }

    #[test]
    fn the_editors_lyrics_go_first_and_the_playing_track_waits_for_them() {
        let mut s = Shared::default();
        s.queue.set_current(job("playing"));
        s.ask(ask("asked", "t"));
        let (first, asked) = s.next().unwrap();
        assert_eq!((first.song.video_id.as_str(), asked.map(|a| a.song.track)), ("asked", Some("t".into())));
        // A track that starts meanwhile is queued; it is no reason to stop the editor's job.
        s.queue.set_current(job("next"));
        assert!(!s.queue.should_preempt());
        let (second, asked) = s.next().unwrap();
        assert_eq!((second.song.video_id.as_str(), asked.is_some()), ("next", false), "asked once, run once");
    }

    #[test]
    fn the_editors_songs_are_timed_in_the_order_asked_and_asking_again_keeps_the_place() {
        let mut s = Shared::default();
        s.ask(ask("one", "first"));
        s.ask(ask("two", "second"));
        s.ask(ask("again", "first"));
        let tracks = |s: &Shared| s.timing().into_iter().map(|r| r.track).collect::<Vec<_>>();
        assert_eq!(tracks(&s), ["first", "second"]);
        let (job, asked) = s.next().unwrap();
        s.running = asked.map(|a| a.song);
        assert_eq!((job.song.video_id.as_str(), tracks(&s)), ("again", vec!["first".to_string(), "second".into()]), "the newer lines, in the older one's place");
        assert_eq!(s.next().unwrap().0.song.video_id, "two");
        assert!(s.next().is_none());
    }

    #[test]
    fn asking_again_for_the_song_running_stops_it_and_only_the_newer_job_answers() {
        let _guard = CANCEL.lock().unwrap_or_else(|e| e.into_inner());
        let mut s = Shared::default();
        s.ask(ask("older", "song"));
        let (_, older) = s.next().unwrap();
        let older = older.unwrap();
        s.working_on = Some("song — a".into());
        s.running = Some(older.song.clone());
        P.cancel.store(false, Ordering::Relaxed);
        s.ask(ask("newer", "song"));
        assert!(P.cancel.load(Ordering::Relaxed), "the older lines make way");
        // The older one ends (stopped, or finished past its last checkpoint): it
        // keeps nothing and says nothing; the newer one runs.
        assert!(s.superseded(&older));
        assert_eq!(s.next().unwrap().0.song.video_id, "newer");
        assert!(!s.superseded(&older), "switched off: nothing newer, so it says it was stopped");
    }

    #[test]
    fn a_newer_ask_replaces_a_waiting_one_whatever_it_was_for() {
        let mut s = Shared::default();
        let own = Ask { asked: Asked { purpose: Purpose::Saved, ..ask("own", "song").asked }, job: job("own") };
        s.ask(own);
        s.ask(ask("pasted", "song"));
        let (job, asked) = s.next().unwrap();
        assert_eq!((job.song.video_id.as_str(), asked.map(|a| a.purpose)), ("pasted", Some(Purpose::Pasted { was: None })), "the words the user gave last");
        assert!(s.next().is_none());
    }

    #[test]
    fn an_editors_job_stops_a_queue_job_but_not_another_editors() {
        let _guard = CANCEL.lock().unwrap_or_else(|e| e.into_inner());
        let mut s = Shared::default();
        s.working_on = Some("queued — song".into());
        P.cancel.store(false, Ordering::Relaxed);
        s.ask(ask("one", "first"));
        assert!(P.cancel.load(Ordering::Relaxed), "the queue's job makes way");
        P.cancel.store(false, Ordering::Relaxed);
        s.running = Some(SongRef { artist: "a".into(), track: "first".into() });
        s.ask(ask("two", "second"));
        assert!(!P.cancel.load(Ordering::Relaxed), "it waits its turn");
    }

    /// Tests that touch the shared cancel flag, one at a time.
    static CANCEL: Mutex<()> = Mutex::new(());

    #[test]
    fn the_editor_gets_its_lines_timed_or_the_reason() {
        let lyrics = job("a").lyrics.unwrap();
        let al = |start: f64, text: &str| AlignedLine {
            start: Some(start),
            end: Some(start + 2.0),
            conf: 0.9,
            words: vec![align::AlignedWord { start, end: start + 2.0, text: text.into() }],
        };
        let json = serde_json::to_string(&[al(10.0, "one"), al(14.0, "two"), al(20.5, "three")]).unwrap();
        let timed = answer(&lyrics, &Ok("aligned"), AlignedCache::Hit(json)).unwrap();
        let shown: Vec<_> = timed.iter().map(|l| (l.time, l.text.as_str(), l.duration)).collect();
        assert_eq!(shown, [(10.0, "one", Some(4.0)), (14.0, "two", Some(6.5)), (20.5, "three", Some(3.0))], "times it never had");

        assert!(answer(&lyrics, &Ok("rejected (low confidence)"), AlignedCache::Rejected).is_err());
        let skipped = Err(Failure::Skip("audio is 200s, the player says 250s".into()));
        assert_eq!(answer(&lyrics, &skipped, AlignedCache::Miss).unwrap_err(), "audio is 200s, the player says 250s");
        assert_eq!(answer(&lyrics, &Err(Failure::Cancelled), AlignedCache::Miss).unwrap_err(), STOPPED);
    }

    #[test]
    fn pasted_lines_are_trimmed_and_blank_ones_dropped() {
        let pasted = ["  one ".to_string(), String::new(), "   ".into(), "two".into()];
        let lines = untimed(&pasted);
        assert_eq!(lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), ["one", "two"]);
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
