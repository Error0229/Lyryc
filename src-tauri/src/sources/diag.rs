//! Hit-rate harness for the extra sources (research/sources-hits).
//!
//! For each case and provider it records the query, the candidates, what
//! `best()` picked or why nothing, and whether the guards passed and why not.
//! Every response is kept in `research/sources-hits/.http/`, so a rerun only
//! pays for URLs it has not seen. Requests are sequential, 1 s apart, capped by
//! `LYRYC_DIAG_BUDGET` (default 120), and a provider that fails `TRIP_AFTER`
//! times in a row is dropped for the run, as the app's breaker would.
//!
//! ```sh
//! LYRYC_DIAG=before cargo test --lib sources::diag -- --ignored --nocapture
//! ```

use super::*;
use crate::cleaning::clean_title;
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Deserialize, Clone)]
struct Case {
    id: u32,
    kind: String,
    raw_title: String,
    raw_artist: String,
    play: Option<f64>,
    titles: Vec<String>,
    artists: Vec<String>,
    identity: Option<Identity>,
}

#[derive(Deserialize, Clone)]
struct Identity {
    title_native: String,
    artist_native: String,
    title_romanized: Option<String>,
    artist_romanized: Option<String>,
}

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../research/sources-hits")
}

/// FNV-1a: a cache file name that is stable across toolchains.
fn fnv(s: &str) -> String {
    let h = s.bytes().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
    });
    format!("{h:016x}")
}

#[derive(Default)]
struct Stats {
    live: usize,
    cached: usize,
    errors: HashMap<String, u8>,
}

/// Cached, sequential, budgeted GET.
struct Http {
    budget: usize,
    stats: Mutex<Stats>,
}

impl Http {
    fn new() -> Self {
        let budget = std::env::var("LYRYC_DIAG_BUDGET")
            .ok()
            .and_then(|b| b.parse().ok())
            .unwrap_or(120);
        Self {
            budget,
            stats: Mutex::default(),
        }
    }

    fn host(url: &str) -> String {
        url.split('/').nth(2).unwrap_or("").to_string()
    }

    fn tripped(&self, url: &str) -> bool {
        let stats = self.stats.lock().unwrap();
        stats.errors.get(&Self::host(url)).copied().unwrap_or(0) >= breaker::TRIP_AFTER
    }

