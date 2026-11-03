//! Lyrics lookup: a short, sequential conversation with LRCLIB, then one
//! LLM call for the song's real names, then the opt-in extra sources.
//!
//! 1. Cache, then YouTube Music's own lyrics when the track plays there and
//!    extra sources are on (`from_ytmusic`).
//! 2. `/api/get` with the player's title, then with the regex-cleaned title.
//! 3. One structured `/api/search` for the cleaned title; one full-text `q=`
//!    search only if nothing so far found the song.
//! 4. One LLM call for canonical names, then `/api/get` + search with them.
//! 5. Extra sources (QQ / NetEase / Kugou), when enabled: also when LRCLIB
//!    failed. They search by the identified native names first, then the
//!    player's: QQ and co. list 雪国 / ヨルシカ, not "Snow Country" / Yorushika.
//!
//! "Search again" (`refetch`) forgets the cached answer and asks the LLM first,
//! with web search, then falls back to the player's names: the regex pass is
//! what usually found the wrong song.
//!
//! At most `MAX_REQUESTS` LRCLIB requests per song, one at a time, and no URL
//! twice. A plain-only hit spends one of them looking for a synced entry of the
//! same recording.

use crate::cleaning::{clean_title, normalize_name, Names};
use crate::llm;
use crate::prealign::{self, Served};
use crate::lyrics_cache::{
    cache_instrumental, cache_lyrics, cache_no_lyrics, forget_lyrics, get_cached_lyrics, song_key, CacheResult,
};
use crate::llm::identify::SongIdentity;
use crate::sources::{self, align, Found, Provider};
use crate::timing;
use crate::types::LyricLine;
use crate::ytmusic::{self, YtLyrics};
use anyhow::Result;
use log::{debug, info, warn};
use regex::Regex;
use reqwest::{Client, StatusCode};
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex as StdMutex, OnceLock};
use std::time::Duration;
use tauri::Emitter;

/// Most LRCLIB requests one song may cost (retries of a busy answer aside).
const MAX_REQUESTS: usize = 5;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
/// A search result scoring below this is some other song.
const MIN_SCORE: f32 = 0.6;
/// Fewer lines than this is a junk record (LRCLIB has some, e.g. "[00:00.00]probe"),
/// not a song.
const MIN_RECORD_LINES: usize = 3;

// LRCLIB etiquette (https://lrclib.net/docs): identify the client, send requests
// one at a time with a short gap, and honour Retry-After on 429/503. Ignoring it
// gets "ServerOverloaded" answers and eventually a temporary ban.
const LRCLIB_USER_AGENT: &str = concat!(
    "Lyryc v",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/Error0229/Lyryc)"
);
const LRCLIB_MIN_GAP: Duration = Duration::from_millis(250);
const LRCLIB_MAX_RETRIES: u32 = 2;
const LRCLIB_MAX_RETRY_WAIT: Duration = Duration::from_secs(5);

/// When the last LRCLIB request finished. The lock is held across each request,
/// which is what serializes all LRCLIB traffic in the app.
static LRCLIB_LAST: tokio::sync::Mutex<Option<tokio::time::Instant>> =
    tokio::sync::Mutex::const_new(None);

fn client() -> &'static Client {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        Client::builder()
            .user_agent(LRCLIB_USER_AGENT)
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create lyrics HTTP client")
    })
}

/// The one way to send a request to LRCLIB: serialized, spaced, and retrying
/// 429/503 after `Retry-After` (with up to 30% jitter, as the docs ask). Returns
/// the final response whatever its status; callers decide what it means.
async fn lrclib_get(
    client: &Client,
    url: &str,
    per_req_timeout: Duration,
) -> Result<reqwest::Response> {
    let mut attempt = 0;
    loop {
        let resp = {
            let mut last = LRCLIB_LAST.lock().await;
            if let Some(at) = *last {
                tokio::time::sleep_until(at + LRCLIB_MIN_GAP).await;
            }
            let resp = client.get(url).timeout(per_req_timeout).send().await;
            *last = Some(tokio::time::Instant::now());
            resp?
        };

        let status = resp.status();
        let busy = status == StatusCode::TOO_MANY_REQUESTS
            || status == StatusCode::SERVICE_UNAVAILABLE;
        // Retry only when the server says when: retrying an overloaded server
        // on a guess triples the load and costs ~3 s per question.
        let wait = match retry_after(&resp) {
            Some(wait) if busy && attempt < LRCLIB_MAX_RETRIES && wait <= LRCLIB_MAX_RETRY_WAIT => wait,
            _ => return Ok(resp),
        };
        attempt += 1;
        debug!("LRCLIB answered {} for {}; retrying in {:?}", status, url, wait);
        tokio::time::sleep(with_jitter(wait)).await;
    }
}

/// After this many LRCLIB requests in a row got no answer, stop asking...
const LRCLIB_TRIP_AFTER: u32 = 3;
/// ...for this long. The first request after it probes; a failure pauses again.
const LRCLIB_COOLDOWN: Duration = Duration::from_secs(120);

/// LRCLIB's recent health. While it is paused, every question is answered
/// "no answer" at once (so nothing is negative-cached), and the lookup moves
/// on to the LLM and the sources instead of waiting on a server that is down.
#[derive(Default)]
struct Health {
    failures: u32,
    paused_until: Option<tokio::time::Instant>,
}

impl Health {
    fn paused(&self, now: tokio::time::Instant) -> bool {
        self.paused_until.is_some_and(|until| now < until)
    }

    fn record(&mut self, ok: bool, now: tokio::time::Instant) {
        if ok {
            *self = Health::default();
            return;
        }
        self.failures += 1;
        if self.failures >= LRCLIB_TRIP_AFTER && !self.paused(now) {
            warn!("LRCLIB is not answering; pausing it for {}s", LRCLIB_COOLDOWN.as_secs());
            self.paused_until = Some(now + LRCLIB_COOLDOWN);
        }
    }
}

static LRCLIB_HEALTH: LazyLock<StdMutex<Health>> = LazyLock::new(Default::default);

fn retry_after(resp: &reqwest::Response) -> Option<Duration> {
    let secs: f64 = resp
        .headers()
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse()
        .ok()?;
    (secs.is_finite() && secs >= 0.0).then(|| Duration::from_secs_f64(secs))
}

/// Add 0-30% so clients told the same Retry-After don't come back in lockstep.
fn with_jitter(wait: Duration) -> Duration {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    wait.mul_f64(1.0 + f64::from(nanos % 300) / 1000.0)
}

fn lrclib_url(endpoint: &str, params: &[(&str, &str)]) -> String {
    let query: Vec<String> = params
        .iter()
        .map(|(k, v)| format!("{k}={}", urlencoding::encode(v)))
        .collect();
    format!("https://lrclib.net/api/{endpoint}?{}", query.join("&"))
}

/// What one LRCLIB request came back with.
#[derive(Clone, Debug)]
enum Reply {
    Json(Value),
    /// 404: LRCLIB has no such track. A real answer.
    NotFound,
    /// No usable answer (network, timeout, 5xx after retries).
    Failed(String),
    /// Not asked: LRCLIB is paused after failing (see `Health`).
    Paused,
}

/// Synced lyrics from one LRCLIB record. None for plain-only, instrumental,
/// empty and junk records.
fn synced_from_record(record: &Value) -> Option<Vec<LyricLine>> {
    let synced = record["syncedLyrics"].as_str().filter(|s| !s.trim().is_empty())?;
    let lyrics = parse_lrc_format(synced);
    (lyrics.len() >= MIN_RECORD_LINES).then_some(lyrics)
}

fn plain_from_record(record: &Value) -> Option<&str> {
    let plain = record["plainLyrics"].as_str()?;
    (plain.lines().filter(|l| !l.trim().is_empty()).count() >= MIN_RECORD_LINES).then_some(plain)
}

fn has_lyrics(record: &Value) -> bool {
    synced_from_record(record).is_some() || plain_from_record(record).is_some()
}

/// The LRCLIB entry an estimated result came from.
#[derive(Clone, Debug)]
struct PlainSource {
    track: String,
    artist: String,
    duration: f64,
}

/// What LRCLIB had for the song.
#[derive(Debug)]
enum Lrclib {
    /// Real timing: a synced entry, or a plain hit upgraded to one.
    Synced(Vec<LyricLine>),
    /// Plain lyrics only, with an estimate (cached as one).
    Plain { estimate: Vec<LyricLine>, src: PlainSource },
}

// ---------------------------------------------------------------------------
// Matching search results to the song.

/// The names a search result is compared against.
struct Want {
    titles: Vec<String>,
    artists: Vec<String>,
    duration: Option<f64>,
}

/// 1 for the same name, `contains` when one holds the other, 0.4-0.9 by shared words.
fn string_match_score(expected: &str, candidate: &str, contains: f32) -> f32 {
    let (e, c) = (normalize_name(expected), normalize_name(candidate));
    if e.is_empty() || c.is_empty() {
        return 0.0;
    }
    if e == c {
        return 1.0;
    }
    if c.contains(&e) || e.contains(&c) {
        return contains;
    }
    let words: Vec<&str> = e.split(' ').collect();
    let shared = words.iter().filter(|w| c.split(' ').any(|x| x == **w)).count();
    if shared == 0 {
        0.0
    } else {
        0.4 + 0.5 * shared as f32 / words.len() as f32
    }
}

static COVER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)cover|karaoke|instrumental|remix").unwrap());

/// Title 0.7, artist 0.25, synced 0.05; minus covers and a length mismatch.
/// LRCLIB's names are canonical, so they are only normalized, not cleaned;
/// either half of a bilingual "雪国 - Snow Country" counts as the title.
fn score(record: &Value, want: &Want) -> f32 {
    let track = record["trackName"].as_str().unwrap_or("");
    let artist = record["artistName"].as_str().unwrap_or("");
    let mut tracks = vec![track];
    if let Some((native, translated)) = track.split_once(" - ") {
        tracks.extend([native, translated]);
    }
    let best = |expected: &[String], candidates: &[&str], contains: f32| {
        expected
            .iter()
            .flat_map(|e| candidates.iter().map(move |c| string_match_score(e, c, contains)))
            .fold(0.0f32, f32::max)
    };
    let mut s = 0.7 * best(&want.titles, &tracks, 0.85)
        + 0.25 * best(&want.artists, &[artist], 0.7);
    if synced_from_record(record).is_some() {
        s += 0.05;
    }
    if COVER.is_match(track) && !want.titles.iter().any(|t| COVER.is_match(t)) {
        s -= 0.15;
    }
    if let (Some(want_secs), Some(secs)) = (want.duration, record["duration"].as_f64()) {
        // Same recording within LRCLIB's 2 s; an exact name at any length still passes.
        s -= ((want_secs - secs).abs() as f32 - 2.0).clamp(0.0, 10.0) / 40.0;
    }
    s.clamp(0.0, 1.0)
}

/// The lyric-bearing result that best names the song, if any is close enough.
fn best_match(results: Vec<Value>, want: &Want) -> Option<Value> {
    let (s, record) = results
        .into_iter()
        .filter(has_lyrics)
        .map(|r| (score(&r, want), r))
        .max_by(|a, b| a.0.total_cmp(&b.0))?;
    debug!(
        "Best search result: '{}' by '{}' ({:.2})",
        record["trackName"].as_str().unwrap_or(""),
        record["artistName"].as_str().unwrap_or(""),
        s
    );
    (s >= MIN_SCORE).then_some(record)
}

// ---------------------------------------------------------------------------
// The lookup.

/// One song's conversation with LRCLIB: the budget, the answers so far, and
/// whether every question got one.
struct Lookup<'a, E> {
    fx: &'a E,
    req: &'a Request,
    /// Answers by URL, so a repeated question costs nothing.
    asked: HashMap<String, Option<Value>>,
    /// Some request got no answer, so "nothing found" is not trustworthy and
    /// must not be negative-cached.
    incomplete: bool,
    /// LRCLIB's exact lookup matched a record marked instrumental.
    instrumental: bool,
}

impl<'a, E: Effects> Lookup<'a, E> {
    fn new(fx: &'a E, req: &'a Request) -> Self {
        Lookup { fx, req, asked: HashMap::new(), incomplete: false, instrumental: false }
    }

