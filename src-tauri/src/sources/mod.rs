//! Opt-in synced lyrics from QQ Music, NetEase and Kugou.
//!
//! These are unofficial, reverse-engineered endpoints (the same ones Lyricify,
//! syncedlyrics and LyricsX use). They are only called when the user turned on
//! "Extra lyric sources", and every provider sits behind a circuit breaker, so
//! one that breaks or starts blocking stops costing anything.
//! Benchmark and endpoint survey: docs/research/timing/REPORT.md.
//!
//! A wrong song is worse than no timing, so every result has to pass guards:
//! a candidate of the same length (+-3 s), at least `MIN_LINES` timed lines,
//! and either >= 50% of the known lines text-matched or, with no known text, a
//! matching title.

mod align;
mod breaker;
mod kugou;
mod netease;
mod qq;

use crate::types::LyricLine;
use anyhow::Result;
use breaker::Breaker;
use log::debug;
use regex::Regex;
use reqwest::Client;
use serde::Serialize;
use serde_json::Value;
use std::future::Future;
use std::sync::OnceLock;
use std::time::Duration;

/// In order of preference (QQ had the best coverage and accuracy).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Qq,
    Netease,
    Kugou,
}

impl Provider {
    pub const ALL: [Provider; 3] = [Provider::Qq, Provider::Netease, Provider::Kugou];

    fn api(self) -> &'static Api {
        match self {
            Provider::Qq => &qq::API,
            Provider::Netease => &netease::API,
            Provider::Kugou => &kugou::API,
        }
    }
}

/// What to look for. `duration` is the length of the recording being played.
#[derive(Clone, Debug)]
pub struct Query {
    pub track: String,
    pub artist: String,
    pub duration: f64,
}

/// Timing a provider found that passed the guards.
#[derive(Clone, Debug)]
pub struct Found {
    pub provider: Provider,
    pub lines: Vec<LyricLine>,
}

/// One search result of a provider.
#[derive(Clone, Debug, PartialEq)]
struct Candidate {
    id: String,
    title: String,
    artist: String,
    duration: f64,
    /// Kugou's download key; empty elsewhere.
    key: String,
}

/// A provider's LRC for the candidate it picked.
#[derive(Clone, Debug)]
struct Hit {
    title: String,
    lrc: String,
}

/// A provider is two requests, search then lyric, described by these parsers.
/// Parsers return Err when the response no longer has the expected shape, which
/// counts as a failure for the breaker; Ok(empty) is a normal "not found".
struct Api {
    referer: Option<&'static str>,
    search_url: fn(&Query) -> String,
    candidates: fn(&Value) -> Result<Vec<Candidate>>,
    lyric_url: fn(&Candidate) -> String,
    lyric: fn(&Value) -> Result<Option<String>>,
}

const TIMEOUT: Duration = Duration::from_secs(3);
const SAME_RECORDING_SECS: f64 = 3.0;
const MIN_LINES: usize = 3;
/// Share of the known lines that must find a partner in the source text.
const MIN_MATCHED: f64 = 0.5;
/// Title similarity needed when there is no text to check against.
const MIN_TITLE_SIM: f64 = 0.6;
/// What `parse_lrc_format` gives the last line.
const LAST_LINE_SECS: f64 = 3.0;

static BREAKER: Breaker = Breaker::new();

fn client() -> &'static Client {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        Client::builder()
            // These endpoints serve the desktop players; a browser UA is what they expect.
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
            .timeout(TIMEOUT)
            .connect_timeout(TIMEOUT)
            .build()
            .expect("Failed to create sources HTTP client")
    })
}

async fn get_json(url: &str, referer: Option<&str>) -> Result<Value> {
    let mut req = client().get(url);
    if let Some(r) = referer {
        req = req.header(reqwest::header::REFERER, r);
    }
    Ok(req.send().await?.error_for_status()?.json().await?)
}

/// One provider, one search and at most one lyric request. No retries.
async fn fetch(p: Provider, q: &Query) -> Result<Option<Hit>> {
    let api = p.api();
    let found = get_json(&(api.search_url)(q), api.referer).await?;
    let Some(c) = best((api.candidates)(&found)?, q) else {
        return Ok(None);
    };
    let body = get_json(&(api.lyric_url)(&c), api.referer).await?;
    Ok((api.lyric)(&body)?.map(|lrc| Hit {
        title: c.title,
        lrc,
    }))
}

/// Timing for `q` from the first provider that has some. With `plain`, the
/// result keeps those lines and takes only times from the source; without it,
/// the source's own lines are used. Requests run one after another.
pub async fn find(q: &Query, plain: Option<&[String]>) -> Option<Found> {
    find_with(&BREAKER, q, plain, |p| fetch(p, q)).await
}

