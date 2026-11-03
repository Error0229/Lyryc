//! Opt-in synced lyrics from QQ Music, NetEase and Kugou.
//!
//! These are unofficial, reverse-engineered endpoints (the same ones Lyricify,
//! syncedlyrics and LyricsX use). They are only called when the user turned on
//! "Extra lyric sources", and every provider sits behind a circuit breaker, so
//! one that breaks or starts blocking stops costing anything.
//! Benchmark and endpoint survey: docs/research/timing/REPORT.md; hit rates by
//! name and guard: research/sources-hits.
//!
//! A song is looked up under up to `MAX_SEARCHES` of its names (identified
//! native names first), each name across the providers in order.
//!
//! A wrong song is worse than no timing, so every result has to pass guards:
//! a candidate of the same length (+-3 s), at least `MIN_LINES` timed lines,
//! and either >= 50% of the known lines text-matched or, with no known text, a
//! title and an artist from the song's names and no version marker (live,
//! cover, ...) that its names lack.

pub(crate) mod align;
mod breaker;
#[cfg(test)]
mod diag;
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

/// One name of a song.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Name {
    pub title: String,
    pub artist: String,
}

/// What to look for. `duration` is the length of the recording being played.
#[derive(Clone, Debug)]
pub struct Query {
    /// The song's names, best first, distinct and with a title. The first
    /// `MAX_SEARCHES` are searched for; all of them count for the guards.
    pub names: Vec<Name>,
    pub duration: f64,
    /// Only the words are wanted (a cover's original): any recording's length
    /// will do, and its timing is not checked against `duration`.
    pub any_length: bool,
}

impl Query {
    pub fn new(names: impl IntoIterator<Item = Name>, duration: f64) -> Self {
        let mut out: Vec<Name> = Vec::new();
        for n in names {
            let n = Name {
                title: n.title.trim().to_string(),
                artist: n.artist.trim().to_string(),
            };
            if !n.title.is_empty() && !out.contains(&n) {
                out.push(n);
            }
        }
        Self {
            names: out,
            duration,
            any_length: false,
        }
    }

    /// The song's words, from whichever recording has them.
    pub fn words_of(names: impl IntoIterator<Item = Name>, duration: f64) -> Self {
        Self { any_length: true, ..Self::new(names, duration) }
    }

    fn searches(&self) -> &[Name] {
        &self.names[..self.names.len().min(MAX_SEARCHES)]
    }

    /// Does a result carry one of the song's titles and one of its artists,
    /// and no version marker that the song's titles lack?
    fn names_song(&self, title: &str, artist: &str) -> bool {
        let marked = |t: &str| version_re().is_match(t);
        self.names.iter().any(|n| title_matches(title, &n.title))
            && self.names.iter().any(|n| artist_matches(artist, &n.artist))
            && (!marked(title) || self.names.iter().any(|n| marked(&n.title)))
    }

    /// Best similarity of `s` to the song's names, title or artist by `field`.
    fn sim(&self, s: &str, field: fn(&Name) -> &str) -> f64 {
        self.names
            .iter()
            .map(|n| char_sim(s, field(n)))
            .fold(0.0, f64::max)
    }
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

/// A provider is two requests, search then lyric, described by these parsers.
/// Parsers return Err when the response no longer has the expected shape, which
/// counts as a failure for the breaker; Ok(empty) is a normal "not found".
struct Api {
    referer: Option<&'static str>,
    search_url: fn(&Name, f64) -> String,
    candidates: fn(&Value) -> Result<Vec<Candidate>>,
    lyric_url: fn(&Candidate) -> String,
    lyric: fn(&Value) -> Result<Option<String>>,
}

const TIMEOUT: Duration = Duration::from_secs(3);
/// Names searched per provider and song.
const MAX_SEARCHES: usize = 2;
const SAME_RECORDING_SECS: f64 = 3.0;
const MIN_LINES: usize = 3;
/// Share of the known lines that must find a partner in the source text.
const MIN_MATCHED: f64 = 0.5;
/// Title similarity needed when there is no text to check against.
const MIN_TITLE_SIM: f64 = 0.6;
/// A name inside a longer credit must be this long to count ("Eve" is not "Steve").
const MIN_CONTAINED: usize = 4;
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

async fn get_json(url: String, referer: Option<&'static str>) -> Result<Value> {
    let mut req = client().get(url);
    if let Some(r) = referer {
        req = req.header(reqwest::header::REFERER, r);
    }
    Ok(req.send().await?.error_for_status()?.json().await?)
}

/// Why one search of one provider gave no timing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Miss {
    /// No results at all.
    Empty,
    /// Nothing within `SAME_RECORDING_SECS` of the length being played.
    NoSameLength,
    /// Same length, but no result names the song (checked with no known text).
    OtherSong,
    /// The picked result has no lyric.
    NoLyric,
    /// Fewer than `MIN_LINES` timed lines.
    FewLines,
    /// Timed past the end of the recording being played.
    TooLong,
    /// Under `MIN_MATCHED` of the known lines found a partner.
    Text,
}