    async fn ask(&mut self, url: String) -> Option<Value> {
        if let Some(answer) = self.asked.get(&url) {
            return answer.clone();
        }
        if self.asked.len() >= MAX_REQUESTS {
            debug!("LRCLIB budget spent; skipping {}", url);
            return None;
        }
        if self.req.background {
            after_live_lookups().await;
        }
        let answer = match self.fx.lrclib(&url).await {
            Reply::Json(v) => Some(v),
            Reply::NotFound => None,
            Reply::Failed(e) => {
                warn!("LRCLIB gave no answer for {}: {}", url, e);
                self.incomplete = true;
                None
            }
            Reply::Paused => {
                debug!("LRCLIB paused; not asking {}", url);
                self.incomplete = true;
                None
            }
        };
        self.asked.insert(url, answer.clone());
        answer
    }

    /// `/api/get`: LRCLIB's exact lookup by names and length (+-2 s).
    async fn exact(&mut self, title: &str, artist: &str) -> Option<Value> {
        let secs = self.req.duration?.round().to_string();
        if title.trim().is_empty() || artist.trim().is_empty() {
            return None;
        }
        let url = lrclib_url("get", &[("track_name", title), ("artist_name", artist), ("duration", &secs)]);
        let record = self.ask(url).await?;
        // Same names, same length: the flag is about this recording.
        self.instrumental |= record["instrumental"].as_bool() == Some(true);
        Some(record).filter(has_lyrics)
    }

    async fn search(&mut self, params: &[(&str, &str)], want: &Want) -> Option<Value> {
        match self.ask(lrclib_url("search", params)).await? {
            Value::Array(results) => best_match(results, want),
            _ => None,
        }
    }

    /// Steps 2-3 for one spelling: exact lookups for each title, one structured
    /// search for the last, and with `full_text` one `q=` search if all missed.
    async fn find(&mut self, titles: &[String], artist: &str, full_text: bool) -> Option<Lrclib> {
        for title in titles {
            if let Some(record) = self.exact(title, artist).await {
                return Some(self.settle(record).await);
            }
        }
        let title = titles.last()?;
        let mut want_titles = titles.to_vec();
        want_titles.push(self.req.title.clone());
        let want = Want {
            titles: want_titles,
            artists: vec![artist.to_string(), self.req.artist.clone()],
            duration: self.req.duration,
        };
        let mut record = self.search(&[("track_name", title), ("artist_name", artist)], &want).await;
        if record.is_none() && full_text {
            record = self.search(&[("q", &format!("{title} {artist}"))], &want).await;
        }
        Some(self.settle(record?).await)
    }

    /// A lyric-bearing record as an answer. Plain-only: look for a synced
    /// entry of the same recording under LRCLIB's own spelling first.
    async fn settle(&mut self, record: Value) -> Lrclib {
        if let Some(lines) = synced_from_record(&record) {
            return Lrclib::Synced(lines);
        }
        let src = PlainSource {
            track: record["trackName"].as_str().unwrap_or("").to_string(),
            artist: record["artistName"].as_str().unwrap_or("").to_string(),
            duration: record["duration"].as_f64().unwrap_or(0.0),
        };
        if src.duration > 0.0 {
            let url = lrclib_url("search", &[("track_name", &src.track), ("artist_name", &src.artist)]);
            if let Some(Value::Array(results)) = self.ask(url).await {
                let synced = timing::pick_synced_alternative(&results, src.duration).map(|lrc| parse_lrc_format(&lrc));
                if let Some(lines) = synced.filter(|l| l.len() >= MIN_RECORD_LINES) {
                    info!("Upgraded plain lyrics to a synced LRCLIB entry for '{}'", src.track);
                    return Lrclib::Synced(lines);
                }
            }
        }
        info!("No synced timing for '{}'; returning estimated timing", src.track);
        let plain = plain_from_record(&record).unwrap_or("");
        Lrclib::Plain { estimate: timing::estimate_plain(plain, src.duration), src }
    }

    /// Steps 2-3 with the player's names and the regex-cleaned title.
    async fn regex_pass(&mut self) -> Option<Lrclib> {
        let (raw, artist) = (self.req.title.clone(), self.req.artist.clone());
        let cleaned = clean_title(&raw, &artist);
        let titles = if cleaned.is_empty() || cleaned == raw { vec![raw] } else { vec![raw, cleaned] };
        self.find(&titles, &artist, true).await
    }

    /// A song's words from LRCLIB, whatever recording they were filed under:
    /// one search, synced text preferred, timing dropped.
    async fn words_of(&mut self, song: &Names) -> Option<Vec<String>> {
        let want = Want { titles: vec![song.title.clone()], artists: vec![song.artist.clone()], duration: None };
        let record = self.search(&[("track_name", &song.title), ("artist_name", &song.artist)], &want).await?;
        if let Some(lines) = synced_from_record(&record) {
            return Some(lines.into_iter().map(|l| l.text).collect());
        }
        let plain = plain_from_record(&record)?;
        Some(plain.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect())
    }

    /// Step 4: an exact lookup and a search with the identified names.
    async fn identified_pass(&mut self, names: &[Names]) -> Option<Lrclib> {
        let best = names.first()?;
        self.find(&[best.title.clone()], &best.artist, false).await
    }
}

// ---------------------------------------------------------------------------
// The contract.

/// Where the timing of a lyric list came from: the contract's `source`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TimingSource {
    Lrclib,
    /// Plain LRCLIB lyrics spread over the track by syllables. Not real timing.
    LrclibEstimate,
    Qq,
    Netease,
    Kugou,
    /// YouTube Music's own timed lyrics.
    Ytmusic,
    /// YouTube Music's plain lyrics spread over the track by syllables.
    YtmusicEstimate,
    /// Word-level timing from the opt-in pre-aligner (YouTube Music audio).
    Aligned,
    /// No lyrics: the song has no words (no `lines`). Not a timing.
    Instrumental,
    /// A cover: the original's words spread over this recording by syllables.
    CoverEstimate,
}

impl TimingSource {
    /// The cache's name for it; also the serialized name.
    fn as_str(self) -> &'static str {
        match self {
            TimingSource::Lrclib => "lrclib",
            TimingSource::LrclibEstimate => "lrclib-estimate",
            TimingSource::Qq => "qq",
            TimingSource::Netease => "netease",
            TimingSource::Kugou => "kugou",
            TimingSource::Ytmusic => "ytmusic",
            TimingSource::YtmusicEstimate => "ytmusic-estimate",
            TimingSource::Aligned => "aligned",
            TimingSource::Instrumental => "instrumental",
            TimingSource::CoverEstimate => "cover-estimate",
        }
    }

    /// Plain text spread over the track, not real timing.
    fn is_estimate(self) -> bool {
        matches!(self, TimingSource::LrclibEstimate | TimingSource::YtmusicEstimate | TimingSource::CoverEstimate)
    }

    /// Cache rows from before sources were recorded are LRCLIB.
    fn from_cache(name: Option<&str>) -> Self {
        match name {
            Some("qq") => TimingSource::Qq,
            Some("netease") => TimingSource::Netease,
            Some("kugou") => TimingSource::Kugou,
            Some("ytmusic") => TimingSource::Ytmusic,
            Some("lrclib-estimate") => TimingSource::LrclibEstimate,
            Some("ytmusic-estimate") => TimingSource::YtmusicEstimate,
            Some("cover-estimate") => TimingSource::CoverEstimate,
            _ => TimingSource::Lrclib,
        }
    }
}

impl From<Provider> for TimingSource {
    fn from(p: Provider) -> Self {
        match p {
            Provider::Qq => TimingSource::Qq,
            Provider::Netease => TimingSource::Netease,
            Provider::Kugou => TimingSource::Kugou,
        }
    }
}

/// `fetch_lyrics_timed` answer.
#[derive(Debug, Serialize)]
pub struct TimedLyrics {
    pub lines: Vec<LyricLine>,
    pub source: TimingSource,
    /// A `lyrics-timing-updated` event may still follow for this track.
    pub pending: bool,
}

impl TimedLyrics {
    fn instrumental() -> Self {
        TimedLyrics { lines: Vec::new(), source: TimingSource::Instrumental, pending: false }
    }
}

pub const TIMING_UPDATED_EVENT: &str = "lyrics-timing-updated";

/// `lyrics-timing-updated` payload, keyed by the names the frontend asked for.
#[derive(Clone, Debug, Serialize)]
pub struct TimingUpdate {
    pub title: String,
    pub artist: String,
    pub lines: Vec<LyricLine>,
    pub source: TimingSource,
}

/// One lookup, as the frontend asked for it.
#[derive(Clone, Debug)]
struct Request {
    title: String,
    artist: String,
    /// Player length in seconds, if plausible.
    duration: Option<f64>,
    /// The YouTube Music track being played, if that is the player.
    video_id: Option<String>,
    /// Opt-in, paid: when identification without search finds nothing, retry
    /// it with the provider's web search.
    web_search: bool,
    /// The user pressed "Search again": the last answer was wrong.
    refetch: bool,
    /// A lookahead or pre-aligner lookup: lets the song on screen use LRCLIB first.
    background: bool,
}

impl Request {
    fn new(title: String, artist: String, duration: Option<f64>) -> Self {
        let duration = duration.filter(|d| (1.0..=3600.0).contains(d));
        Self { title, artist, duration, video_id: None, web_search: false, refetch: false, background: false }
    }
}

/// A background lookup that may replace an estimate with real timing.
#[derive(Debug)]
struct Upgrade {
    query: sources::Query,
    plain: Vec<String>,
    /// The song's `corrections()` when the job was made.
    corrections: u64,
}

/// How many times the user has corrected each song this session ("Search
/// again", "Mark as instrumental"). An upgrade begun before a correction must
/// neither overwrite it in the cache nor put the wrong lines back on screen.
static CORRECTIONS: LazyLock<StdMutex<HashMap<(String, String), u64>>> = LazyLock::new(Default::default);

fn corrections(req: &Request) -> u64 {
    CORRECTIONS.lock().unwrap().get(&song_key(&req.artist, &req.title)).copied().unwrap_or(0)
}

fn correct(artist: &str, title: &str) {
    *CORRECTIONS.lock().unwrap().entry(song_key(artist, title)).or_default() += 1;
}

/// The LLM's answer about a song.
#[derive(Clone, Debug, Default, PartialEq)]
struct SongNames {
    /// Native spelling first.
    names: Vec<Names>,
    /// It has no words: there are no lyrics to find.
    instrumental: bool,
    /// A cover: the original song's names, whose words it sings.
    cover_of: Option<Names>,
}

/// Everything the lookup does to the outside world, behind a trait so tests
/// run it offline and count its requests.
trait Effects: Send + Sync {
    fn cached(&self, artist: &str, track: &str) -> CacheResult;
    /// Send one LRCLIB request.
    fn lrclib(&self, url: &str) -> impl Future<Output = Reply> + Send;
    /// What the LLM says the song is; empty names when it is not set up,
    /// failed or is unsure.
    fn names(&self, req: &Request, web_search: bool) -> impl Future<Output = SongNames> + Send;
    fn sources(&self, q: &sources::Query, plain: Option<&[String]>) -> impl Future<Output = Option<Found>> + Send;
    /// YouTube Music's lyrics for the track; None when it has none or failed.
    fn ytmusic(&self, video_id: &str) -> impl Future<Output = Option<YtLyrics>> + Send;
    fn save(&self, artist: &str, track: &str, lines: &[LyricLine], source: TimingSource);
    fn save_none(&self, artist: &str, track: &str);
    fn save_instrumental(&self, artist: &str, track: &str);
    /// The pre-aligner's word timing for these lines (`prealign::serve`).
    fn aligned(&self, _req: &Request, _lines: &[LyricLine], _source: TimingSource) -> Served {
        Served::Off
    }
}

struct Live;

async fn lrclib_reply(url: &str) -> Reply {
    let resp = match lrclib_get(client(), url, REQUEST_TIMEOUT).await {
        Ok(resp) => resp,
        Err(e) => return Reply::Failed(e.to_string()),
    };
    if resp.status() == StatusCode::NOT_FOUND {
        return Reply::NotFound;
    }
    let json = async { resp.error_for_status()?.json::<Value>().await };
    match json.await {
        Ok(v) => Reply::Json(v),
        Err(e) => Reply::Failed(e.to_string()),
    }
}

