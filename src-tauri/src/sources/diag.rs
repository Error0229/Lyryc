//! Hit-rate harness for the extra sources (research/sources-hits).
//!
//! For each case it builds the sources query the way the lookup does
//! (`lyrics::source_query`), with the case's identified names ("after") and
//! without them ("nollm", no LLM set up), and records per provider and search
//! the query, the candidates, the pick and the verdict, then which provider
//! the real `find_with` answers with. Every response is kept in
//! `research/sources-hits/.http/`, so a rerun only pays for URLs it has not
//! seen. Requests are sequential, 1 s apart, capped by `LYRYC_DIAG_BUDGET`
//! (default 120), and a host that fails `TRIP_AFTER` times in a row is dropped
//! for the run, as the app's breaker would.
//!
//! ```sh
//! LYRYC_DIAG=after cargo test --lib sources::diag -- --ignored --nocapture
//! ```
//!
//! `results/before.jsonl` came from this harness at c8031b7 (one search per
//! provider: mode a with the player's names, mode b with the native names).

use super::*;
use crate::cleaning::Names;
use crate::llm::identify::SongIdentity;
use crate::lyrics::{identified_names, source_query};
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

impl Identity {
    /// The names the lookup would get from `identify_song`.
    fn names(&self) -> Vec<Names> {
        identified_names(&SongIdentity {
            title_native: self.title_native.clone(),
            title_romanized: self.title_romanized.clone(),
            artist_native: self.artist_native.clone(),
            artist_romanized: self.artist_romanized.clone(),
            album: None,
            duration_s: None,
            isrc: None,
            instrumental: false,
            cover_of: None,
            confidence: 1.0,
            evidence_urls: vec![],
        })
    }
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

    async fn get(&self, url: String, referer: Option<&'static str>) -> Result<Value> {
        let file = dir().join(".http").join(fnv(&url) + ".json");
        if let Ok(text) = std::fs::read_to_string(&file) {
            self.stats.lock().unwrap().cached += 1;
            let stored: Value = serde_json::from_str(&text)?;
            return Ok(stored["body"].clone());
        }
        {
            let mut stats = self.stats.lock().unwrap();
            let host = Self::host(&url);
            if stats.errors.get(&host).copied().unwrap_or(0) >= breaker::TRIP_AFTER {
                return Err(anyhow::anyhow!("{host} tripped; not asking"));
            }
            if stats.live >= self.budget {
                return Err(anyhow::anyhow!("request budget spent"));
            }
            stats.live += 1;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
        let got = get_json(url.clone(), referer).await;
        let mut stats = self.stats.lock().unwrap();
        let errors = stats.errors.entry(Self::host(&url)).or_default();
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

/// The harness's own judge: does a result name the case's song, by the
/// names in cases.json? Independent of the guards under test.
fn is_the_song(title: &str, artist: &str, case: &Case) -> bool {
    let title_ok = case.titles.iter().any(|t| title_matches(title, t));
    let artist_ok = case.artists.iter().any(|a| {
        let (a, c) = (align::norm(a), align::norm(artist));
        !a.is_empty() && (c.contains(&a) || a.contains(&c) || char_sim(&c, &a) >= 0.6)
    });
    title_ok && artist_ok && !version_re().is_match(title)
}

/// One provider, one name, through the real `fetch`, with the story around it.
async fn probe(http: &Http, p: Provider, name: &Name, q: &Query, case: &Case) -> Value {
    let get = |url, referer| http.get(url, referer);
    let api = p.api();
    let url = (api.search_url)(name, q.duration);
    let mut row = json!({"provider": p, "query": url, "track": name.title, "artist": name.artist});
    // Cached by now or about to be: the same request `fetch` makes first.
    if let Ok(cands) = get(url, api.referer)
        .await
        .and_then(|v| (api.candidates)(&v))
    {
        let deltas: Vec<f64> = cands
            .iter()
            .filter(|c| is_the_song(&c.title, &c.artist, case))
            .map(|c| c.duration - q.duration)
            .collect();
        row["song_deltas"] = json!(deltas);
        row["candidates"] = json!(cands
            .iter()
            .map(|c| json!([c.title, c.artist, c.duration]))
            .collect::<Vec<_>>());
        if let Ok(pick) = best(cands, q, true) {
            row["pick"] = json!([pick.title, pick.artist, pick.duration]);
            row["pick_is_song"] = json!(is_the_song(&pick.title, &pick.artist, case));
        }
    }
    match fetch(&get, p, name, q, None).await {
        Err(e) => {
            row["outcome"] = json!("error");
            row["error"] = json!(e.to_string());
        }
        Ok(Err(miss)) => row["outcome"] = json!(miss),
        Ok(Ok((lines, _))) => {
            row["outcome"] = json!("accepted");
            row["lines"] = json!(lines.len());
        }
    }
    row
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
        println!(
            "#{} {} / {} ({}s, {})",
            case.id, case.raw_title, case.raw_artist, duration, case.kind
        );
        let identified = case
            .identity
            .as_ref()
            .map(Identity::names)
            .unwrap_or_default();
        for (mode, names) in [("after", identified), ("nollm", vec![])] {
            let q = source_query(&case.raw_title, &case.raw_artist, &names, duration);
            for p in Provider::ALL {
                // Alone, as its hit rate: its names in order until one passes.
                for name in q.searches() {
                    let mut row = probe(&http, p, name, &q, &case).await;
                    row["id"] = json!(case.id);
                    row["mode"] = json!(mode);
                    println!(
                        "  {mode} {p:?} '{}' / '{}': {} {}",
                        name.title,
                        name.artist,
                        row["outcome"].as_str().unwrap_or(""),
                        row.get("pick").map_or(String::new(), |v| format!(
                            "{v} song={}",
                            row["pick_is_song"]
                        ))
                    );
                    let done = row["outcome"] == "accepted";
                    rows.push(row);
                    if done {
                        break;
                    }
                }
            }
            // Together, as the app runs them.
            let get = |url, referer| http.get(url, referer);
            let found = find_with(&Breaker::new(), &q, |p, n| fetch(&get, p, n, &q, None)).await;
            let winner = found.map(|f| f.provider);
            println!("  {mode} => {winner:?}");
            rows.push(
                json!({"id": case.id, "mode": format!("{mode}-pipeline"), "provider": winner}),
            );
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
