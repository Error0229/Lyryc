//! YouTube Music's own lyrics (the player's "Lyrics" tab) for a videoId.
//!
//! A Rust port of the lyrics path of ytmusicapi (https://github.com/sigma67/ytmusicapi,
//! MIT): `get_watch_playlist` -> `next` for the lyrics tab's browseId, then
//! `get_lyrics(timestamps=True)` -> `browse` as the Android Music app, which is
//! the client that gets timed lines. This is YouTube Music's internal InnerTube
//! API, not a public one: no key, no cookies, and it may change without notice.
//! So: short timeouts, no retries, and a breaker that stops asking after
//! `TRIP_AFTER` consecutive failures.

mod parse;

pub use parse::{is_video_id, video_id_from_url};

use anyhow::{anyhow, Result};
use log::debug;
use reqwest::Client;
use serde_json::{json, Value};
use std::future::Future;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Lyrics for one track. `lines` are (start in seconds, text); every start is
/// Some exactly when `timed`.
#[derive(Clone, Debug, PartialEq)]
pub struct YtLyrics {
    pub lines: Vec<(Option<f64>, String)>,
    pub timed: bool,
    /// YouTube's credit line, e.g. "Source: LyricFind".
    pub source: Option<String>,
}

const API: &str = "https://music.youtube.com/youtubei/v1/";
const ORIGIN: &str = "https://music.youtube.com";
/// ytmusicapi's `constants.USER_AGENT`.
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:88.0) Gecko/20100101 Firefox/88.0";
/// ytmusicapi's `as_mobile()` client.
const ANDROID_MUSIC: (&str, &str) = ("ANDROID_MUSIC", "7.21.50");
const TIMEOUT: Duration = Duration::from_secs(4);
const TRIP_AFTER: u8 = 3;

/// The pattern of `sources::breaker`, for one endpoint: open after
/// `TRIP_AFTER` failures in a row, and then for the rest of the session.
struct Breaker(AtomicU8);

impl Breaker {
    const fn new() -> Self {
        Self(AtomicU8::new(0))
    }
    fn is_open(&self) -> bool {
        self.0.load(Ordering::Relaxed) >= TRIP_AFTER
    }
    fn record(&self, ok: bool) {
        if self.is_open() {
            return;
        }
        if ok {
            self.0.store(0, Ordering::Relaxed);
        } else {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }
}

static BREAKER: Breaker = Breaker::new();

/// YouTube Music's lyrics for `video_id`: timed when it has them, else plain.
/// Ok(None) when it has none; Err for network trouble, a changed response, or
/// a tripped breaker.
pub async fn lyrics(video_id: &str) -> Result<Option<YtLyrics>> {
    if !is_video_id(video_id) {
        return Err(anyhow!("not a videoId: {video_id:?}"));
    }
    guarded(&BREAKER, fetch(video_id)).await
}

async fn guarded(b: &Breaker, f: impl Future<Output = Result<Option<YtLyrics>>>) -> Result<Option<YtLyrics>> {
    if b.is_open() {
        return Err(anyhow!("YouTube Music lyrics are off for this session after {TRIP_AFTER} failures"));
    }
    let r = f.await;
    b.record(r.is_ok());
    r
}

/// `next`, then `browse` as the mobile app; the web client's `browse` only if
/// the mobile answer had an unknown shape.
async fn fetch(video_id: &str) -> Result<Option<YtLyrics>> {
    let next = post("next", json!({"videoId": video_id, "isAudioOnly": true}), web_client()).await?;
    let Some(browse_id) = parse::browse_id(&next)? else {
        return Ok(None);
    };
    let body = json!({"browseId": browse_id});
    match parse::lyrics(&post("browse", body.clone(), ANDROID_MUSIC).await?) {
        Err(e) => debug!("YouTube Music mobile lyrics for {video_id}: {e}; trying the web client"),
        found => return found,
    }
    parse::lyrics(&post("browse", body, web_client()).await?)
}

/// ytmusicapi's `initialize_context`: `WEB_REMIX` at today's version.
fn web_client() -> (&'static str, &'static str) {
    static VERSION: OnceLock<String> = OnceLock::new();
    let version = VERSION.get_or_init(|| {
        let days = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() / 86_400);
        let (y, m, d) = civil_from_days(days as i64);
        format!("1.{y:04}{m:02}{d:02}.01.00")
    });
    ("WEB_REMIX", version)
}

/// Days since 1970-01-01 to (year, month, day), proleptic Gregorian
/// (Howard Hinnant's `civil_from_days`).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

fn client() -> &'static Client {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        Client::builder()
            .user_agent(USER_AGENT)
            .timeout(TIMEOUT)
            .connect_timeout(TIMEOUT)
            .build()
            .expect("Failed to create YouTube Music HTTP client")
    })
}

/// ytmusicapi's `_send_request`: POST JSON with the client context.
async fn post(endpoint: &str, mut body: Value, (name, version): (&str, &str)) -> Result<Value> {
    body["context"] = json!({"client": {"clientName": name, "clientVersion": version, "hl": "en"}, "user": {}});
    let resp = client()
        .post(format!("{API}{endpoint}?alt=json"))
        .header(reqwest::header::ORIGIN, ORIGIN)
        .json(&body)
        .send()
        .await?;
    Ok(resp.error_for_status()?.json().await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn breaker_trips_after_three_failures_and_stays_open() {
        let b = Breaker::new();
        let fail = || async { Err(anyhow!("boom")) };
        let none = || async { Ok(None) };
        for _ in 0..2 {
            assert!(guarded(&b, fail()).await.is_err());
        }
        assert!(guarded(&b, none()).await.unwrap().is_none(), "\"no lyrics\" is an answer");
        for _ in 0..3 {
            assert!(guarded(&b, fail()).await.is_err());
        }
        assert!(b.is_open());
        let asked = std::sync::atomic::AtomicBool::new(false);
        let r = guarded(&b, async {
            asked.store(true, Ordering::Relaxed);
            Ok(None)
        })
        .await;
        assert!(r.is_err() && !asked.load(Ordering::Relaxed), "open: not asked at all");
    }

    #[tokio::test]
    async fn rejects_what_is_not_a_video_id_without_a_request() {
        assert!(lyrics("../browse").await.is_err());
        assert!(!BREAKER.is_open());
    }

    #[test]
    fn web_client_version_is_dated() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_782), (2024, 2, 29));
        assert_eq!(civil_from_days(20_759), (2026, 11, 2));
        let (_, v) = web_client();
        assert!(v.starts_with("1.20") && v.ends_with(".01.00") && v.len() == 16, "{v}");
    }

    /// `cargo test --lib ytmusic_live -- --ignored --nocapture`. Prints counts
    /// and metadata only, never lyric text.
    #[tokio::test]
    #[ignore = "network"]
    async fn ytmusic_live() {
        for id in std::env::var("YT_IDS").unwrap_or_default().split(',').filter(|s| !s.is_empty()) {
            let started = std::time::Instant::now();
            let r = fetch(id).await;
            let ms = started.elapsed().as_millis();
            match r {
                Ok(Some(l)) => println!("{id}: found, timed {}, {} lines, {:?}, {ms} ms", l.timed, l.lines.len(), l.source),
                Ok(None) => println!("{id}: none, {ms} ms"),
                Err(e) => println!("{id}: error {e}, {ms} ms"),
            }
            tokio::time::sleep(Duration::from_millis(800)).await;
        }
    }
}