impl Effects for Live {
    fn cached(&self, artist: &str, track: &str) -> CacheResult {
        get_cached_lyrics(artist, track)
    }
    async fn lrclib(&self, url: &str) -> Reply {
        if LRCLIB_HEALTH.lock().unwrap().paused(tokio::time::Instant::now()) {
            return Reply::Paused;
        }
        let reply = lrclib_reply(url).await;
        let ok = !matches!(reply, Reply::Failed(_));
        LRCLIB_HEALTH.lock().unwrap().record(ok, tokio::time::Instant::now());
        reply
    }
    /// Song identification as measured in docs/research/llm-search and
    /// llm-lyrics-deep: the release's own names, e.g. "Snow Country" / Yorushika
    /// -> 雪国 / ヨルシカ. With `web_search` the provider searches first (10/10
    /// hard cases in the study, ~$0.015-0.05 per song on the user's key).
    async fn names(&self, req: &Request, web_search: bool) -> SongNames {
        let Some(id) = llm::identify::identify_song(&req.title, &req.artist, req.duration, web_search).await else {
            return SongNames::default();
        };
        info!(
            "Identified '{}' by '{}' as '{}' by '{}'{}{}",
            req.title,
            req.artist,
            id.title_native,
            id.artist_native,
            match (&id.cover_of, id.instrumental) {
                (_, true) => ", instrumental".to_string(),
                (Some(original), _) => format!(", a cover of {original}"),
                _ => String::new(),
            },
            if web_search { " (web search)" } else { "" }
        );
        let cover_of = id.cover_of.clone().map(|artist| Names { artist, title: id.title_native.clone() });
        SongNames { names: identified_names(&id), instrumental: id.instrumental, cover_of }
    }
    fn sources(&self, q: &sources::Query, plain: Option<&[String]>) -> impl Future<Output = Option<Found>> + Send {
        sources::find(q, plain)
    }
    async fn ytmusic(&self, video_id: &str) -> Option<YtLyrics> {
        ytmusic::lyrics(video_id).await.unwrap_or_else(|e| {
            warn!("YouTube Music lyrics for {}: {}", video_id, e);
            None
        })
    }
    fn save(&self, artist: &str, track: &str, lines: &[LyricLine], source: TimingSource) {
        cache_lyrics(artist, track, lines, source.as_str());
    }
    fn save_none(&self, artist: &str, track: &str) {
        cache_no_lyrics(artist, track);
    }
    fn save_instrumental(&self, artist: &str, track: &str) {
        cache_instrumental(artist, track);
    }
    fn aligned(&self, req: &Request, lines: &[LyricLine], source: TimingSource) -> Served {
        prealign::serve(req.video_id.as_deref(), &req.title, &req.artist, req.duration, lines, !source.is_estimate())
    }
}

#[tauri::command]
pub async fn fetch_lyrics(
    track_name: String,
    artist_name: String,
    duration: Option<f64>,       // seconds, from the player; enables the exact lookup
    _request_id: Option<String>, // Keep for compatibility
) -> Result<Vec<LyricLine>, String> {
    let req = Request::new(track_name, artist_name, duration);
    fetch_timed(&Live, &req, false).await.map(|(timed, _)| timed.lines)
}

/// `fetch_lyrics` plus where the timing came from. With `extra_sources`, a
/// plain-only LRCLIB hit answers with the estimate at once and looks for real
/// timing in the background (`lyrics-timing-updated`), and a track LRCLIB
/// lacks (or can't answer for) is looked up in the extra sources.
#[tauri::command]
#[allow(clippy::too_many_arguments)] // flat on purpose: they are the invoke() fields
pub async fn fetch_lyrics_timed(
    app: tauri::AppHandle,
    track_name: String,
    artist_name: String,
    duration: Option<f64>,
    extra_sources: bool,
    video_id: Option<String>, // YouTube Music's id for the track, if it plays there
    web_search: Option<bool>, // the user's "identify with web search" opt-in
    refetch: Option<bool>,    // "Search again": the lyrics on screen are wrong
) -> Result<TimedLyrics, String> {
    let mut req = Request::new(track_name, artist_name, duration);
    req.video_id = video_id.filter(|id| ytmusic::is_video_id(id));
    req.web_search = web_search.unwrap_or(false);
    req.refetch = refetch.unwrap_or(false);
    let (mut timed, mut upgrade) = {
        // The lock first: a lookahead holding it must not wait on this lookup.
        let _song = song_lock(&req).await;
        let _live = LiveLookup::start();
        // Under the lock: a lookup of the song still running when the user
        // pressed Search again saves its answer before this forgets it.
        if req.refetch {
            forget_song(&req);
        } else {
            adopt_queue_names(&req);
        }
        fetch_timed(&Live, &req, extra_sources).await?
    };
    if with_aligned(&Live, &req, &mut timed) {
        upgrade = None; // word timing beats anything a source could add
    }
    if let Some(job) = upgrade {
        tauri::async_runtime::spawn(async move {
            let Some(update) = run_upgrade(&Live, &req, job).await else {
                return;
            };
            if let Err(e) = app.emit(TIMING_UPDATED_EVENT, update) {
                warn!("Failed to emit {}: {}", TIMING_UPDATED_EVENT, e);
            }
        });
    }
    Ok(timed)
}

/// Everything the app remembers about the song led to the wrong lyrics.
fn forget_song(req: &Request) {
    info!("Searching again for '{}' by '{}'", req.title, req.artist);
    correct(&req.artist, &req.title);
    forget_lyrics(&req.artist, &req.title);
    llm::identify::forget(&req.title, &req.artist);
    prealign::forget(req.video_id.as_deref());
}

/// The user says the song has no words: remembered like a found instrumental,
/// so no lookup runs for it again. "Search again" forgets it.
/// Written once the song's running lookup is done: it would overwrite the mark.
#[tauri::command]
pub async fn mark_instrumental(track_name: String, artist_name: String, video_id: Option<String>) {
    let mut req = Request::new(track_name, artist_name, None);
    req.video_id = video_id.filter(|id| ytmusic::is_video_id(id));
    let _song = song_lock(&req).await;
    info!("Marked '{}' by '{}' instrumental", req.title, req.artist);
    correct(&req.artist, &req.title);
    cache_instrumental(&req.artist, &req.title);
}

/// The serving hook: swap in pre-aligned timing when there is some, or mark
/// the answer pending while it is being made. True when swapped.
fn with_aligned<E: Effects>(fx: &E, req: &Request, timed: &mut TimedLyrics) -> bool {
    match fx.aligned(req, &timed.lines, timed.source) {
        Served::Hit(lines) => {
            *timed = TimedLyrics { lines, source: TimingSource::Aligned, pending: false };
            true
        }
        Served::Pending => {
            timed.pending = true;
            false
        }
        Served::Off => false,
    }
}

// ---------------------------------------------------------------------------
// Looking ahead: the queue's next songs, looked up while this one plays, so
// their lyrics come from the cache the moment they start.

/// Mirrored from the overlay: "Prepare lyrics for upcoming songs".
static AHEAD_ON: AtomicBool = AtomicBool::new(false);
static AHEAD_EXTRA: AtomicBool = AtomicBool::new(false);

/// The overlay mirrors the setting (and "Extra lyric sources") here on start
/// and on every change; the backend keeps no copy.
#[tauri::command]
pub fn lookahead_configure(enabled: bool, extra_sources: bool) {
    AHEAD_ON.store(enabled, Ordering::Relaxed);
    AHEAD_EXTRA.store(extra_sources, Ordering::Relaxed);
}

/// `UPCOMING_TRACKS` from the extension: look up each new song once a
/// session, one at a time, in queue order. Never with paid web search.
pub fn on_upcoming(data: &Value) {
    static SEEN: LazyLock<StdMutex<HashSet<String>>> = LazyLock::new(Default::default);
    static TURN: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    if !AHEAD_ON.load(Ordering::Relaxed) {
        return;
    }
    for song in prealign::upcoming(data) {
        if !SEEN.lock().unwrap().insert(song.video_id.clone()) {
            continue;
        }
        tauri::async_runtime::spawn(async move {
            let _turn = TURN.lock().await;
            if AHEAD_ON.load(Ordering::Relaxed) {
                let extra = AHEAD_EXTRA.load(Ordering::Relaxed);
                lookup_ahead(&song.video_id, &song.title, &song.artist, song.duration, extra).await;
            }
        });
    }
}

/// The names the queue gave each video the lookahead looked up.
static QUEUE_NAMES: LazyLock<StdMutex<HashMap<String, (String, String)>>> = LazyLock::new(Default::default);

/// YouTube names one video two ways: the queue shows the uploader's title, the
/// player a translated one ("ご自愛ください - gojiai kudasai" vs "gojiai kudasai",
/// with an English UI). What the lookahead found under the queue's names is the
/// answer for the player's too: copied over unless the player's names have one.
fn adopt_queue_names(req: &Request) {
    let Some(id) = &req.video_id else {
        return;
    };
    let Some((title, artist)) = QUEUE_NAMES.lock().unwrap().get(id).cloned() else {
        return;
    };
    if song_key(&artist, &title) == song_key(&req.artist, &req.title)
        || !matches!(get_cached_lyrics(&req.artist, &req.title), CacheResult::Miss | CacheResult::NegativeHit)
    {
        return;
    }
    match get_cached_lyrics(&artist, &title) {
        CacheResult::Hit(lines, source) => {
            cache_lyrics(&req.artist, &req.title, &lines, source.as_deref().unwrap_or("lrclib"));
        }
        CacheResult::Instrumental => cache_instrumental(&req.artist, &req.title),
        _ => return,
    }
    debug!("'{}' is the queue's '{}': using what the lookahead found", req.title, title);
}

/// Lookups for the song on screen. Background ones (lookahead, pre-aligner)
/// wait for them before each LRCLIB request: one request at a time goes to
/// LRCLIB, and the song playing now should not queue behind the next one's.
static LIVE: AtomicUsize = AtomicUsize::new(0);
static LIVE_DONE: tokio::sync::Notify = tokio::sync::Notify::const_new();

struct LiveLookup;

impl LiveLookup {
    fn start() -> Self {
        LIVE.fetch_add(1, Ordering::SeqCst);
        LiveLookup
    }
}

impl Drop for LiveLookup {
    fn drop(&mut self) {
        LIVE.fetch_sub(1, Ordering::SeqCst);
        LIVE_DONE.notify_waiters();
    }
}

async fn after_live_lookups() {
    loop {
        let done = LIVE_DONE.notified();
        tokio::pin!(done);
        done.as_mut().enable(); // registered before the check: no missed wakeup
        if LIVE.load(Ordering::SeqCst) == 0 {
            return;
        }
        done.await;
    }
}

/// One lookup per song at a time. A second asker (the song starts while its
/// lookahead runs, or the pre-aligner wants it too) waits, then reads the cache.
async fn song_lock(req: &Request) -> tokio::sync::OwnedMutexGuard<()> {
    type Locks = HashMap<(String, String), Arc<tokio::sync::Mutex<()>>>;
    static LOCKS: LazyLock<StdMutex<Locks>> = LazyLock::new(Default::default);
    let lock = {
        let mut locks = LOCKS.lock().unwrap();
        locks.retain(|_, l| Arc::strong_count(l) > 1); // drop the idle ones
        // By video when there is one: the queue and the player may name it differently.
        let key = match &req.video_id {
            Some(id) => (id.clone(), String::new()),
            None => song_key(&req.artist, &req.title),
        };
        locks.entry(key).or_default().clone()
    };
    lock.lock_owned().await
}

/// A song that is about to play, looked up the normal way (cache, YouTube
/// Music's own lyrics for this videoId and the other sources when enabled,
/// LRCLIB with its budget and etiquette), for the lookahead and the
/// pre-aligner. An estimate is upgraded inline when a source has real timing.
/// The bool: real timing.
pub async fn lookup_ahead(
    video_id: &str,
    title: &str,
    artist: &str,
    duration: Option<f64>,
    extra_sources: bool,
) -> Option<(Vec<LyricLine>, bool)> {
    let mut req = Request::new(title.to_string(), artist.to_string(), duration);
    req.video_id = ytmusic::is_video_id(video_id).then(|| video_id.to_string());
    req.background = true;
    if let Some(id) = &req.video_id {
        QUEUE_NAMES.lock().unwrap().insert(id.clone(), (title.to_string(), artist.to_string()));
    }
    let _song = song_lock(&req).await;
    let (timed, upgrade) = fetch_timed(&Live, &req, extra_sources).await.ok()?;
    if let Some(job) = upgrade {
        if let Some(update) = run_upgrade(&Live, &req, job).await {
            return Some((update.lines, true));
        }
    }
    let real = !timed.source.is_estimate();
    Some((timed.lines, real))
}