async fn find_with<F, Fut>(
    breaker: &Breaker,
    q: &Query,
    plain: Option<&[String]>,
    fetch: F,
) -> Option<Found>
where
    F: Fn(Provider) -> Fut,
    Fut: Future<Output = Result<Option<Hit>>>,
{
    for p in Provider::ALL {
        if breaker.is_open(p) {
            continue;
        }
        let hit = fetch(p).await;
        breaker.record(p, hit.is_ok());
        match hit {
            Err(e) => debug!("{:?} failed for '{}' by '{}': {}", p, q.track, q.artist, e),
            Ok(None) => debug!("{:?}: no same-length match for '{}'", p, q.track),
            Ok(Some(hit)) => match accept(&hit, q, plain) {
                Some(lines) => return Some(Found { provider: p, lines }),
                None => debug!(
                    "{:?}: '{}' failed the guards for '{}'",
                    p, hit.title, q.track
                ),
            },
        }
    }
    None
}

/// The guards. None means "don't trust this", never "partially trust it".
fn accept(hit: &Hit, q: &Query, plain: Option<&[String]>) -> Option<Vec<LyricLine>> {
    let src = align::parse_lrc(&hit.lrc, &q.track, &q.artist);
    let last = src.last()?.0;
    if src.len() < MIN_LINES || last > q.duration + SAME_RECORDING_SECS {
        return None;
    }
    let Some(plain) = plain else {
        return title_matches(&hit.title, &q.track).then(|| to_lines(src));
    };
    let mapped = align::map_lines(plain, &src);
    let matched = mapped.iter().flatten().count();
    if (matched as f64) < MIN_MATCHED * plain.len() as f64 {
        return None;
    }
    let times = align::interpolate(&mapped, q.duration.max(last));
    Some(to_lines(
        times.into_iter().zip(plain.iter().cloned()).collect(),
    ))
}

fn to_lines(timed: Vec<(f64, String)>) -> Vec<LyricLine> {
    let ends: Vec<Option<f64>> = timed
        .iter()
        .skip(1)
        .map(|(t, _)| Some(*t))
        .chain([None])
        .collect();
    timed
        .into_iter()
        .zip(ends)
        .map(|((time, text), end)| LyricLine {
            time,
            text,
            duration: Some(end.map_or(LAST_LINE_SECS, |e| (e - time).max(0.0))),
        })
        .collect()
}

/// With no lyric text to compare, the title has to match instead.
fn title_matches(candidate: &str, track: &str) -> bool {
    let (c, t) = (align::norm(candidate), align::norm(track));
    !c.is_empty()
        && !t.is_empty()
        && (c.starts_with(&t) || t.starts_with(&c) || align::dice(&c, &t) >= MIN_TITLE_SIM)
}

/// Share of distinct characters two names have in common.
fn char_sim(a: &str, b: &str) -> f64 {
    use std::collections::HashSet;
    let a: HashSet<char> = align::norm(a).chars().collect();
    let b: HashSet<char> = align::norm(b).chars().collect();
    a.intersection(&b).count() as f64 / a.union(&b).count().max(1) as f64
}

fn cover_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)cover|翻唱|女声|男声|伴奏|instrumental|karaoke|remix|live|dj").unwrap()
    })
}

/// Same length first, then the closest title and artist, with covers penalised.
fn best(cands: Vec<Candidate>, q: &Query) -> Option<Candidate> {
    let score = |c: &Candidate| {
        char_sim(&c.title, &q.track) + char_sim(&c.artist, &q.artist)
            - (c.duration - q.duration).abs() / 30.0
            - if cover_re().is_match(&c.title) {
                0.5
            } else {
                0.0
            }
    };
    cands
        .into_iter()
        .filter(|c| c.duration > 0.0 && (c.duration - q.duration).abs() <= SAME_RECORDING_SECS)
        .map(|c| (score(&c), c))
        .reduce(|a, b| if b.0 > a.0 { b } else { a })
        .map(|(_, c)| c)
}

