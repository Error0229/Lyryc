//! Lyrics lookup: a short, sequential conversation with LRCLIB, then the
//! opt-in extra sources, then one LLM-assisted retry.
//!
//! 1. Cache.
//! 2. `/api/get` with the player's title, then with the regex-cleaned title.
//! 3. One structured `/api/search` for the cleaned title; one full-text `q=`
//!    search only if nothing so far found the song.
//! 4. Extra sources (QQ / NetEase / Kugou), when enabled: also when LRCLIB failed.
//! 5. One LLM call for canonical names, then `/api/get` + search with them.
//!
//! At most `MAX_REQUESTS` LRCLIB requests per song, one at a time, and no URL
//! twice. A plain-only hit spends one of them looking for a synced entry of the
//! same recording.

use crate::cleaning::{clean_title, normalize_name, Names};
use crate::llm;
use crate::lyrics_cache::{cache_lyrics, cache_no_lyrics, get_cached_lyrics, CacheResult};
use crate::sources::{self, Found, Provider};
use crate::timing;
use crate::types::LyricLine;
use anyhow::Result;
use log::{debug, info, warn};
use regex::Regex;
use reqwest::{Client, StatusCode};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
use std::sync::{LazyLock, OnceLock};
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
        let wait = retry_after(&resp).unwrap_or(Duration::from_secs(1));
        if !busy || attempt == LRCLIB_MAX_RETRIES || wait > LRCLIB_MAX_RETRY_WAIT {
            return Ok(resp);
        }
        attempt += 1;
        debug!("LRCLIB answered {} for {}; retrying in {:?}", status, url, wait);
        tokio::time::sleep(with_jitter(wait)).await;
    }
}

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
    /// Plain lyrics only, with an estimate (never cached).
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
}