/// Lines from one provider for one name, or why not.
type Attempt = Result<Vec<LyricLine>, Miss>;

/// One provider, one name: one search and at most one lyric request, both
/// through `get(url, referer)`. No retries. Err is a failed request or a
/// response of the wrong shape, which is what the breaker counts.
async fn fetch<G, Fut>(
    get: &G,
    p: Provider,
    name: &Name,
    q: &Query,
    plain: Option<&[String]>,
) -> Result<Attempt>
where
    G: Fn(String, Option<&'static str>) -> Fut,
    Fut: Future<Output = Result<Value>>,
{
    let api = p.api();
    let found = get((api.search_url)(name, q.duration), api.referer).await?;
    let c = match best((api.candidates)(&found)?, q, plain.is_none()) {
        Ok(c) => c,
        Err(miss) => return Ok(Err(miss)),
    };
    let body = get((api.lyric_url)(&c), api.referer).await?;
    let Some(lrc) = (api.lyric)(&body)? else {
        return Ok(Err(Miss::NoLyric));
    };
    Ok(check(
        &align::parse_lrc(&lrc, &c.title, &c.artist),
        q,
        plain,
    ))
}

/// Timing for `q` from the first provider that has some. With `plain`, the
/// result keeps those lines and takes only times from the source; without it,
/// the source's own lines are used. Requests run one after another.
pub async fn find(q: &Query, plain: Option<&[String]>) -> Option<Found> {
    find_with(&BREAKER, q, |p, name| fetch(&get_json, p, name, q, plain)).await
}

/// Each searched name across the providers in order, until one passes.
async fn find_with<'a, F, Fut>(breaker: &Breaker, q: &'a Query, fetch: F) -> Option<Found>
where
    F: Fn(Provider, &'a Name) -> Fut,
    Fut: Future<Output = Result<Attempt>>,
{
    for name in q.searches() {
        for p in Provider::ALL {
            if breaker.is_open(p) {
                continue;
            }
            let attempt = fetch(p, name).await;
            breaker.record(p, attempt.is_ok());
            match attempt {
                Err(e) => debug!(
                    "{:?} failed for '{}' by '{}': {}",
                    p, name.title, name.artist, e
                ),
                Ok(Err(miss)) => debug!(
                    "{:?}: {:?} for '{}' by '{}'",
                    p, miss, name.title, name.artist
                ),
                Ok(Ok(lines)) => return Some(Found { provider: p, lines }),
            }
        }
    }
    None
}

/// The guards on the picked result's timed lines. None of them is "partially
/// trust it".
fn check(src: &[(f64, String)], q: &Query, plain: Option<&[String]>) -> Attempt {
    let last = src.last().ok_or(Miss::FewLines)?.0;
    if src.len() < MIN_LINES {
        return Err(Miss::FewLines);
    }
    if last > q.duration + SAME_RECORDING_SECS && !q.any_length {
        return Err(Miss::TooLong);
    }
    let Some(plain) = plain else {
        return Ok(to_lines(src.to_vec()));
    };
    let mapped = align::map_lines(plain, src);
    let matched = mapped.iter().flatten().count();
    if (matched as f64) < MIN_MATCHED * plain.len() as f64 {
        return Err(Miss::Text);
    }
    let times = align::interpolate(&mapped, q.duration.max(last));
    Ok(to_lines(
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
            words: None,
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

/// Some part of one artist credit is some part of the other: "Ayase / YOASOBI"
/// is YOASOBI, "米津玄師 (よねづ けんし)" is 米津玄師.
fn artist_matches(candidate: &str, known: &str) -> bool {
    let parts = |s: &str| -> Vec<String> {
        s.split(['/', ',', '&', '、', '(', ')', '（', '）', '×'])
            .chain([s])
            .map(align::norm)
            .filter(|p| !p.is_empty())
            .collect()
    };
    let known = parts(known);
    parts(candidate).iter().any(|c| {
        known.iter().any(|k| {
            let (short, long) = if c.len() < k.len() { (c, k) } else { (k, c) };
            short == long
                || (short.chars().count() >= MIN_CONTAINED && long.contains(short.as_str()))
        })
    })
}

/// Share of distinct characters two names have in common.
fn char_sim(a: &str, b: &str) -> f64 {
    use std::collections::HashSet;
    let a: HashSet<char> = align::norm(a).chars().collect();
    let b: HashSet<char> = align::norm(b).chars().collect();
    a.intersection(&b).count() as f64 / a.union(&b).count().max(1) as f64
}

/// A recording other than the original: cover, live, karaoke, other language.
fn version_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)\b(cover|live|karaoke|instrumental|inst|remix|dj|ver|version|acoustic|piano)\b|翻唱|女声|男声|伴奏|原唱|现场|現場|纯音乐",
        )
        .unwrap()
    })
}