/// One way of naming the song to LRCLIB and the sources.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Pass {
    /// The player's names, raw and regex-cleaned: LRCLIB only.
    Player,
    /// The LLM's names, with or without web search: LRCLIB, then the sources.
    Identified { web_search: bool },
    /// No new names: the sources only. The song already missed LRCLIB and the LLM.
    SourcesOnly,
}

/// Cheapest first, normally. "Search again" goes the other way: the player's
/// names are what found the wrong song, so the LLM with web search leads (one
/// paid call the user asked for; it falls through when the provider can't search).
fn passes(req: &Request, negative: bool) -> &'static [Pass] {
    use Pass::*;
    const WEB: Pass = Identified { web_search: true };
    const NO_WEB: Pass = Identified { web_search: false };
    match (req.refetch, negative, req.web_search) {
        (true, _, _) => &[WEB, NO_WEB, Player],
        (false, true, _) => &[SourcesOnly],
        (false, false, false) => &[Player, NO_WEB],
        (false, false, true) => &[Player, NO_WEB, WEB],
    }
}

async fn fetch_timed<E: Effects>(
    fx: &E,
    req: &Request,
    extra: bool,
) -> Result<(TimedLyrics, Option<Upgrade>), String> {
    let negative = match fx.cached(&req.artist, &req.title) {
        CacheResult::Hit(lines, source) => {
            info!("Using cached lyrics for: {} by {}", req.title, req.artist);
            let source = TimingSource::from_cache(source.as_deref());
            // A cached estimate still hopes for real timing from the sources.
            let upgrade = if extra && source.is_estimate() { cached_upgrade(req, &lines) } else { None };
            let pending = upgrade.is_some();
            return Ok((TimedLyrics { lines, source, pending }, upgrade));
        }
        CacheResult::Instrumental => return Ok((TimedLyrics::instrumental(), None)),
        CacheResult::NegativeHit => true,
        CacheResult::Miss => false,
    };
    if let Some(answer) = from_ytmusic(fx, req, extra, negative).await {
        return Ok(answer);
    }

    let started = std::time::Instant::now();
    let mut run = Lookup::new(fx, req);
    let mut found = None;
    // The LLM or LRCLIB says the song has no words. That ends the search, except
    // for "Search again", which doubts every earlier answer, this one too.
    let mut instrumental = false;
    let settled = |instrumental: bool| instrumental && !req.refetch;
    // Names the sources were already asked with (e.g. no LLM set up: always empty).
    let mut tried: Vec<Vec<Names>> = Vec::new();
    // Originals whose words were already looked for.
    let mut covers: Vec<Names> = Vec::new();
    for &pass in passes(req, negative) {
        let said = match pass {
            Pass::Identified { web_search } => fx.names(req, web_search).await,
            Pass::Player | Pass::SourcesOnly => SongNames::default(),
        };
        instrumental |= said.instrumental;
        if settled(instrumental) {
            break;
        }
        let names = said.names;
        found = match pass {
            Pass::Player => run.regex_pass().await,
            _ => run.identified_pass(&names).await,
        };
        instrumental |= run.instrumental;
        if found.is_some() || settled(instrumental) {
            break;
        }
        // The player's names reach the sources with every other pass.
        if pass != Pass::Player && !tried.contains(&names) {
            if let Some(timed) = from_sources(fx, req, &names, extra).await {
                return Ok((timed, None));
            }
            tried.push(names);
        }
        if let Some(original) = said.cover_of.filter(|o| trusted_cover(req, pass) && !covers.contains(o)) {
            if let Some(timed) = from_original(fx, &mut run, req, &original, extra).await {
                return Ok((timed, None));
            }
            covers.push(original);
        }
    }
    info!(
        "Lyrics lookup for '{}' by '{}': {} after {} LRCLIB request(s) in {:.1}s",
        req.title,
        req.artist,
        match &found {
            Some(Lrclib::Synced(_)) => "synced",
            Some(Lrclib::Plain { .. }) => "plain only",
            None if instrumental => "instrumental",
            None if run.incomplete => "no answer",
            None => "nothing",
        },
        run.asked.len(),
        started.elapsed().as_secs_f64()
    );

    match found {
        Some(Lrclib::Synced(lines)) => {
            fx.save(&req.artist, &req.title, &lines, TimingSource::Lrclib);
            Ok((TimedLyrics { lines, source: TimingSource::Lrclib, pending: false }, None))
        }
        Some(Lrclib::Plain { estimate, src }) => {
            fx.save(&req.artist, &req.title, &estimate, TimingSource::LrclibEstimate);
            let upgrade = if extra { upgrade_job(req, &estimate, &src) } else { None };
            let pending = upgrade.is_some();
            Ok((TimedLyrics { lines: estimate, source: TimingSource::LrclibEstimate, pending }, upgrade))
        }
        None if instrumental => {
            fx.save_instrumental(&req.artist, &req.title);
            Ok((TimedLyrics::instrumental(), None))
        }
        None if negative => Err("No lyrics found (cached)".to_string()),
        None if run.incomplete => Err("Lyrics service unavailable, try again later".to_string()),
        None => {
            fx.save_none(&req.artist, &req.title);
            Err("No lyrics found".to_string())
        }
    }
}

/// Step 5: with the setting on and a length to match recordings by, the extra
/// sources, whether LRCLIB had nothing or could not answer. A hit is cached.
async fn from_sources<E: Effects>(fx: &E, req: &Request, names: &[Names], extra: bool) -> Option<TimedLyrics> {
    let duration = req.duration.filter(|_| extra)?;
    let q = source_query(&req.title, &req.artist, names, duration);
    let found = fx.sources(&q, None).await?;
    let source = TimingSource::from(found.provider);
    info!("LRCLIB had nothing for '{}'; {} did", req.title, source.as_str());
    fx.save(&req.artist, &req.title, &found.lines, source);
    Some(TimedLyrics { lines: found.lines, source, pending: false })
}

/// "Cover", "歌ってみた", "翻唱" in what the player shows.
static COVER_MARK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bcover(ed)?\b|カバー|歌ってみた|翻唱").unwrap());

/// Taking another artist's words is only safe when the cover is plain: the
/// player says so, or a web search found it. A guessed "cover of" would put
/// some other song's lyrics on screen.
fn trusted_cover(req: &Request, pass: Pass) -> bool {
    pass == (Pass::Identified { web_search: true }) || COVER_MARK.is_match(&req.title) || COVER_MARK.is_match(&req.artist)
}

/// A cover sings the original's words to its own arrangement: the words are
/// taken from the original (LRCLIB, then the sources at any length) and
/// spread over this recording. Word-level timing retimes them to the audio.
async fn from_original<E: Effects>(
    fx: &E,
    run: &mut Lookup<'_, E>,
    req: &Request,
    original: &Names,
    extra: bool,
) -> Option<TimedLyrics> {
    let duration = req.duration?;
    let words = match run.words_of(original).await {
        Some(words) => words,
        None if extra => {
            let q = sources::Query::words_of([sources::Name { title: original.title.clone(), artist: original.artist.clone() }], duration);
            fx.sources(&q, None).await?.lines.into_iter().map(|l| l.text).collect()
        }
        None => return None,
    };
    if words.len() < MIN_RECORD_LINES {
        return None;
    }
    info!("'{}' is a cover of '{}' by '{}': using its words", req.title, original.title, original.artist);
    let lines = timing::estimate_plain(&words.join("\n"), duration);
    fx.save(&req.artist, &req.title, &lines, TimingSource::CoverEstimate);
    Some(TimedLyrics { lines, source: TimingSource::CoverEstimate, pending: false })
}

/// The names the sources search for, best first: the identified native ones,
/// then the player's (title regex-cleaned), then the other identified
/// spellings, which only count as the song's names.
pub(crate) fn source_query(title: &str, artist: &str, names: &[Names], duration: f64) -> sources::Query {
    let name = |n: &Names| sources::Name { title: n.title.clone(), artist: n.artist.clone() };
    let player = sources::Name { title: clean_title(title, artist), artist: artist.to_string() };
    let all = names.first().map(name).into_iter().chain([player]).chain(names.iter().skip(1).map(name));
    sources::Query::new(all, duration)
}

/// The identification's (title, artist) spellings, native first.
pub(crate) fn identified_names(id: &SongIdentity) -> Vec<Names> {
    id.queries().into_iter().map(|(title, artist)| Names { artist, title }).collect()
}

/// Share of YouTube Music's lines that must pair with LRCLIB's to borrow its times.
const MIN_MAPPED: f64 = 0.5;

/// Step 1b: with the setting on and the track playing in YouTube Music, its own
/// lyrics, before LRCLIB: the exact track, so no wrong-song risk. Timed lines
/// are the answer. Plain lines stay the text and borrow times from LRCLIB's
/// synced entry, else are estimated while the extra sources look for timing in
/// the background. None: carry on as if YouTube Music did not exist.
async fn from_ytmusic<E: Effects>(
    fx: &E,
    req: &Request,
    extra: bool,
    negative: bool,
) -> Option<(TimedLyrics, Option<Upgrade>)> {
    let yt = fx.ytmusic(req.video_id.as_deref().filter(|_| extra)?).await?;
    info!("YouTube Music has {} line(s) for '{}' (timed: {}, {:?})", yt.lines.len(), req.title, yt.timed, yt.source);
    let timed: Option<Vec<(f64, String)>> = yt.lines.iter().map(|(t, s)| t.map(|t| (t, s.clone()))).collect();
    if let Some(timed) = timed {
        return Some((save_real(fx, req, timed, TimingSource::Ytmusic), None));
    }

    let text: Vec<String> = yt.lines.into_iter().map(|(_, s)| s).collect();
    let mut run = Lookup::new(fx, req);
    let synced = if negative { None } else { run.regex_pass().await };
    if let Some(Lrclib::Synced(synced)) = synced {
        if let Some(times) = map_onto(&text, &synced, req.duration) {
            info!("Timed YouTube Music's lines for '{}' with LRCLIB's", req.title);
            return Some((save_real(fx, req, times, TimingSource::Lrclib), None));
        }
    }
    let estimate = timing::estimate_plain(&text.join("\n"), req.duration.unwrap_or(0.0));
    fx.save(&req.artist, &req.title, &estimate, TimingSource::YtmusicEstimate);
    let upgrade = cached_upgrade(req, &estimate);
    let pending = upgrade.is_some();
    Some((TimedLyrics { lines: estimate, source: TimingSource::YtmusicEstimate, pending }, upgrade))
}

/// LRCLIB's times on `text`, if at least `MIN_MAPPED` of its lines pair up.
fn map_onto(text: &[String], synced: &[LyricLine], duration: Option<f64>) -> Option<Vec<(f64, String)>> {
    let src: Vec<(f64, String)> = synced.iter().map(|l| (l.time, l.text.clone())).collect();
    let mapped = align::map_lines(text, &src);
    if (mapped.iter().flatten().count() as f64) < MIN_MAPPED * text.len() as f64 {
        return None;
    }
    let end = duration.unwrap_or(0.0).max(src.last()?.0 + LAST_LINE_SECS);
    Some(align::interpolate(&mapped, end).into_iter().zip(text.iter().cloned()).collect())
}

/// Real timing as the answer, cached.
fn save_real<E: Effects>(fx: &E, req: &Request, timed: Vec<(f64, String)>, source: TimingSource) -> TimedLyrics {
    let lines = with_durations(timed.into_iter().map(|(time, text)| LyricLine { time, text, duration: None, words: None }).collect());
    fx.save(&req.artist, &req.title, &lines, source);
    TimedLyrics { lines, source, pending: false }
}

/// Real timing for estimated lines, looked up by the player's names.
fn cached_upgrade(req: &Request, estimate: &[LyricLine]) -> Option<Upgrade> {
    Some(Upgrade {
        query: source_query(&req.title, &req.artist, &[], req.duration?),
        plain: estimate.iter().map(|l| l.text.clone()).collect(),
        corrections: corrections(req),
    })
}