    async fn get(&self, url: &str, referer: Option<&str>) -> Result<Value> {
        let file = dir().join(".http").join(fnv(url) + ".json");
        if let Ok(text) = std::fs::read_to_string(&file) {
            self.stats.lock().unwrap().cached += 1;
            let stored: Value = serde_json::from_str(&text)?;
            return Ok(stored["body"].clone());
        }
        if self.tripped(url) {
            return Err(anyhow::anyhow!("{} tripped; not asking", Self::host(url)));
        }
        {
            let mut stats = self.stats.lock().unwrap();
            if stats.live >= self.budget {
                return Err(anyhow::anyhow!("request budget spent"));
            }
            stats.live += 1;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
        let got = get_json(url, referer).await;
        let mut stats = self.stats.lock().unwrap();
        let errors = stats.errors.entry(Self::host(url)).or_default();
        match &got {
            Ok(body) => {
                *errors = 0;
                std::fs::create_dir_all(file.parent().unwrap())?;
                std::fs::write(&file, json!({"url": url, "body": body}).to_string())?;
            }
            Err(e) => {
                *errors += 1;
                println!("    !! {url}: {e}");
            }
        }
        got
    }
}

/// Does the candidate name the case's song (any of its known names)?
fn names_song(title: &str, artist: &str, case: &Case) -> bool {
    let title_ok = case.titles.iter().any(|t| title_matches(title, t));
    let artist_ok = case.artists.iter().any(|a| {
        let (a, c) = (align::norm(a), align::norm(artist));
        !a.is_empty() && (c.contains(&a) || a.contains(&c) || char_sim(&c, &a) >= 0.6)
    });
    title_ok && artist_ok && !cover_re().is_match(title)
}

/// One provider, one query: the whole story as a JSON row.
async fn probe(http: &Http, p: Provider, q: &Query, case: &Case) -> Value {
    let api = p.api();
    let url = (api.search_url)(q);
    let mut row = json!({"provider": p, "query": url, "track": q.track, "artist": q.artist});
    let cands = match http.get(&url, api.referer).await.and_then(|v| (api.candidates)(&v)) {
        Ok(c) => c,
        Err(e) => {
            row["outcome"] = json!("error");
            row["error"] = json!(e.to_string());
            return row;
        }
    };
    let song: Vec<f64> = cands
        .iter()
        .filter(|c| names_song(&c.title, &c.artist, case))
        .map(|c| c.duration - q.duration)
        .collect();
    row["candidates"] = json!(cands
        .iter()
        .map(|c| json!([c.title, c.artist, c.duration]))
        .collect::<Vec<_>>());
    row["song_deltas"] = json!(song);
    let Some(pick) = best(cands.clone(), q) else {
        row["outcome"] = json!(if cands.is_empty() { "empty" } else { "no_same_length" });
        return row;
    };
    row["pick"] = json!([pick.title, pick.artist, pick.duration]);
    row["pick_is_song"] = json!(names_song(&pick.title, &pick.artist, case));
    // The title guard needs no lyric: don't download one it would throw away.
    if !title_matches(&pick.title, &q.track) {
        row["outcome"] = json!("rejected");
        row["reject"] = json!(Reject::Title);
        return row;
    }
    let lrc = match http
        .get(&(api.lyric_url)(&pick), api.referer)
        .await
        .and_then(|v| (api.lyric)(&v))
    {
        Ok(Some(lrc)) => lrc,
        Ok(None) => {
            row["outcome"] = json!("no_lyric");
            return row;
        }
        Err(e) => {
            row["outcome"] = json!("error");
            row["error"] = json!(e.to_string());
            return row;
        }
    };
    let hit = Hit {
        title: pick.title.clone(),
        lrc,
    };
    match check(&hit, q, None) {
        Ok(lines) => {
            row["outcome"] = json!("accepted");
            row["lines"] = json!(lines.len());
        }
        Err(r) => {
            row["outcome"] = json!("rejected");
            row["reject"] = json!(r);
        }
    }
    row
}

/// (a) what the app sends today; (b) the identified native names.
fn queries(case: &Case, duration: f64) -> Vec<(&'static str, Query)> {
    let mut out = vec![(
        "a",
        Query {
            track: clean_title(&case.raw_title, &case.raw_artist),
            artist: case.raw_artist.clone(),
            duration,
        },
    )];
    if let Some(id) = &case.identity {
        out.push((
            "b",
            Query {
                track: id.title_native.clone(),
                artist: id.artist_native.clone(),
                duration,
            },
        ));
    }
    out
}

fn cases() -> Vec<Case> {
    let text = std::fs::read_to_string(dir().join("cases.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

#[tokio::test]
#[ignore = "network"]
async fn hit_rate() {
    let label = std::env::var("LYRYC_DIAG").unwrap_or_else(|_| "run".into());
    let http = Http::new();
    let mut rows = Vec::new();
    for case in cases() {
        // The app never asks the sources without a length.
        let Some(duration) = case.play else { continue };
        println!("#{} {} / {} ({}s, {})", case.id, case.raw_title, case.raw_artist, duration, case.kind);
        for (mode, q) in queries(&case, duration) {
            for p in Provider::ALL {
                let mut row = probe(&http, p, &q, &case).await;
                row["id"] = json!(case.id);
                row["mode"] = json!(mode);
                println!(
                    "  {mode} {:?} '{}' / '{}': {} {}",
                    p,
                    q.track,
                    q.artist,
                    row["outcome"].as_str().unwrap_or(""),
                    row.get("pick").map_or(String::new(), |v| format!(
                        "{v} song={} {}",
                        row["pick_is_song"],
                        row.get("reject").map_or(String::new(), Value::to_string)
                    ))
                );
                rows.push(row);
            }
        }
    }
    let out = dir().join("results").join(format!("{label}.jsonl"));
    std::fs::create_dir_all(out.parent().unwrap()).unwrap();
    let text: String = rows.iter().map(|r| r.to_string() + "\n").collect();
    std::fs::write(&out, text).unwrap();
    let stats = http.stats.lock().unwrap();
    println!(
        "{} rows -> {}; {} live request(s), {} cached",
        rows.len(),
        out.display(),
        stats.live,
        stats.cached
    );
}