/// Same length first; with `strict` (no known text to check against later),
/// only results that name the song. Then the closest title and artist, with
/// other versions penalised.
fn best(cands: Vec<Candidate>, q: &Query, strict: bool) -> Result<Candidate, Miss> {
    if cands.is_empty() {
        return Err(Miss::Empty);
    }
    let same: Vec<Candidate> = cands
        .into_iter()
        .filter(|c| q.any_length || (c.duration > 0.0 && (c.duration - q.duration).abs() <= SAME_RECORDING_SECS))
        .collect();
    if same.is_empty() {
        return Err(Miss::NoSameLength);
    }
    let length_gap = |c: &Candidate| if q.any_length { 0.0 } else { (c.duration - q.duration).abs() / 30.0 };
    let score = |c: &Candidate| {
        q.sim(&c.title, |n| &n.title) + q.sim(&c.artist, |n| &n.artist)
            - length_gap(c)
            - if version_re().is_match(&c.title) {
                0.5
            } else {
                0.0
            }
    };
    same.into_iter()
        .filter(|c| !strict || q.names_song(&c.title, &c.artist))
        .map(|c| (score(&c), c))
        .reduce(|a, b| if b.0 > a.0 { b } else { a })
        .map(|(_, c)| c)
        .ok_or(Miss::OtherSong)
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

    pub fn name(title: &str, artist: &str) -> Name {
        Name {
            title: title.into(),
            artist: artist.into(),
        }
    }

    pub fn koi() -> Query {
        Query::new([name("恋", "茅原実里")], 265.0)
    }

    /// LRCLIB-style plain text for the fixture song: the NetEase lines, minus
    /// timing, with one line changed and one added, as a different text source
    /// would have them.
    pub fn koi_plain() -> Vec<String> {
        let mut lines: Vec<String> = koi_src().into_iter().map(|(_, t)| t).collect();
        lines[2] = "ソワソワしてる".into();
        lines.insert(5, "(spoken intro that no source has)".into());
        lines
    }

    fn koi_src() -> Vec<(f64, String)> {
        let text = (netease::API.lyric)(&fixture("netease_lyric.json"))
            .unwrap()
            .unwrap();
        align::parse_lrc(&text, "恋", "茅原実里")
    }

    fn cand(title: &str, artist: &str, duration: f64) -> Candidate {
        Candidate {
            id: format!("{title}/{artist}"),
            title: title.into(),
            artist: artist.into(),
            duration,
            key: String::new(),
        }
    }

    fn timed(lines: &[(f64, &str)]) -> Vec<(f64, String)> {
        lines.iter().map(|(t, s)| (*t, s.to_string())).collect()
    }

    #[test]
    fn query_keeps_distinct_titled_names_in_order() {
        let q = Query::new(
            [
                name("雪国", "ヨルシカ"),
                name(" 雪国 ", "ヨルシカ"),
                name("", "ヨルシカ"),
                name("Snow Country", "Yorushika"),
                name("Yukiguni", "Yorushika"),
            ],
            288.0,
        );
        assert_eq!(
            q.names,
            [
                name("雪国", "ヨルシカ"),
                name("Snow Country", "Yorushika"),
                name("Yukiguni", "Yorushika")
            ]
        );
        assert_eq!(q.searches().len(), MAX_SEARCHES);
    }

    #[test]
    fn words_of_a_song_take_any_recording_that_names_it() {
        let q = Query::words_of([name("恋", "茅原実里")], 299.0);
        let pick = best(vec![cand("恋", "茅原実里", 240.0)], &q, true).unwrap();
        assert_eq!(pick.duration, 240.0, "a cover's length says nothing about the original's");
        assert!(best(vec![cand("別の歌", "誰か", 299.0)], &q, true).is_err(), "but it must name the song");
        let long: Vec<(f64, String)> = (0..5).map(|i| (i as f64 * 80.0, format!("line {i}"))).collect();
        assert!(check(&long, &q, None).is_ok(), "timing past this recording's end is fine");
    }

    #[test]
    fn best_needs_same_length_then_prefers_originals() {
        let q = Query::new([name("Tainted Love", "Soft Cell")], 183.0);
        let cands = vec![
            cand("Tainted Love", "Soft Cell", 200.0),
            cand("Tainted Love (Cover)", "Soft Cell", 183.0),
            cand("Tainted Love", "Soft Cell", 184.5),
        ];
        assert_eq!(best(cands.clone(), &q, true).unwrap().duration, 184.5);
        assert_eq!(best(cands, &q, false).unwrap().duration, 184.5);
        let pick = |c| best(vec![c], &q, true).map(|c| c.duration);
        assert_eq!(
            pick(cand("Tainted Love", "Soft Cell", 190.0)),
            Err(Miss::NoSameLength)
        );
        assert_eq!(
            pick(cand("Tainted Love", "Soft Cell", 0.0)),
            Err(Miss::NoSameLength)
        );
        assert_eq!(best(vec![], &q, true), Err(Miss::Empty));
    }

    /// Picks from the hit-rate study (research/sources-hits): the same length,
    /// and what decided them was the name.
    #[test]
    fn with_no_known_text_a_pick_must_carry_the_songs_names() {
        let pick = |q: &Query, cands: Vec<Candidate>| best(cands, q, true).map(|c| c.title);

        // A romanized query passes on its own name, never on a cover's.
        let kaikai = Query::new(
            [name("回回奇譚", "Eve"), name("Kaikai Kitan", "Eve")],
            221.0,
        );
        let covers = vec![
            cand("Kaikai Kitan", "Will Stetson", 218.0),
            cand("Kaikai Kitan", "Eve", 221.9),
        ];
        assert_eq!(pick(&kaikai, covers), Ok("Kaikai Kitan".into()));
        assert_eq!(
            pick(&kaikai, vec![cand("Kaikai Kitan", "Will Stetson", 218.0)]),
            Err(Miss::OtherSong)
        );

        // The identified native title is accepted next to the player's romanized one.
        let gurenge = cand("紅蓮華", "LiSA (织部里沙)", 238.0);
        let raw = Query::new([name("Gurenge", "LiSA")], 239.0);
        assert_eq!(pick(&raw, vec![gurenge.clone()]), Err(Miss::OtherSong));
        let known = Query::new([name("紅蓮華", "LiSA"), name("Gurenge", "LiSA")], 239.0);
        assert_eq!(pick(&known, vec![gurenge]), Ok("紅蓮華".into()));

        // Traditional names find Simplified listings.
        let balloon = Query::new([name("告白氣球", "周杰倫")], 215.0);
        assert_eq!(
            pick(
                &balloon,
                vec![
                    cand("告白气球", "Xai小爱", 215.7),
                    cand("告白气球", "周杰伦", 215.0)
                ]
            ),
            Ok("告白气球".into())
        );

        // Same title, other artist or other recording.
        let shanhai = Query::new([name("山海", "草東沒有派對")], 311.0);
        assert_eq!(
            pick(&shanhai, vec![cand("山海 (Live)", "华晨宇", 309.0)]),
            Err(Miss::OtherSong)
        );
        let qing = Query::new([name("晴天", "周杰倫")], 270.0);
        assert_eq!(
            pick(
                &qing,
                vec![
                    cand("晴天 (原唱 周杰伦)", "RyaVocal", 270.7),
                    cand("晴天", "周杰伦", 269.0)
                ]
            ),
            Ok("晴天".into())
        );
        let spring = Query::new(
            [name("봄날", "방탄소년단"), name("Spring Day", "BTS")],
            274.0,
        );
        assert_eq!(
            pick(
                &spring,
                vec![cand(
                    "Spring Day (Japanese ver.)",
                    "BTS (防弹少年团)",
                    276.7
                )]
            ),
            Err(Miss::OtherSong)
        );
        let haru = Query::new([name("晴る", "ヨルシカ")], 277.0);
        assert_eq!(
            pick(&haru, vec![cand("雨晴るる", "ヨルシカ (Yorushika)", 277.0)]),
            Err(Miss::OtherSong)
        );

        // A marker the song's own title has is no other version.
        let live = Query::new([name("Live Forever", "Oasis")], 276.0);
        assert_eq!(
            pick(&live, vec![cand("Live Forever", "Oasis", 276.0)]),
            Ok("Live Forever".into())
        );
    }

    #[test]
    fn known_text_picks_by_score_alone() {
        // The text match decides later, so a differently credited entry may be tried.
        let q = Query::new([name("Koi", "Minori Chihara")], 265.0);
        let c = cand("恋", "茅原実里", 265.0);
        assert_eq!(best(vec![c.clone()], &q, false), Ok(c.clone()));
        assert_eq!(best(vec![c], &q, true), Err(Miss::OtherSong));
    }

    #[test]
    fn artist_credits_match_by_part() {
        assert!(artist_matches("YOASOBI (ヨアソビ)", "Ayase / YOASOBI"));
        assert!(artist_matches("米津玄師 (よねづ けんし)", "米津玄師"));
        assert!(artist_matches("茅原实里 (ちはらみのり)", "茅原実里"));
        assert!(artist_matches(
            "草东没有派对",
            "草東沒有派對 No Party For Cao Dong"
        ));
        assert!(artist_matches("Queen", "Queen Official"));
        assert!(artist_matches("AKMU (악동뮤지션)", "AKMU"));
        assert!(!artist_matches("Steve", "Eve"));
        assert!(!artist_matches("Will Stetson", "Eve"));
        assert!(!artist_matches("米津玄師", "Kenshi Yonezu"));
        assert!(!artist_matches("", "Eve"));
    }

    #[test]
    fn guards_keep_lrclib_lines_and_take_source_times() {
        let plain = koi_plain();
        let lines = check(&koi_src(), &koi(), Some(&plain)).unwrap();
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
        let q = Query::new([name("Song", "Band")], 200.0);
        let plain: Vec<String> = [
            "one two three",
            "four five six",
            "seven eight nine",
            "ten eleven twelve",
        ]
        .map(String::from)
        .to_vec();
        let good = timed(&[
            (10.0, "one two three"),
            (20.0, "four five six"),
            (30.0, "seven eight nine"),
        ]);
        assert!(check(&good, &q, Some(&plain)).is_ok());
        assert_eq!(check(&good, &q, None).unwrap().len(), 3);

        let short = timed(&[(10.0, "one two three"), (20.0, "four five six")]);
        assert_eq!(check(&short, &q, None).err(), Some(Miss::FewLines));
        assert_eq!(check(&[], &q, None).err(), Some(Miss::FewLines));

        // Under half of the known lines match: a different song.
        let other = timed(&[
            (10.0, "one two three"),
            (20.0, "la la la"),
            (30.0, "na na na"),
            (40.0, "oh oh"),
        ]);
        assert_eq!(check(&other, &q, Some(&plain)).err(), Some(Miss::Text));

        // Timestamps past the end of the track: a longer recording.
        let long = timed(&[
            (10.0, "one two three"),
            (20.0, "four five six"),
            (250.0, "seven eight nine"),
        ]);
        assert_eq!(check(&long, &q, Some(&plain)).err(), Some(Miss::TooLong));
    }

    #[test]
    fn source_lines_lose_credits_and_the_title_banner() {
        let text = (qq::API.lyric)(&fixture("qq_lyric.json")).unwrap().unwrap();
        let src = align::parse_lrc(&text, "恋 (Love)", "茅原实里 (ちはらみのり)");
        let lines = check(&src, &koi(), None).unwrap();
        assert_eq!(lines[0].text, "眠れない夜 脳裏をよぎる");
        assert!(lines.len() > 30);
    }

    #[test]
    fn title_match_is_prefix_or_close() {
        assert!(title_matches("Tainted Love (Remastered)", "Tainted Love"));
        assert!(title_matches("恋 (Love)", "恋"));
        assert!(title_matches(
            "Bohemian Rhapsody",
            "Bohemian Rhapsody - Remastered 2011"
        ));
        assert!(title_matches("海阔天空", "海闊天空"));
        assert!(!title_matches("失恋无罪", "恋"));
        assert!(!title_matches("", "恋"));
    }

    /// Fake provider calls: a scripted answer per provider and name, and a call log.
    struct Script {
        answers: fn(Provider, &Name) -> Result<Attempt>,
        calls: Mutex<Vec<(Provider, String)>>,
    }

    impl Script {
        fn new(answers: fn(Provider, &Name) -> Result<Attempt>) -> Self {
            Self {
                answers,
                calls: Mutex::new(vec![]),
            }
        }
        async fn call(&self, p: Provider, n: &Name) -> Result<Attempt> {
            self.calls.lock().unwrap().push((p, n.title.clone()));
            (self.answers)(p, n)
        }
        fn calls(&self) -> Vec<(Provider, String)> {
            self.calls.lock().unwrap().clone()
        }
    }

    fn koi_lines() -> Result<Attempt> {
        Ok(check(&koi_src(), &koi(), None))
    }

    #[tokio::test]
    async fn providers_run_in_order_until_one_passes() {
        let s = Script::new(|p, _| match p {
            Provider::Qq => Err(anyhow!("timeout")),
            Provider::Netease => Ok(Err(Miss::NoSameLength)),
            Provider::Kugou => koi_lines(),
        });
        let found = find_with(&Breaker::new(), &koi(), |p, n| s.call(p, n))
            .await
            .unwrap();
        assert_eq!(found.provider, Provider::Kugou);
        let providers: Vec<Provider> = s.calls().into_iter().map(|(p, _)| p).collect();
        assert_eq!(providers, Provider::ALL);
    }

    #[tokio::test]
    async fn each_name_is_tried_across_providers_before_the_next() {
        let q = Query::new(
            [
                name("雪国", "ヨルシカ"),
                name("Snow Country", "Yorushika"),
                name("Yukiguni", "Yorushika"),
            ],
            288.0,
        );
        let s = Script::new(|p, n| match (p, n.title.as_str()) {
            (Provider::Netease, "Snow Country") => koi_lines(),
            _ => Ok(Err(Miss::OtherSong)),
        });
        let found = find_with(&Breaker::new(), &q, |p, n| s.call(p, n))
            .await
            .unwrap();
        assert_eq!(found.provider, Provider::Netease);
        let calls = s.calls();
        assert_eq!(
            calls,
            [
                (Provider::Qq, "雪国".into()),
                (Provider::Netease, "雪国".into()),
                (Provider::Kugou, "雪国".into()),
                (Provider::Qq, "Snow Country".into()),
                (Provider::Netease, "Snow Country".into()),
            ]
        );

        // Nothing anywhere: two searches per provider, never a third name.
        let s = Script::new(|_, _| Ok(Err(Miss::Empty)));
        assert!(find_with(&Breaker::new(), &q, |p, n| s.call(p, n))
            .await
            .is_none());
        assert_eq!(s.calls().len(), MAX_SEARCHES * Provider::ALL.len());
        assert!(s.calls().iter().all(|(_, t)| t != "Yukiguni"));
    }

    #[tokio::test]
    async fn a_tripped_provider_is_skipped() {
        let s = Script::new(|p, _| match p {
            Provider::Qq => Err(anyhow!("blocked")),
            _ => Ok(Err(Miss::Empty)),
        });
        let b = Breaker::new();
        for _ in 0..3 {
            assert!(find_with(&b, &koi(), |p, n| s.call(p, n)).await.is_none());
        }
        s.calls.lock().unwrap().clear();
        find_with(&b, &koi(), |p, n| s.call(p, n)).await;
        let providers: Vec<Provider> = s.calls().into_iter().map(|(p, _)| p).collect();
        assert_eq!(providers, vec![Provider::Netease, Provider::Kugou]);

        // Tripped during a song: its second name skips it too.
        let s = Script::new(|p, _| match p {
            Provider::Kugou => Err(anyhow!("blocked")),
            _ => Ok(Err(Miss::Empty)),
        });
        let b = Breaker::new();
        b.record(Provider::Kugou, false);
        b.record(Provider::Kugou, false);
        let two = Query::new([name("恋", "茅原実里"), name("Koi", "Minori")], 265.0);
        find_with(&b, &two, |p, n| s.call(p, n)).await;
        assert_eq!(
            s.calls()
                .iter()
                .filter(|(p, _)| *p == Provider::Kugou)
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn fetch_searches_by_name_and_picks_through_the_guards() {
        let urls = Mutex::new(Vec::new());
        let get = |url: String, _referer: Option<&'static str>| {
            urls.lock().unwrap().push(url.clone());
            let body = if url.contains("search") {
                fixture("netease_search.json")
            } else {
                fixture("netease_lyric.json")
            };
            async move { Ok(body) }
        };
        let q = koi();
        let lines = fetch(&get, Provider::Netease, &q.names[0], &q, None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(lines[0].time, 24.45);
        let urls = urls.lock().unwrap().clone();
        assert_eq!(urls.len(), 2, "one search, one lyric: {urls:?}");
        assert!(urls[0].contains("s=%E6%81%8B%20"), "searched by the name");

        let other = Query::new([name("恋", "Someone Else")], 265.0);
        let miss = fetch(&get, Provider::Netease, &other.names[0], &other, None)
            .await
            .unwrap();
        assert_eq!(miss.unwrap_err(), Miss::OtherSong);
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
            let q = Query::new([name(track, artist)], duration);
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
                let attempt = fetch(&get_json, p, &q.names[0], &q, plain.as_deref()).await;
                let ms = t0.elapsed().as_millis();
                match attempt {
                    Err(e) => println!("  {:?}: ERROR after {} ms: {}", p, ms, e),
                    Ok(Err(miss)) => println!("  {:?}: {:?} ({} ms)", p, miss, ms),
                    Ok(Ok(lines)) => println!(
                        "  {:?}: {} lines, first at {:.2}s ({} ms)",
                        p,
                        lines.len(),
                        lines[0].time,
                        ms
                    ),
                }
            }
        }
    }

    async fn lrclib_plain(q: &Query) -> Option<Vec<String>> {
        let url = format!(
            "https://lrclib.net/api/get?track_name={}&artist_name={}&duration={}",
            urlencoding::encode(&q.names[0].title),
            urlencoding::encode(&q.names[0].artist),
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