/// Look up LRCLIB's spelling of the song, for the length being played.
fn upgrade_job(req: &Request, estimate: &[LyricLine], src: &PlainSource) -> Option<Upgrade> {
    let duration = req.duration.or((src.duration > 0.0).then_some(src.duration))?;
    Some(Upgrade {
        query: sources::Query::new([sources::Name { title: src.track.clone(), artist: src.artist.clone() }], duration),
        plain: estimate.iter().map(|l| l.text.clone()).collect(),
        corrections: corrections(req),
    })
}

/// Real timing for an estimate, cached, as the event to send.
async fn run_upgrade<E: Effects>(fx: &E, req: &Request, job: Upgrade) -> Option<TimingUpdate> {
    let found = fx.sources(&job.query, Some(&job.plain)).await?;
    if corrections(req) != job.corrections {
        info!("Dropped an upgrade for '{}': the user corrected the song meanwhile", req.title);
        return None;
    }
    let source = TimingSource::from(found.provider);
    info!("Upgraded estimated timing for '{}' from {}", req.title, source.as_str());
    fx.save(&req.artist, &req.title, &found.lines, source);
    Some(TimingUpdate { title: req.title.clone(), artist: req.artist.clone(), lines: found.lines, source })
}

/// One LRCLIB record for the names, synced preferred: a structured search,
/// then a full-text one if that found nothing.
#[tauri::command]
pub async fn fetch_lrclib_raw(
    track_name: String,
    artist_name: String,
) -> Result<serde_json::Value, String> {
    info!("Fetching raw LRCLib for: {} by {}", track_name, artist_name);
    let mut params = vec![("track_name", track_name.as_str())];
    if !artist_name.trim().is_empty() {
        params.push(("artist_name", artist_name.as_str()));
    }
    let full_text = format!("{} {}", track_name, artist_name);
    let mut results = Vec::new();
    for url in [lrclib_url("search", &params), lrclib_url("search", &[("q", &full_text)])] {
        if let Reply::Json(Value::Array(found)) = Live.lrclib(&url).await {
            results = found;
        }
        if !results.is_empty() {
            break;
        }
    }
    let chosen = results
        .iter()
        .find(|r| r["syncedLyrics"].as_str().is_some_and(|s| !s.trim().is_empty()))
        .or(results.first())
        .ok_or("No LRCLib results")?;
    let text = |k: &str| chosen[k].as_str().unwrap_or("");
    Ok(serde_json::json!({
        "trackName": text("trackName"),
        "artistName": text("artistName"),
        "albumName": text("albumName"),
        "duration": chosen["duration"].as_f64().unwrap_or(0.0),
        "plainLyrics": text("plainLyrics"),
        "syncedLyrics": text("syncedLyrics"),
    }))
}

static LRC_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\[(\d{2}):(\d{2})(?:\.(\d{2}))?\](.*)$").unwrap());

pub fn parse_lrc_format(lrc_content: &str) -> Vec<LyricLine> {
    let mut lyrics: Vec<LyricLine> = lrc_content
        .lines()
        .filter_map(|line| {
            let caps = LRC_LINE.captures(line)?;
            let text = caps[4].trim().to_string();
            let num = |i: usize| caps.get(i).map_or(0.0, |m| m.as_str().parse().unwrap_or(0.0));
            (!text.is_empty()).then(|| LyricLine {
                time: num(1) * 60.0 + num(2) + num(3) / 100.0,
                text,
                duration: None,
                words: None,
            })
        })
        .collect();

    lyrics.sort_by(|a, b| a.time.total_cmp(&b.time));
    with_durations(lyrics)
}

/// What the last line of a lyric list is given.
const LAST_LINE_SECS: f64 = 3.0;