fn str_of(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use anyhow::anyhow;
    use std::sync::Mutex;

    pub fn fixture(name: &str) -> Value {
        let path = format!(
            "{}/src/sources/fixtures/{}",
            env!("CARGO_MANIFEST_DIR"),
            name
        );
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    pub fn koi() -> Query {
        Query {
            track: "恋".into(),
            artist: "茅原実里".into(),
            duration: 265.0,
        }
    }

    /// LRCLIB-style plain text for the fixture song: the NetEase lines, minus
    /// timing, with one line changed and one added, as a different text source
    /// would have them.
    pub fn koi_plain() -> Vec<String> {
        let lrc = netease::API.lyric;
        let text = lrc(&fixture("netease_lyric.json")).unwrap().unwrap();
        let mut lines: Vec<String> = align::parse_lrc(&text, "恋", "茅原実里")
            .into_iter()
            .map(|(_, t)| t)
            .collect();
        lines[2] = "ソワソワしてる".into();
        lines.insert(5, "(spoken intro that no source has)".into());
        lines
    }

    fn cand(title: &str, artist: &str, duration: f64) -> Candidate {
        Candidate {
            id: title.into(),
            title: title.into(),
            artist: artist.into(),
            duration,
            key: String::new(),
        }
    }

    fn lrc(lines: &[(f64, &str)]) -> String {
        lines
            .iter()
            .map(|(t, s)| format!("[{:02}:{:05.2}]{}\n", (*t as u32) / 60, t % 60.0, s))
            .collect()
    }

    #[test]
    fn best_needs_same_length_then_prefers_originals() {
        let q = Query {
            track: "Tainted Love".into(),
            artist: "Soft Cell".into(),
            duration: 183.0,
        };
        let cands = vec![
            cand("Tainted Love", "Soft Cell", 200.0),
            cand("Tainted Love (Cover)", "Soft Cell", 183.0),
            cand("Tainted Love", "Soft Cell", 184.5),
        ];
        assert_eq!(best(cands, &q).unwrap().duration, 184.5);
        assert!(best(vec![cand("Tainted Love", "Soft Cell", 190.0)], &q).is_none());
        assert!(best(vec![cand("Tainted Love", "Soft Cell", 0.0)], &q).is_none());
    }

    #[test]
    fn guards_keep_lrclib_lines_and_take_source_times() {
        let q = koi();
        let plain = koi_plain();
        let text = (netease::API.lyric)(&fixture("netease_lyric.json"))
            .unwrap()
            .unwrap();
        let lines = accept(
            &Hit {
                title: "恋".into(),
                lrc: text,
            },
            &q,
            Some(&plain),
        )
        .unwrap();
        assert_eq!(
            lines.iter().map(|l| l.text.clone()).collect::<Vec<_>>(),
            plain
        );
        assert_eq!(lines[0].time, 24.45);
        assert_eq!(
            lines[2].time, 34.45,
            "a slightly different line still pairs"
        );
        assert!(
            lines[5].time > lines[4].time && lines[5].time < lines[6].time,
            "unpaired line is interpolated"
        );
        assert!(lines.windows(2).all(|w| w[0].time <= w[1].time));
        assert_eq!(lines[0].duration, Some(lines[1].time - lines[0].time));
    }

    #[test]
    fn guards_reject_wrong_song_short_files_and_wrong_length() {
        let q = Query {
            track: "Song".into(),
            artist: "Band".into(),
            duration: 200.0,
        };
        let plain: Vec<String> = [
            "one two three",
            "four five six",
            "seven eight nine",
            "ten eleven twelve",
        ]
        .map(String::from)
        .to_vec();
        let hit = |lines: &[(f64, &str)]| Hit {
            title: "Song".into(),
            lrc: lrc(lines),
        };

        let good = hit(&[
            (10.0, "one two three"),
            (20.0, "four five six"),
            (30.0, "seven eight nine"),
        ]);
        assert!(accept(&good, &q, Some(&plain)).is_some());

        // Fewer than 3 timed lines.
        let short = hit(&[(10.0, "one two three"), (20.0, "four five six")]);
        assert!(accept(&short, &q, Some(&plain)).is_none());

        // Under half of the known lines match: a different song.
        let other = hit(&[
            (10.0, "one two three"),
            (20.0, "la la la"),
            (30.0, "na na na"),
            (40.0, "oh oh"),
        ]);
        assert!(accept(&other, &q, Some(&plain)).is_none());

        // Timestamps past the end of the track: a longer recording.
        let long = hit(&[
            (10.0, "one two three"),
            (20.0, "four five six"),
            (250.0, "seven eight nine"),
        ]);
        assert!(accept(&long, &q, Some(&plain)).is_none());
    }

    #[test]
    fn without_known_text_the_title_must_match() {
        let q = koi();
        let text = (qq::API.lyric)(&fixture("qq_lyric.json")).unwrap().unwrap();
        let lines = accept(
            &Hit {
                title: "恋 (Love)".into(),
                lrc: text.clone(),
            },
            &q,
            None,
        )
        .unwrap();
        assert_eq!(
            lines[0].text, "眠れない夜 脳裏をよぎる",
            "credits and the title banner are dropped"
        );
        assert!(lines.len() > 30);
        assert!(accept(
            &Hit {
                title: "失恋无罪".into(),
                lrc: text
            },
            &q,
            None
        )
        .is_none());
    }

    #[test]
    fn title_match_is_prefix_or_close() {
        assert!(title_matches("Tainted Love (Remastered)", "Tainted Love"));
        assert!(title_matches("恋 (Love)", "恋"));
        assert!(title_matches(
            "Bohemian Rhapsody",
            "Bohemian Rhapsody - Remastered 2011"
        ));
        assert!(!title_matches("失恋无罪", "恋"));
        assert!(!title_matches("", "恋"));
    }

    /// Fake provider calls: a scripted answer per provider, and a call log.
    struct Script {
        answers: fn(Provider) -> Result<Option<Hit>>,
        calls: Mutex<Vec<Provider>>,
    }

    impl Script {
        async fn call(&self, p: Provider) -> Result<Option<Hit>> {
            self.calls.lock().unwrap().push(p);
            (self.answers)(p)
        }
    }

    fn koi_hit() -> Result<Option<Hit>> {
        let text = (netease::API.lyric)(&fixture("netease_lyric.json")).unwrap();
        Ok(text.map(|lrc| Hit {
            title: "恋".into(),
            lrc,
        }))
    }

    #[tokio::test]
    async fn providers_run_in_order_until_one_passes() {
        let s = Script {
            answers: |p| match p {
                Provider::Qq => Err(anyhow!("timeout")),
                Provider::Netease => Ok(None),
                Provider::Kugou => koi_hit(),
            },
            calls: Mutex::new(vec![]),
        };
        let b = Breaker::new();
        let found = find_with(&b, &koi(), Some(&koi_plain()), |p| s.call(p))
            .await
            .unwrap();
        assert_eq!(found.provider, Provider::Kugou);
        assert_eq!(*s.calls.lock().unwrap(), Provider::ALL);
    }

    #[tokio::test]
    async fn a_tripped_provider_is_skipped() {
        let s = Script {
            answers: |p| match p {
                Provider::Qq => Err(anyhow!("blocked")),
                _ => Ok(None),
            },
            calls: Mutex::new(vec![]),
        };
        let b = Breaker::new();
        for _ in 0..3 {
            assert!(find_with(&b, &koi(), None, |p| s.call(p)).await.is_none());
        }
        s.calls.lock().unwrap().clear();
        find_with(&b, &koi(), None, |p| s.call(p)).await;
        assert_eq!(
            *s.calls.lock().unwrap(),
            vec![Provider::Netease, Provider::Kugou]
        );
    }

    /// Hits the real endpoints. Run by hand, politely:
    /// `cargo test live_check -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn live_check() {
        let tracks = [
            ("恋", "茅原実里", 265.0),
            ("사랑", "Monday Kiz", 283.0),
            ("Tainted Love", "Love Alexa", 183.0),
        ];
        for (track, artist, duration) in tracks {
            let q = Query {
                track: track.into(),
                artist: artist.into(),
                duration,
            };
            let plain = lrclib_plain(&q).await;
            println!(
                "== {} / {} ({:.0}s), LRCLIB plain lines: {:?}",
                track,
                artist,
                duration,
                plain.as_ref().map(Vec::len)
            );
            for p in Provider::ALL {
                let t0 = std::time::Instant::now();
                let hit = fetch(p, &q).await;
                let ms = t0.elapsed().as_millis();
                match hit {
                    Err(e) => println!("  {:?}: ERROR after {} ms: {}", p, ms, e),
                    Ok(None) => println!("  {:?}: no same-length candidate ({} ms)", p, ms),
                    Ok(Some(h)) => {
                        let src = align::parse_lrc(&h.lrc, track, artist);
                        let res = accept(&h, &q, plain.as_deref());
                        let matched = plain
                            .as_ref()
                            .map(|pl| align::map_lines(pl, &src).iter().flatten().count());
                        println!(
                            "  {:?}: '{}', {} timed lines, matched {:?}/{:?}, accepted: {} ({} ms)",
                            p,
                            h.title,
                            src.len(),
                            matched,
                            plain.as_ref().map(Vec::len),
                            res.as_ref().map_or("no".to_string(), |l| format!(
                                "{} lines, first at {:.2}s",
                                l.len(),
                                l[0].time
                            )),
                            ms
                        );
                    }
                }
            }
        }
    }

    async fn lrclib_plain(q: &Query) -> Option<Vec<String>> {
        let url = format!(
            "https://lrclib.net/api/get?track_name={}&artist_name={}&duration={}",
            urlencoding::encode(&q.track),
            urlencoding::encode(&q.artist),
            q.duration.round()
        );
        let v: Value = Client::new()
            .get(url)
            .header(
                "User-Agent",
                "Lyryc live check (https://github.com/Error0229/Lyryc)",
            )
            .send()
            .await
            .ok()?
            .json()
            .await
            .ok()?;
        let plain = v["plainLyrics"].as_str()?;
        Some(
            plain
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(String::from)
                .collect(),
        )
    }
}