impl<E: Effects> Lookup<'_, E> {
    async fn ask(&mut self, url: String) -> Option<Value> {
        if let Some(answer) = self.asked.get(&url) {
            return answer.clone();
        }
        if self.asked.len() >= MAX_REQUESTS {
            debug!("LRCLIB budget spent; skipping {}", url);
            return None;
        }
        let answer = match self.fx.lrclib(&url).await {
            Reply::Json(v) => Some(v),
            Reply::NotFound => None,
            Reply::Failed(e) => {
                warn!("LRCLIB gave no answer for {}: {}", url, e);
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
        self.ask(url).await.filter(has_lyrics)
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
        info!("No synced timing for '{}'; returning estimated timing (not cached)", src.track);
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

    /// Step 5: one LLM call, then an exact lookup and a search with its names.
    async fn llm_pass(&mut self) -> Option<Lrclib> {
        let names = self.fx.names(self.req).await?;
        self.find(&[names.title], &names.artist, false).await
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
        }
    }

    /// Cache rows from before sources were recorded are LRCLIB.
    fn from_cache(name: Option<&str>) -> Self {
        match name {
            Some("qq") => TimingSource::Qq,
            Some("netease") => TimingSource::Netease,
            Some("kugou") => TimingSource::Kugou,
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
}

impl Request {
    fn new(title: String, artist: String, duration: Option<f64>) -> Self {
        let duration = duration.filter(|d| (1.0..=3600.0).contains(d));
        Self { title, artist, duration }
    }
}

/// A background lookup that may replace an estimate with real timing.
#[derive(Debug)]
struct Upgrade {
    query: sources::Query,
    plain: Vec<String>,
}

/// Everything the lookup does to the outside world, behind a trait so tests
/// run it offline and count its requests.
trait Effects: Send + Sync {
    fn cached(&self, artist: &str, track: &str) -> CacheResult;
    /// Send one LRCLIB request.
    fn lrclib(&self, url: &str) -> impl Future<Output = Reply> + Send;
    /// The LLM's names for the song; None when it is not set up, failed or is unsure.
    fn names(&self, req: &Request) -> impl Future<Output = Option<Names>> + Send;
    fn sources(&self, q: &sources::Query, plain: Option<&[String]>) -> impl Future<Output = Option<Found>> + Send;
    fn save(&self, artist: &str, track: &str, lines: &[LyricLine], source: TimingSource);
    fn save_none(&self, artist: &str, track: &str);
}

struct Live;

impl Effects for Live {
    fn cached(&self, artist: &str, track: &str) -> CacheResult {
        get_cached_lyrics(artist, track)
    }
    async fn lrclib(&self, url: &str) -> Reply {
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
    /// Song identification as measured in docs/research/llm-search (search off):
    /// the release's own names, e.g. "Snow Country" / Yorushika -> 雪国 / ヨルシカ.
    async fn names(&self, req: &Request) -> Option<Names> {
        let id = llm::identify::identify_song(&req.title, &req.artist, req.duration, false).await?;
        info!(
            "Identified '{}' by '{}' as '{}' by '{}'",
            req.title, req.artist, id.title_native, id.artist_native
        );
        Some(Names { artist: id.artist_native, title: id.title_native })
    }
    fn sources(&self, q: &sources::Query, plain: Option<&[String]>) -> impl Future<Output = Option<Found>> + Send {
        sources::find(q, plain)
    }
    fn save(&self, artist: &str, track: &str, lines: &[LyricLine], source: TimingSource) {
        cache_lyrics(artist, track, lines, source.as_str());
    }
    fn save_none(&self, artist: &str, track: &str) {
        cache_no_lyrics(artist, track);
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
pub async fn fetch_lyrics_timed(
    app: tauri::AppHandle,
    track_name: String,
    artist_name: String,
    duration: Option<f64>,
    extra_sources: bool,
) -> Result<TimedLyrics, String> {
    let req = Request::new(track_name, artist_name, duration);
    let (timed, upgrade) = fetch_timed(&Live, &req, extra_sources).await?;
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

async fn fetch_timed<E: Effects>(
    fx: &E,
    req: &Request,
    extra: bool,
) -> Result<(TimedLyrics, Option<Upgrade>), String> {
    let negative = match fx.cached(&req.artist, &req.title) {
        CacheResult::Hit(lines, source) => {
            info!("Using cached lyrics for: {} by {}", req.title, req.artist);
            let source = TimingSource::from_cache(source.as_deref());
            return Ok((TimedLyrics { lines, source, pending: false }, None));
        }
        CacheResult::NegativeHit => true,
        CacheResult::Miss => false,
    };

    let started = std::time::Instant::now();
    let mut run = Lookup { fx, req, asked: HashMap::new(), incomplete: false };
    let mut found = if negative { None } else { run.regex_pass().await };
    if found.is_none() {
        if let Some(timed) = from_sources(fx, req, extra).await {
            return Ok((timed, None));
        }
        if !negative {
            found = run.llm_pass().await;
        }
    }
    info!(
        "Lyrics lookup for '{}' by '{}': {} after {} LRCLIB request(s) in {:.1}s",
        req.title,
        req.artist,
        match &found {
            Some(Lrclib::Synced(_)) => "synced",
            Some(Lrclib::Plain { .. }) => "plain only",
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
            let upgrade = if extra { upgrade_job(req, &estimate, &src) } else { None };
            let pending = upgrade.is_some();
            Ok((TimedLyrics { lines: estimate, source: TimingSource::LrclibEstimate, pending }, upgrade))
        }
        None if negative => Err("No lyrics found (cached)".to_string()),
        None if run.incomplete => Err("Lyrics service unavailable, try again later".to_string()),
        None => {
            fx.save_none(&req.artist, &req.title);
            Err("No lyrics found".to_string())
        }
    }
}

/// Step 4: with the setting on and a length to match recordings by, the extra
/// sources, whether LRCLIB had nothing or could not answer. A hit is cached.
async fn from_sources<E: Effects>(fx: &E, req: &Request, extra: bool) -> Option<TimedLyrics> {
    let duration = req.duration.filter(|_| extra)?;
    let track = clean_title(&req.title, &req.artist);
    let q = sources::Query { track, artist: req.artist.clone(), duration };
    let found = fx.sources(&q, None).await?;
    let source = TimingSource::from(found.provider);
    info!("LRCLIB had nothing for '{}'; {} did", req.title, source.as_str());
    fx.save(&req.artist, &req.title, &found.lines, source);
    Some(TimedLyrics { lines: found.lines, source, pending: false })
}

/// Look up LRCLIB's spelling of the song, for the length being played.
fn upgrade_job(req: &Request, estimate: &[LyricLine], src: &PlainSource) -> Option<Upgrade> {
    let duration = req.duration.or((src.duration > 0.0).then_some(src.duration))?;
    Some(Upgrade {
        query: sources::Query { track: src.track.clone(), artist: src.artist.clone(), duration },
        plain: estimate.iter().map(|l| l.text.clone()).collect(),
    })
}

/// Real timing for an estimate, cached, as the event to send.
async fn run_upgrade<E: Effects>(fx: &E, req: &Request, job: Upgrade) -> Option<TimingUpdate> {
    let found = fx.sources(&job.query, Some(&job.plain)).await?;
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
            })
        })
        .collect();

    lyrics.sort_by(|a, b| a.time.total_cmp(&b.time));
    for i in 0..lyrics.len().saturating_sub(1) {
        lyrics[i].duration = Some(lyrics[i + 1].time - lyrics[i].time);
    }
    // The last line gets 3 seconds.
    if let Some(last) = lyrics.last_mut() {
        last.duration = Some(3.0);
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
        /// The first rule whose text is in the URL answers; otherwise 404.
        lrclib: Vec<(&'static str, Reply)>,
        names: Option<Names>,
        answer: Option<Found>,
        urls: Mutex<Vec<String>>,
        llm_calls: Mutex<u32>,
        /// One entry per sources call: how many known lines it was given.
        calls: Mutex<Vec<Option<usize>>>,
        saved: Mutex<Vec<TimingSource>>,
        saved_none: Mutex<u32>,
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
            if self.negative_cached { CacheResult::NegativeHit } else { CacheResult::Miss }
        }
        fn lrclib(&self, url: &str) -> impl Future<Output = Reply> + Send {
            self.urls.lock().unwrap().push(url.to_string());
            let reply = self.lrclib.iter().find(|(n, _)| url.contains(n)).map_or(Reply::NotFound, |(_, r)| r.clone());
            async move { reply }
        }
        fn names(&self, _req: &Request) -> impl Future<Output = Option<Names>> + Send {
            *self.llm_calls.lock().unwrap() += 1;
            let names = self.names.clone();
            async move { names }
        }
        fn sources(&self, _q: &sources::Query, plain: Option<&[String]>) -> impl Future<Output = Option<Found>> + Send {
            self.calls.lock().unwrap().push(plain.map(<[String]>::len));
            let answer = self.answer.clone();
            async move { answer }
        }
        fn save(&self, _artist: &str, _track: &str, _lines: &[LyricLine], source: TimingSource) {
            self.saved.lock().unwrap().push(source);
        }
        fn save_none(&self, _artist: &str, _track: &str) {
            *self.saved_none.lock().unwrap() += 1;
        }
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
        assert_eq!(fx.llm_calls(), 0, "the sources answered first");

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
        let fx = Fake { names: Some(names), ..Default::default() }.rule("search?", json_reply(json!([])));
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
        let fx = Fake { names: Some(Names { artist: "なとり".into(), title: "Overdose".into() }), ..Default::default() }
            .rule("artist_name=%E3%81%AA%E3%81%A8%E3%82%8A", json_reply(json!([record("Overdose", "なとり", 266.0, true)])))
            .rule("search?", json_reply(json!([])));
        let req = Request::new("Overdose".into(), "Natori".into(), None);
        let (timed, _) = fetch_timed(&fx, &req, false).await.unwrap();
        assert_eq!(timed.source, TimingSource::Lrclib);
        assert_eq!(fx.asked("/get?"), 0, "no length, no exact lookups");
        assert_eq!(fx.urls().len(), 3, "search, q, LLM search");
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
        assert!(fx.saved().is_empty(), "estimates are not cached");
        assert_eq!(fx.urls().len(), 3, "get, search, same-recording search");

        let job = upgrade.unwrap();
        assert_eq!((job.query.track.as_str(), job.query.duration), ("Koi", 265.0), "LRCLIB's spelling, player's length");
        let update = run_upgrade(&fx, &req(), job).await.unwrap();
        assert_eq!((update.title.as_str(), update.artist.as_str()), ("Koi", "Minori"), "keyed by what was asked");
        assert_eq!(update.source, TimingSource::Qq);
        assert_eq!(fx.calls(), vec![Some(4)], "sources get LRCLIB's lines");
        assert_eq!(fx.saved(), vec![TimingSource::Qq]);
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
        assert!(fx.saved().is_empty());
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

    #[test]
    fn contract_names() {
        let names: Vec<String> = [
            TimingSource::Lrclib,
            TimingSource::LrclibEstimate,
            TimingSource::Qq,
            TimingSource::Netease,
            TimingSource::Kugou,
        ]
        .iter()
        .map(|s| {
            let json = serde_json::to_value(s).unwrap();
            assert_eq!(json, s.as_str());
            assert!(*s == TimingSource::LrclibEstimate || TimingSource::from_cache(Some(s.as_str())) == *s);
            s.as_str().to_string()
        })
        .collect();
        assert_eq!(names, ["lrclib", "lrclib-estimate", "qq", "netease", "kugou"]);
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
            };
            println!("  {url} -> {status}");
            self.0.lock().unwrap().push(status);
            reply
        }
        async fn names(&self, _req: &Request) -> Option<Names> {
            None
        }
        fn sources(&self, q: &sources::Query, plain: Option<&[String]>) -> impl Future<Output = Option<Found>> + Send {
            sources::find(q, plain)
        }
        fn save(&self, _artist: &str, _track: &str, _lines: &[LyricLine], _source: TimingSource) {}
        fn save_none(&self, _artist: &str, _track: &str) {}
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

    #[test]
    fn parses_lrc() {
        let lines = parse_lrc_format("[00:02.50]b\n[00:01.00]a\n[00:03]\nnot a line");
        let got: Vec<(f64, &str, Option<f64>)> = lines.iter().map(|l| (l.time, l.text.as_str(), l.duration)).collect();
        assert_eq!(got, vec![(1.0, "a", Some(1.5)), (2.5, "b", Some(3.0))]);
    }
}