/// Each line lasts until the next starts; the last gets `LAST_LINE_SECS`.
fn with_durations(mut lyrics: Vec<LyricLine>) -> Vec<LyricLine> {
    for i in 0..lyrics.len().saturating_sub(1) {
        lyrics[i].duration = Some(lyrics[i + 1].time - lyrics[i].time);
    }
    if let Some(last) = lyrics.last_mut() {
        last.duration = Some(LAST_LINE_SECS);
    }
    lyrics
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Mutex;

    #[test]
    fn exact_path_takes_only_real_synced_records() {
        let synced = json!({
            "syncedLyrics": "[00:01.00]a\n[00:02.00]b\n[00:03.00]c",
            "plainLyrics": "x"
        });
        assert_eq!(synced_from_record(&synced).unwrap()[0].text, "a");

        // Plain-only is settled separately (upgrade or uncached estimate).
        let plain = json!({"syncedLyrics": "  ", "plainLyrics": "one\ntwo\nthree"});
        assert!(synced_from_record(&plain).is_none());
        assert!(has_lyrics(&plain));

        // A real junk record that /api/get returned for a popular song.
        let junk = json!({"syncedLyrics": "[00:00.00]probe", "plainLyrics": "probe"});
        assert!(synced_from_record(&junk).is_none());
        assert!(!has_lyrics(&junk));

        let instrumental = json!({"instrumental": true, "syncedLyrics": null, "plainLyrics": null});
        assert!(!has_lyrics(&instrumental));
    }

    #[test]
    fn jitter_adds_at_most_thirty_percent() {
        let base = Duration::from_secs(1);
        for _ in 0..100 {
            let wait = with_jitter(base);
            assert!(wait >= base && wait < base.mul_f64(1.3));
        }
    }

    const SYNCED: &str = "[00:10.00]one\n[00:20.00]two\n[00:30.00]three";

    fn record(track: &str, artist: &str, duration: f64, synced: bool) -> Value {
        json!({
            "trackName": track,
            "artistName": artist,
            "duration": duration,
            "syncedLyrics": if synced { Some(SYNCED) } else { None },
            "plainLyrics": "one\ntwo\nthree\nfour",
        })
    }

    /// Offline stand-in for LRCLIB, the LLM, the sources and the cache,
    /// recording what the lookup did.
    #[derive(Default)]
    struct Fake {
        negative_cached: bool,
        cached_instrumental: bool,
        cached_hit: Option<(Vec<LyricLine>, Option<String>)>,
        /// The first rule whose text is in the URL answers; otherwise 404.
        lrclib: Vec<(&'static str, Reply)>,
        names: Vec<Names>,
        /// What identification with web search finds.
        names_searched: Vec<Names>,
        /// The LLM says the song has no words.
        llm_instrumental: bool,
        /// The LLM says it is a cover of this song.
        llm_cover_of: Option<Names>,
        saved_instrumental: Mutex<u32>,
        answer: Option<Found>,
        urls: Mutex<Vec<String>>,
        llm_calls: Mutex<u32>,
        /// One entry per sources call: how many known lines it was given.
        calls: Mutex<Vec<Option<usize>>>,
        /// One entry per sources call: the names it was given, as "title / artist".
        searched: Mutex<Vec<Vec<String>>>,
        saved: Mutex<Vec<TimingSource>>,
        saved_none: Mutex<u32>,
        yt: Option<YtLyrics>,
        yt_calls: Mutex<u32>,
        /// What the pre-aligner answers, and the source it was asked with.
        prealigned: Option<Served>,
        aligned_asked: Mutex<Vec<(TimingSource, usize)>>,
    }

    impl Fake {
        fn answering(provider: Provider) -> Self {
            Fake { answer: Some(Found { provider, lines: parse_lrc_format(SYNCED) }), ..Default::default() }
        }
        fn rule(mut self, needle: &'static str, reply: Reply) -> Self {
            self.lrclib.push((needle, reply));
            self
        }
        fn urls(&self) -> Vec<String> {
            self.urls.lock().unwrap().clone()
        }
        fn asked(&self, needle: &str) -> usize {
            self.urls().iter().filter(|u| u.contains(needle)).count()
        }
        fn calls(&self) -> Vec<Option<usize>> {
            self.calls.lock().unwrap().clone()
        }
        fn searched(&self) -> Vec<Vec<String>> {
            self.searched.lock().unwrap().clone()
        }
        fn saved(&self) -> Vec<TimingSource> {
            self.saved.lock().unwrap().clone()
        }
        fn saved_none(&self) -> u32 {
            *self.saved_none.lock().unwrap()
        }
        fn llm_calls(&self) -> u32 {
            *self.llm_calls.lock().unwrap()
        }
    }

    impl Effects for Fake {
        fn cached(&self, _artist: &str, _track: &str) -> CacheResult {
            match &self.cached_hit {
                Some((lines, source)) => CacheResult::Hit(lines.clone(), source.clone()),
                None if self.cached_instrumental => CacheResult::Instrumental,
                None if self.negative_cached => CacheResult::NegativeHit,
                None => CacheResult::Miss,
            }
        }
        fn lrclib(&self, url: &str) -> impl Future<Output = Reply> + Send {
            self.urls.lock().unwrap().push(url.to_string());
            let reply = self.lrclib.iter().find(|(n, _)| url.contains(n)).map_or(Reply::NotFound, |(_, r)| r.clone());
            async move { reply }
        }
        fn names(&self, _req: &Request, web_search: bool) -> impl Future<Output = SongNames> + Send {
            *self.llm_calls.lock().unwrap() += 1;
            let names = if web_search { self.names_searched.clone() } else { self.names.clone() };
            let said = SongNames { names, instrumental: self.llm_instrumental, cover_of: self.llm_cover_of.clone() };
            async move { said }
        }
        fn sources(&self, q: &sources::Query, plain: Option<&[String]>) -> impl Future<Output = Option<Found>> + Send {
            self.calls.lock().unwrap().push(plain.map(<[String]>::len));
            self.searched.lock().unwrap().push(q.names.iter().map(|n| format!("{} / {}", n.title, n.artist)).collect());
            let answer = self.answer.clone();
            async move { answer }
        }
        fn ytmusic(&self, _video_id: &str) -> impl Future<Output = Option<YtLyrics>> + Send {
            *self.yt_calls.lock().unwrap() += 1;
            let yt = self.yt.clone();
            async move { yt }
        }
        fn save(&self, _artist: &str, _track: &str, _lines: &[LyricLine], source: TimingSource) {
            self.saved.lock().unwrap().push(source);
        }
        fn save_none(&self, _artist: &str, _track: &str) {
            *self.saved_none.lock().unwrap() += 1;
        }
        fn save_instrumental(&self, _artist: &str, _track: &str) {
            *self.saved_instrumental.lock().unwrap() += 1;
        }
        fn aligned(&self, _req: &Request, lines: &[LyricLine], source: TimingSource) -> Served {
            self.aligned_asked.lock().unwrap().push((source, lines.len()));
            self.prealigned.clone().unwrap_or(Served::Off)
        }
    }

    /// Four estimated lines, as the cache would hand them back.
    fn estimate_lines() -> Vec<LyricLine> {
        timing::estimate_plain("one\ntwo\nthree\nfour", 265.0)
    }

    fn req() -> Request {
        Request::new("Koi".into(), "Minori".into(), Some(265.0))
    }

    fn json_reply(v: Value) -> Reply {
        Reply::Json(v)
    }

    fn overloaded() -> Reply {
        Reply::Failed("HTTP status server error (503 Service Unavailable)".into())
    }

    #[test]
    fn lrclib_pauses_after_three_failures_in_a_row_then_probes_again() {
        let t0 = tokio::time::Instant::now();
        let mut h = Health::default();
        h.record(false, t0);
        h.record(false, t0);
        h.record(true, t0);
        h.record(false, t0);
        h.record(false, t0);
        assert!(!h.paused(t0), "an answer in between resets the count");
        h.record(false, t0);
        assert!(h.paused(t0));
        let later = t0 + LRCLIB_COOLDOWN;
        assert!(!h.paused(later), "the pause ends; the next request probes");
        h.record(false, later);
        assert!(h.paused(later), "and a failed probe pauses again at once");
        h.record(true, later);
        assert!(!h.paused(later));
    }

    #[tokio::test]
    async fn exact_hit_costs_one_request_and_never_asks_sources() {
        let fx = Fake::answering(Provider::Qq).rule("/get?", json_reply(record("Koi", "Minori", 265.0, true)));
        let (timed, upgrade) = fetch_timed(&fx, &req(), true).await.unwrap();
        assert_eq!((timed.source, timed.pending), (TimingSource::Lrclib, false));
        assert!(upgrade.is_none());
        assert_eq!(fx.urls().len(), 1);
        assert!(fx.calls().is_empty());
        assert_eq!(fx.llm_calls(), 0);
        assert_eq!(fx.saved(), vec![TimingSource::Lrclib]);
    }

    /// The real 'SYNC' / 'Daoko' log: /api/get and the structured search answer
    /// "nothing", and the full-text search is overloaded on every attempt.
    fn sync_daoko(fx: Fake) -> Fake {
        fx.rule("search?track_name=", json_reply(json!([]))).rule("q=", overloaded())
    }

    #[tokio::test]
    async fn lrclib_down_falls_back_to_sources_but_never_caches_a_miss() {
        let fx = sync_daoko(Fake::answering(Provider::Netease));
        let req = Request::new("SYNC".into(), "Daoko".into(), Some(222.0));
        let (timed, _) = fetch_timed(&fx, &req, true).await.unwrap();
        assert_eq!(timed.source, TimingSource::Netease);
        assert_eq!(fx.calls(), vec![None], "no known text: sources use their own lines");
        assert_eq!(fx.saved(), vec![TimingSource::Netease], "a source hit is real timing");
        assert_eq!(fx.saved_none(), 0);
        assert_eq!(fx.asked("q="), 1, "one full-text search");
        assert_eq!(fx.urls().len(), 3, "get, search, q");
        assert_eq!(fx.llm_calls(), 1, "identification runs before the sources");
        assert_eq!(fx.searched(), [["SYNC / Daoko"]], "no LLM names: the player's, as before");

        // Nobody answers: an error, and no "no lyrics" entry (LRCLIB never said so).
        let fx = sync_daoko(Fake::default());
        assert!(fetch_timed(&fx, &req, true).await.is_err());
        assert_eq!(fx.calls().len(), 1);
        assert_eq!(fx.saved_none(), 0);
    }

    #[tokio::test]
    async fn a_bilingual_title_is_asked_for_whole_and_matches_either_half() {
        // YouTube Music sends the artist separately: the dash is part of the title.
        let fx = Fake::default().rule("search?track_name=", json_reply(json!([record("心に穴が空いた", "ヨルシカ", 230.0, true)])));
        let req = Request::new("心に穴が空いた - Hole In The Heart".into(), "Yorushika".into(), Some(230.0));
        let (timed, _) = fetch_timed(&fx, &req, false).await.unwrap();
        assert_eq!(timed.source, TimingSource::Lrclib);
        let urls = fx.urls();
        assert_eq!(urls.len(), 2, "one get (the cleaned title is the same), one search: {urls:#?}");
        assert!(urls.iter().all(|u| u.contains("Hole%20In%20The%20Heart")), "never the English half alone");
    }

    #[tokio::test]
    async fn an_incomplete_lookup_is_not_negative_cached() {
        let fx = sync_daoko(Fake::default());
        let req = Request::new("SYNC".into(), "Daoko".into(), Some(222.0));
        assert!(fetch_timed(&fx, &req, false).await.is_err());
        assert!(fx.calls().is_empty(), "sources are off");
        assert_eq!(fx.llm_calls(), 1);
        assert_eq!(fx.saved_none(), 0);
    }

    #[tokio::test]
    async fn request_cap_holds_when_everything_misses() {
        let names = Names { artist: "Chihara Minori".into(), title: "Koi no Uta".into() };
        let fx = Fake { names: vec![names], ..Default::default() }.rule("search?", json_reply(json!([])));
        let req = Request::new("Minori - Koi (Official Video)".into(), "Minori".into(), Some(265.0));
        assert!(fetch_timed(&fx, &req, false).await.is_err());

        let urls = fx.urls();
        assert_eq!(urls.len(), MAX_REQUESTS, "{urls:#?}");
        assert!(urls[0].contains("/get?track_name=Minori%20-%20Koi"), "raw title first");
        assert!(urls[1].contains("/get?track_name=Koi&"), "then the cleaned title");
        assert!(urls[2].contains("/search?track_name=Koi&"));
        assert!(urls[3].contains("/search?q=Koi%20Minori"));
        assert!(urls[4].contains("/get?track_name=Koi%20no%20Uta&artist_name=Chihara%20Minori"));
        assert_eq!(fx.llm_calls(), 1);
        assert_eq!(fx.saved_none(), 1, "a complete miss is negative-cached");
    }

    #[tokio::test]
    async fn llm_names_find_what_the_player_names_did_not() {
        let fx = Fake { names: vec![Names { artist: "なとり".into(), title: "Overdose".into() }], ..Default::default() }
            .rule("artist_name=%E3%81%AA%E3%81%A8%E3%82%8A", json_reply(json!([record("Overdose", "なとり", 266.0, true)])))
            .rule("search?", json_reply(json!([])));
        let req = Request::new("Overdose".into(), "Natori".into(), None);
        let (timed, _) = fetch_timed(&fx, &req, false).await.unwrap();
        assert_eq!(timed.source, TimingSource::Lrclib);
        assert_eq!(fx.asked("/get?"), 0, "no length, no exact lookups");
        assert_eq!(fx.urls().len(), 3, "search, q, LLM search");
    }

    fn snow_country() -> (Request, Vec<Names>) {
        let req = Request::new("Snow Country".into(), "Yorushika".into(), Some(288.0));
        let id = SongIdentity {
            title_native: "雪国".into(),
            title_romanized: Some("Yukiguni".into()),
            artist_native: "ヨルシカ".into(),
            artist_romanized: Some("Yorushika".into()),
            album: None,
            duration_s: Some(288.0),
            isrc: None,
            instrumental: false,
            cover_of: None,
            confidence: 0.9,
            evidence_urls: vec![],
        };
        (req, identified_names(&id))
    }

    #[tokio::test]
    async fn identified_names_go_to_lrclib_first_then_lead_the_sources() {
        let (req, names) = snow_country();
        let fx = Fake { names, ..Fake::answering(Provider::Qq) }.rule("search?", json_reply(json!([])));
        let (timed, _) = fetch_timed(&fx, &req, true).await.unwrap();
        assert_eq!(timed.source, TimingSource::Qq);
        assert_eq!(fx.llm_calls(), 1);
        let urls = fx.urls();
        assert!(urls.len() <= MAX_REQUESTS, "{urls:#?}");
        assert!(urls.iter().any(|u| u.contains("/get?track_name=%E9%9B%AA%E5%9B%BD&artist_name=%E3%83%A8")), "LRCLIB by the native names: {urls:#?}");
        assert_eq!(
            fx.searched(),
            [[
                "雪国 / ヨルシカ",
                "Snow Country / Yorushika",
                "雪国 / Yorushika",
                "Yukiguni / ヨルシカ",
                "Yukiguni / Yorushika"
            ]],
            "native names first, then the player's, then the other spellings"
        );
        assert_eq!(fx.saved(), vec![TimingSource::Qq]);
    }

    #[tokio::test]
    async fn web_search_identification_is_opt_in_and_only_after_the_free_pass() {
        // The free pass can't place the song; only the web search pass finds 雪国 / ヨルシカ.
        let (mut req, searched) = snow_country();
        let fx = || {
            Fake { names_searched: searched.clone(), ..Default::default() }
                .rule("artist_name=%E3%83%A8", json_reply(json!([record("雪国", "ヨルシカ", 288.0, true)])))
                .rule("search?", json_reply(json!([])))
        };

        let off = fx();
        assert!(fetch_timed(&off, &req, true).await.is_err());
        assert_eq!(off.llm_calls(), 1, "setting off: the free identification only");

        req.web_search = true;
        let on = fx();
        let (timed, _) = fetch_timed(&on, &req, true).await.unwrap();
        assert_eq!(timed.source, TimingSource::Lrclib);
        assert_eq!(on.llm_calls(), 2, "free pass first, then web search");
        assert_eq!(on.calls().len(), 1, "the sources ran once, after the free pass only");
    }

    #[tokio::test]
    async fn web_search_pass_is_skipped_when_it_has_nothing_new() {
        // No LLM answer either way (e.g. none set up): the sources are asked once, not twice.
        let (mut req, _) = snow_country();
        req.web_search = true;
        let fx = Fake::default().rule("search?", json_reply(json!([])));
        assert!(fetch_timed(&fx, &req, true).await.is_err());
        assert_eq!(fx.calls().len(), 1);
    }

    #[tokio::test]
    async fn search_again_asks_the_web_search_llm_before_the_players_names() {
        // The player's names find some other song; the web search names find 雪国.
        let (mut req, searched) = snow_country();
        req.refetch = true;
        let fx = Fake { names_searched: searched, ..Default::default() }
            .rule("artist_name=%E3%83%A8", json_reply(json!([record("雪国", "ヨルシカ", 288.0, true)])))
            .rule("Snow", json_reply(json!([record("Snow Country", "Someone Else", 288.0, true)])));
        let (timed, _) = fetch_timed(&fx, &req, true).await.unwrap();
        assert_eq!(timed.source, TimingSource::Lrclib);
        assert_eq!(fx.llm_calls(), 1, "web search found it; no second LLM call");
        assert_eq!(fx.asked("Snow"), 0, "the player's names were never asked");
        assert_eq!(fx.saved(), vec![TimingSource::Lrclib]);
    }

    #[tokio::test]
    async fn search_again_without_an_llm_still_tries_the_sources_then_the_players_names() {
        let mut fx = Fake::default().rule("search?", json_reply(json!([])));
        fx.negative_cached = true; // ignored: the user asked again
        let mut req = req();
        req.refetch = true;
        assert!(fetch_timed(&fx, &req, true).await.is_err());
        assert_eq!(fx.llm_calls(), 2, "web search, then without");
        assert_eq!(fx.calls().len(), 1, "the sources once: the empty names repeat");
        assert!(fx.asked("Koi") > 0, "the player's names last");
    }

    #[tokio::test]
    async fn an_lrclib_hit_by_identified_names_never_asks_the_sources() {
        let (req, names) = snow_country();
        let fx = Fake { names, ..Fake::answering(Provider::Qq) }
            .rule("artist_name=%E3%83%A8", json_reply(json!([record("雪国", "ヨルシカ", 288.0, true)])))
            .rule("search?", json_reply(json!([])));
        let (timed, _) = fetch_timed(&fx, &req, true).await.unwrap();
        assert_eq!(timed.source, TimingSource::Lrclib);
        assert!(fx.calls().is_empty());
        assert_eq!(fx.saved(), vec![TimingSource::Lrclib]);
    }

    #[tokio::test]
    async fn without_the_setting_identification_still_retries_lrclib_only() {
        let (req, names) = snow_country();
        let fx = Fake { names, ..Fake::answering(Provider::Qq) };
        assert!(fetch_timed(&fx, &req, false).await.is_err());
        assert_eq!(fx.llm_calls(), 1);
        assert!(fx.calls().is_empty());
        assert!(fx.asked("track_name=%E9%9B%AA%E5%9B%BD") > 0);
    }

    #[test]
    fn source_names_drop_repeats_of_the_players() {
        let names = vec![Names { artist: "YOASOBI".into(), title: "夜に駆ける".into() }];
        let q = source_query("夜に駆ける", "YOASOBI", &names, 261.0);
        assert_eq!(q.names.len(), 1);
        let q = source_query("YOASOBI「夜に駆ける」 Official Music Video", "Ayase / YOASOBI", &[], 261.0);
        assert_eq!((q.names[0].title.as_str(), q.names[0].artist.as_str()), ("夜に駆ける", "Ayase / YOASOBI"));
    }

    #[tokio::test]
    async fn a_structured_hit_skips_full_text_and_llm() {
        let results = json!([record("Koi (Live)", "Minori", 300.0, true), record("Koi", "Minori", 266.0, true)]);
        let fx = Fake::default().rule("search?track_name=", json_reply(results));
        let (timed, _) = fetch_timed(&fx, &req(), false).await.unwrap();
        assert_eq!(timed.source, TimingSource::Lrclib);
        assert_eq!((fx.asked("q="), fx.llm_calls()), (0, 0));
        assert_eq!(fx.urls().len(), 2, "get, search");
    }

    #[test]
    fn best_match_weighs_name_and_length() {
        let want = Want { titles: vec!["Koi".into()], artists: vec!["Minori".into()], duration: Some(265.0) };
        let pick = |results: Vec<Value>| best_match(results, &want).map(|r| r["duration"].as_f64().unwrap());
        // Same name: the closer length wins, synced or not.
        assert_eq!(pick(vec![record("Koi", "Minori", 300.0, true), record("Koi", "Minori", 266.0, false)]), Some(266.0));
        // A cover of the right length loses to the song.
        assert_eq!(pick(vec![record("Koi (Cover)", "Minori", 265.0, true), record("Koi", "Minori", 270.0, true)]), Some(270.0));
        // Some other song is no match at all.
        assert_eq!(pick(vec![record("Hello", "Adele", 265.0, true)]), None);
        // The right song at a very different length (a video with an intro) still is.
        assert_eq!(pick(vec![record("Koi", "Minori", 330.0, true)]), Some(330.0));
    }

    #[tokio::test]
    async fn plain_only_answers_estimate_now_and_upgrades_later() {
        let fx = Fake::answering(Provider::Qq)
            .rule("artist_name=Minori%20Chihara", json_reply(json!([record("Koi", "Minori Chihara", 266.0, false)])))
            .rule("search?track_name=Koi&artist_name=Minori", json_reply(json!([record("Koi", "Minori Chihara", 266.0, false)])));
        let (timed, upgrade) = fetch_timed(&fx, &req(), true).await.unwrap();
        assert_eq!((timed.source, timed.pending, timed.lines.len()), (TimingSource::LrclibEstimate, true, 4));
        assert!(fx.calls().is_empty(), "nothing slow before answering");
        assert_eq!(fx.saved(), vec![TimingSource::LrclibEstimate], "cached: a replay asks LRCLIB nothing");
        assert_eq!(fx.urls().len(), 3, "get, search, same-recording search");

        let job = upgrade.unwrap();
        assert_eq!((job.query.names[0].title.as_str(), job.query.duration), ("Koi", 265.0), "LRCLIB's spelling, player's length");
        assert_eq!(job.query.names.len(), 1);
        let update = run_upgrade(&fx, &req(), job).await.unwrap();
        assert_eq!((update.title.as_str(), update.artist.as_str()), ("Koi", "Minori"), "keyed by what was asked");
        assert_eq!(update.source, TimingSource::Qq);
        assert_eq!(fx.calls(), vec![Some(4)], "sources get LRCLIB's lines");
        assert_eq!(fx.saved(), vec![TimingSource::LrclibEstimate, TimingSource::Qq], "real timing replaces it");
    }

    #[tokio::test]
    async fn a_correction_while_upgrading_drops_the_upgrade() {
        // Its own song: CORRECTIONS is shared by every test.
        let req = Request::new("Lantern Rain".into(), "Corrected Twice".into(), Some(200.0));
        let fx = Fake { cached_hit: Some((estimate_lines(), Some("lrclib-estimate".into()))), ..Fake::answering(Provider::Qq) };
        let (_, upgrade) = fetch_timed(&fx, &req, true).await.unwrap();
        correct(&req.artist, &req.title); // "Search again" or "Mark as instrumental" meanwhile
        assert!(run_upgrade(&fx, &req, upgrade.unwrap()).await.is_none(), "no stale event");
        assert!(fx.saved().is_empty(), "the correction stays in the cache");

        let (_, upgrade) = fetch_timed(&fx, &req, true).await.unwrap();
        assert!(run_upgrade(&fx, &req, upgrade.unwrap()).await.is_some(), "a job made after it runs");
    }

    #[tokio::test]
    async fn plain_only_upgrades_to_a_synced_entry_of_the_same_recording() {
        let fx = Fake::default()
            .rule("/get?", json_reply(record("Koi", "Minori Chihara", 266.0, false)))
            .rule("artist_name=Minori%20Chihara", json_reply(json!([record("Koi", "Minori Chihara", 265.5, true)])));
        let (timed, upgrade) = fetch_timed(&fx, &req(), true).await.unwrap();
        assert_eq!((timed.source, timed.pending), (TimingSource::Lrclib, false));
        assert!(upgrade.is_none());
        assert_eq!(fx.saved(), vec![TimingSource::Lrclib]);
    }

    #[tokio::test]
    async fn plain_only_without_the_setting_or_a_find_stays_estimated() {
        let plain = || json_reply(record("Koi", "Minori", 266.0, false));
        let fx = Fake::answering(Provider::Qq).rule("/get?", plain());
        let (timed, upgrade) = fetch_timed(&fx, &req(), false).await.unwrap();
        assert_eq!((timed.source, timed.pending), (TimingSource::LrclibEstimate, false));
        assert!(upgrade.is_none());

        let fx = Fake::default().rule("/get?", plain());
        let (_, upgrade) = fetch_timed(&fx, &req(), true).await.unwrap();
        assert!(run_upgrade(&fx, &req(), upgrade.unwrap()).await.is_none());
        assert_eq!(fx.saved(), vec![TimingSource::LrclibEstimate], "only the estimate");
    }

    #[tokio::test]
    async fn a_cached_estimate_answers_at_once_and_still_looks_for_timing() {
        let lines = estimate_lines();
        let fx = Fake { cached_hit: Some((lines, Some("lrclib-estimate".into()))), ..Fake::answering(Provider::Qq) };
        let (timed, upgrade) = fetch_timed(&fx, &req(), true).await.unwrap();
        assert_eq!((timed.source, timed.pending), (TimingSource::LrclibEstimate, true));
        assert!(fx.urls().is_empty(), "no LRCLIB");
        let update = run_upgrade(&fx, &req(), upgrade.unwrap()).await.unwrap();
        assert_eq!(update.source, TimingSource::Qq);

        let (_, upgrade) = fetch_timed(&fx, &req(), false).await.unwrap();
        assert!(upgrade.is_none(), "without the sources there is nothing to upgrade from");
    }

    #[tokio::test]
    async fn an_instrumental_by_the_llm_stops_before_the_sources_and_is_remembered() {
        let fx = Fake { llm_instrumental: true, ..Fake::answering(Provider::Qq) }.rule("search?", json_reply(json!([])));
        let (timed, upgrade) = fetch_timed(&fx, &req(), true).await.unwrap();
        assert_eq!((timed.source, timed.lines.len()), (TimingSource::Instrumental, 0));
        assert!(upgrade.is_none());
        assert!(fx.calls().is_empty(), "no sources");
        assert_eq!(fx.llm_calls(), 1);
        assert_eq!((*fx.saved_instrumental.lock().unwrap(), fx.saved_none()), (1, 0));

        let fx = Fake { cached_instrumental: true, ..Fake::answering(Provider::Qq) };
        let (timed, _) = fetch_timed(&fx, &req(), true).await.unwrap();
        assert_eq!(timed.source, TimingSource::Instrumental);
        assert!(fx.urls().is_empty() && fx.calls().is_empty() && fx.llm_calls() == 0, "a replay does nothing");
    }

    #[tokio::test]
    async fn lrclib_marking_the_exact_recording_instrumental_skips_the_llm() {
        let fx = Fake::answering(Provider::Qq)
            .rule("/get?", json_reply(json!({"instrumental": true, "syncedLyrics": null, "plainLyrics": null})));
        let (timed, _) = fetch_timed(&fx, &req(), true).await.unwrap();
        assert_eq!(timed.source, TimingSource::Instrumental);
        assert_eq!(fx.llm_calls(), 0);
        assert!(fx.calls().is_empty());
    }

    #[tokio::test]
    async fn search_again_doubts_an_instrumental_verdict_but_falls_back_to_it() {
        let (mut req, names) = snow_country();
        req.refetch = true;
        let fx = Fake { llm_instrumental: true, names, ..Default::default() }
            .rule("artist_name=%E3%83%A8", json_reply(json!([record("雪国", "ヨルシカ", 288.0, true)])))
            .rule("search?", json_reply(json!([])));
        let (timed, _) = fetch_timed(&fx, &req, true).await.unwrap();
        assert_eq!(timed.source, TimingSource::Lrclib, "lyrics found despite the verdict");

        let fx = Fake { llm_instrumental: true, ..Default::default() }.rule("search?", json_reply(json!([])));
        let (timed, _) = fetch_timed(&fx, &req, true).await.unwrap();
        assert_eq!(timed.source, TimingSource::Instrumental, "nothing found: the verdict stands");
        assert_eq!(fx.calls().len(), 1, "but every pass ran");
    }

    /// A "covered by" upload whose own recording no source has.
    fn a_cover() -> (Request, Names) {
        let req = Request::new("Bye My Sigh - Covered by Someone".into(), "Someone".into(), Some(299.0));
        (req, Names { title: "Bye My Sigh".into(), artist: "Original Band".into() })
    }

    #[tokio::test]
    async fn a_cover_sings_the_originals_words_on_its_own_timing() {
        let (req, original) = a_cover();
        // LRCLIB has the original at another length; nothing for the cover.
        let fx = Fake { llm_cover_of: Some(original), prealigned: Some(Served::Pending), ..Default::default() }
            .rule("artist_name=Original", json_reply(json!([record("Bye My Sigh", "Original Band", 240.0, true)])))
            .rule("search?", json_reply(json!([])));
        let (mut timed, upgrade) = fetch_timed(&fx, &req, false).await.unwrap();
        assert_eq!((timed.source, timed.lines.len()), (TimingSource::CoverEstimate, 3), "the synced text, timing dropped");
        assert!(timed.lines.last().unwrap().time < 299.0, "spread over the cover, not the original");
        assert!(upgrade.is_none(), "no source can time a cover");
        assert_eq!(fx.saved(), vec![TimingSource::CoverEstimate]);
        assert!(with_aligned(&fx, &req, &mut timed) || timed.pending);
        assert_eq!(fx.aligned_asked.lock().unwrap()[0].0, TimingSource::CoverEstimate, "word timing retimes it");
    }

    #[tokio::test]
    async fn the_originals_words_come_from_the_sources_at_any_length() {
        let (req, original) = a_cover();
        let fx = Fake { llm_cover_of: Some(original), ..Fake::answering(Provider::Netease) }
            .rule("search?", json_reply(json!([])));
        // The sources' normal same-length query misses; the words query is the second call.
        let fx = Fake { answer: None, ..fx };
        assert!(fetch_timed(&fx, &req, true).await.is_err());
        assert_eq!(fx.calls().len(), 2, "the song's own names, then the original's words");
    }

    #[tokio::test]
    async fn an_unmarked_cover_guess_is_not_trusted_without_web_search() {
        let (_, original) = a_cover();
        let req = Request::new("Bye My Sigh".into(), "Someone".into(), Some(299.0));
        let fx = Fake { llm_cover_of: Some(original), ..Default::default() }
            .rule("artist_name=Original", json_reply(json!([record("Bye My Sigh", "Original Band", 240.0, true)])))
            .rule("search?", json_reply(json!([])));
        assert!(fetch_timed(&fx, &req, false).await.is_err());
        assert_eq!(fx.asked("artist_name=Original"), 0, "another artist's words only when the cover is plain");
    }

    #[tokio::test]
    async fn nothing_anywhere_is_negative_cached_once() {
        let fx = Fake::default();
        assert!(fetch_timed(&fx, &req(), true).await.is_err());
        assert_eq!(fx.calls().len(), 1);
        assert_eq!(fx.saved_none(), 1);

        // Already negative-cached: retry the sources only; no LRCLIB, no LLM.
        let fx = Fake { negative_cached: true, ..Default::default() };
        assert!(fetch_timed(&fx, &req(), true).await.is_err());
        assert_eq!((fx.calls().len(), fx.saved_none()), (1, 0));
        assert!(fx.urls().is_empty());
        assert_eq!(fx.llm_calls(), 0);
    }

    #[tokio::test]
    async fn sources_stay_off_without_the_setting_or_a_length() {
        let fx = Fake::answering(Provider::Qq);
        assert!(fetch_timed(&fx, &req(), false).await.is_err());
        let no_length = Request::new("Koi".into(), "Minori".into(), None);
        assert!(fetch_timed(&fx, &no_length, true).await.is_err());
        assert!(fx.calls().is_empty());
        assert!(fx.saved().is_empty());
    }

    /// YouTube Music's answer: lines with times when `timed`.
    fn yt(texts: &[&str], timed: bool) -> YtLyrics {
        let lines = texts.iter().enumerate().map(|(i, t)| (timed.then_some(5.0 * (i + 1) as f64), t.to_string())).collect();
        YtLyrics { lines, timed, source: Some("Source: Test".into()) }
    }

    fn on_ytmusic(fx: Fake, lyrics: YtLyrics) -> (Fake, Request) {
        let mut req = req();
        req.video_id = Some("abcDEF12_-x".into());
        (Fake { yt: Some(lyrics), ..fx }, req)
    }

    #[tokio::test]
    async fn ytmusic_timed_lyrics_answer_before_lrclib() {
        let (fx, req) = on_ytmusic(Fake::answering(Provider::Qq), yt(&["one", "two", "three"], true));
        let (timed, upgrade) = fetch_timed(&fx, &req, true).await.unwrap();
        assert_eq!((timed.source, timed.pending), (TimingSource::Ytmusic, false));
        assert!(upgrade.is_none());
        let got: Vec<(f64, Option<f64>)> = timed.lines.iter().map(|l| (l.time, l.duration)).collect();
        assert_eq!(got, vec![(5.0, Some(5.0)), (10.0, Some(5.0)), (15.0, Some(3.0))]);
        assert!(fx.urls().is_empty(), "no LRCLIB request");
        assert!(fx.calls().is_empty());
        assert_eq!(fx.saved(), vec![TimingSource::Ytmusic]);
    }

    #[tokio::test]
    async fn ytmusic_plain_lines_take_lrclib_times() {
        let lines = ["One!", "an extra line", "two", "three"];
        let fx = Fake::default().rule("/get?", json_reply(record("Koi", "Minori", 265.0, true)));
        let (fx, req) = on_ytmusic(fx, yt(&lines, false));
        let (timed, upgrade) = fetch_timed(&fx, &req, true).await.unwrap();
        assert_eq!((timed.source, timed.pending), (TimingSource::Lrclib, false), "the timing is LRCLIB's");
        assert!(upgrade.is_none());
        let got: Vec<(f64, &str)> = timed.lines.iter().map(|l| (l.time, l.text.as_str())).collect();
        assert_eq!(got, vec![(10.0, "One!"), (15.0, "an extra line"), (20.0, "two"), (30.0, "three")], "YouTube Music's text");
        assert_eq!(fx.saved(), vec![TimingSource::Lrclib]);
        assert_eq!(fx.urls().len(), 1);
    }

    #[tokio::test]
    async fn ytmusic_plain_lines_without_lrclib_timing_are_estimated_then_upgraded() {
        // LRCLIB has the song, but its synced text is some other song's.
        let fx = Fake::answering(Provider::Netease).rule("/get?", json_reply(record("Koi", "Minori", 265.0, true)));
        let (fx, req) = on_ytmusic(fx, yt(&["alpha line", "beta line", "gamma line", "delta line"], false));
        let (timed, upgrade) = fetch_timed(&fx, &req, true).await.unwrap();
        assert_eq!((timed.source, timed.pending, timed.lines.len()), (TimingSource::YtmusicEstimate, true, 4));
        assert_eq!(timed.lines[0].text, "alpha line");
        assert_eq!(fx.saved(), vec![TimingSource::YtmusicEstimate]);
        assert_eq!(fx.saved_none(), 0);

        let update = run_upgrade(&fx, &req, upgrade.unwrap()).await.unwrap();
        assert_eq!(update.source, TimingSource::Netease);
        assert_eq!(fx.calls(), vec![Some(4)], "sources get YouTube Music's lines");

        // Without a length there is nothing for the sources to match: no upgrade.
        let (fx, mut req) = on_ytmusic(Fake::default(), yt(&["alpha line", "beta line"], false));
        req.duration = None;
        let (timed, upgrade) = fetch_timed(&fx, &req, true).await.unwrap();
        assert_eq!((timed.source, timed.pending), (TimingSource::YtmusicEstimate, false));
        assert!(upgrade.is_none());
    }

    #[tokio::test]
    async fn without_ytmusic_lyrics_the_pipeline_is_unchanged() {
        let exact = || json_reply(record("Koi", "Minori", 265.0, true));
        let mut req = req();
        req.video_id = Some("abcDEF12_-x".into());
        let fx = Fake::default().rule("/get?", exact());
        let (timed, _) = fetch_timed(&fx, &req, true).await.unwrap();
        assert_eq!(timed.source, TimingSource::Lrclib);
        assert_eq!((*fx.yt_calls.lock().unwrap(), fx.urls().len()), (1, 1));

        // Setting off, or no videoId: YouTube Music is never asked.
        let (fx, req) = on_ytmusic(Fake::default().rule("/get?", exact()), yt(&["one", "two", "three"], true));
        assert_eq!(fetch_timed(&fx, &req, false).await.unwrap().0.source, TimingSource::Lrclib);
        assert_eq!(fetch_timed(&fx, &self::req(), true).await.unwrap().0.source, TimingSource::Lrclib);
        assert_eq!(*fx.yt_calls.lock().unwrap(), 0);
    }

    #[test]
    fn contract_names() {
        let names: Vec<String> = [
            TimingSource::Lrclib,
            TimingSource::LrclibEstimate,
            TimingSource::Qq,
            TimingSource::Netease,
            TimingSource::Kugou,
            TimingSource::Ytmusic,
            TimingSource::YtmusicEstimate,
        ]
        .iter()
        .map(|s| {
            let json = serde_json::to_value(s).unwrap();
            assert_eq!(json, s.as_str());
            let estimate = matches!(s, TimingSource::LrclibEstimate | TimingSource::YtmusicEstimate);
            assert!(estimate || TimingSource::from_cache(Some(s.as_str())) == *s);
            s.as_str().to_string()
        })
        .collect();
        assert_eq!(names, ["lrclib", "lrclib-estimate", "qq", "netease", "kugou", "ytmusic", "ytmusic-estimate"]);
        assert_eq!(TimingSource::from_cache(None), TimingSource::Lrclib);
        let timed = TimedLyrics { lines: vec![], source: TimingSource::Qq, pending: true };
        assert_eq!(serde_json::to_value(timed).unwrap(), json!({"lines": [], "source": "qq", "pending": true}));
    }

    #[test]
    fn user_agent_follows_lrclib_format() {
        assert!(LRCLIB_USER_AGENT.starts_with("Lyryc v"));
        assert!(LRCLIB_USER_AGENT.ends_with("(https://github.com/Error0229/Lyryc)"));
    }

    /// Live LRCLIB (and, with a length, the extra sources), without the user's
    /// cache or LLM. `cargo test --lib live_lookup -- --ignored --nocapture`
    struct LiveNoCache(Mutex<Vec<String>>);

    impl Effects for LiveNoCache {
        fn cached(&self, _artist: &str, _track: &str) -> CacheResult {
            CacheResult::Miss
        }
        async fn lrclib(&self, url: &str) -> Reply {
            let reply = Live.lrclib(url).await;
            let status = match &reply {
                Reply::Json(_) => "ok".to_string(),
                Reply::NotFound => "404".to_string(),
                Reply::Failed(e) => e.clone(),
                Reply::Paused => "paused".to_string(),
            };
            println!("  {url} -> {status}");
            self.0.lock().unwrap().push(status);
            reply
        }
        async fn names(&self, _req: &Request, _web_search: bool) -> SongNames {
            SongNames::default()
        }
        fn sources(&self, q: &sources::Query, plain: Option<&[String]>) -> impl Future<Output = Option<Found>> + Send {
            sources::find(q, plain)
        }
        async fn ytmusic(&self, _video_id: &str) -> Option<YtLyrics> {
            None
        }
        fn save(&self, _artist: &str, _track: &str, _lines: &[LyricLine], _source: TimingSource) {}
        fn save_none(&self, _artist: &str, _track: &str) {}
        fn save_instrumental(&self, _artist: &str, _track: &str) {}
    }

    #[tokio::test]
    #[ignore = "network"]
    async fn live_lookup() {
        let songs = [
            ("SYNC", "Daoko", None),
            ("雪国 - Snow Country", "Yorushika", Some(288.0)),
            ("Mr. Brightside", "The Killers", Some(222.0)),
            ("Hype Boy", "NewJeans", Some(179.0)),
            ("YOASOBI「夜に駆ける」 Official Music Video", "Ayase / YOASOBI", Some(261.0)),
        ];
        for (title, artist, duration) in songs {
            println!("'{title}' by '{artist}' ({duration:?})");
            let fx = LiveNoCache(Mutex::new(Vec::new()));
            let started = std::time::Instant::now();
            let result = fetch_timed(&fx, &Request::new(title.into(), artist.into(), duration), true).await;
            let outcome = match result {
                Ok((t, _)) => format!("{} lines, {:?}, pending {}", t.lines.len(), t.source, t.pending),
                Err(e) => e,
            };
            let statuses = fx.0.lock().unwrap().clone();
            println!("  => {outcome}; {} LRCLIB request(s), {:.1}s
", statuses.len(), started.elapsed().as_secs_f64());
            assert!(!statuses.iter().any(|s| s.contains("429")), "rate-limited; stopping");
        }
    }

    fn aligned_lines() -> Vec<LyricLine> {
        let mut lines = parse_lrc_format(SYNCED);
        lines[0].words = Some(vec![crate::types::LyricWord { time: 10.1, text: "one".into(), duration: Some(0.4) }]);
        lines
    }

    #[tokio::test]
    async fn cached_alignment_replaces_the_timing_and_is_not_saved_as_lyrics() {
        let fx = Fake { prealigned: Some(Served::Hit(aligned_lines())), ..Fake::default() }
            .rule("/get?", json_reply(record("Koi", "Minori", 265.0, true)));
        let req = req();
        let (mut timed, _) = fetch_timed(&fx, &req, false).await.unwrap();
        assert!(with_aligned(&fx, &req, &mut timed));
        assert_eq!((timed.source, timed.pending), (TimingSource::Aligned, false));
        assert_eq!(timed.lines[0].words.as_ref().unwrap()[0].time, 10.1);
        assert_eq!(*fx.aligned_asked.lock().unwrap(), vec![(TimingSource::Lrclib, 3)], "asked with LRCLIB's lines");
        assert_eq!(fx.saved(), vec![TimingSource::Lrclib], "the lyrics cache keeps LRCLIB's timing");

        let json = serde_json::to_value(&timed).unwrap();
        assert_eq!(json["source"], "aligned");
        assert_eq!(json["lines"][0]["words"][0]["text"], "one");
        assert!(json["lines"][1].get("words").is_none(), "no words key without words");
    }

    #[tokio::test]
    async fn an_alignment_in_progress_marks_the_answer_pending() {
        let fx = Fake { prealigned: Some(Served::Pending), ..Fake::default() }
            .rule("/get?", json_reply(record("Koi", "Minori", 265.0, true)));
        let (mut timed, _) = fetch_timed(&fx, &req(), false).await.unwrap();
        assert!(!with_aligned(&fx, &req(), &mut timed));
        assert_eq!((timed.source, timed.pending), (TimingSource::Lrclib, true));
    }

    #[tokio::test]
    async fn without_the_aligner_nothing_changes() {
        let fx = Fake::default().rule("search?track_name=", json_reply(json!([record("Koi", "Minori", 265.0, false)])));
        let (mut timed, _) = fetch_timed(&fx, &req(), false).await.unwrap();
        let before = serde_json::to_value(&timed).unwrap();
        assert!(!with_aligned(&fx, &req(), &mut timed));
        assert_eq!(serde_json::to_value(&timed).unwrap(), before);
        assert_eq!(fx.aligned_asked.lock().unwrap()[0].0, TimingSource::LrclibEstimate, "estimates are flagged as such");
    }

    #[test]
    fn fetch_lyrics_payload_is_unchanged() {
        let lines = parse_lrc_format(SYNCED);
        let json = serde_json::to_value(&lines).unwrap();
        assert_eq!(json[0], json!({"time": 10.0, "text": "one", "duration": 10.0}));
        // Cached rows from before `words` existed still load.
        let old: Vec<LyricLine> = serde_json::from_str(r#"[{"time":1.0,"text":"a","duration":null}]"#).unwrap();
        assert_eq!(old[0].words, None);
    }

    #[test]
    fn parses_lrc() {
        let lines = parse_lrc_format("[00:02.50]b\n[00:01.00]a\n[00:03]\nnot a line");
        let got: Vec<(f64, &str, Option<f64>)> = lines.iter().map(|l| (l.time, l.text.as_str(), l.duration)).collect();
        assert_eq!(got, vec![(1.0, "a", Some(1.5)), (2.5, "b", Some(3.0))]);
    }
}
