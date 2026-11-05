//! Live runner for `research/llm-search` (approach B). Ignored; run explicitly:
//!
//! ```sh
//! cargo test --lib search_experiment -- --ignored --nocapture --test-threads=1
//! ```
//!
//! Uses the app's saved cleaning route from a *copy* of `llm-settings.json`
//! (`LYRYC_LLM_SETTINGS`, default `%APPDATA%/com.lyryc.app/`), so the real
//! file is never moved or rewritten. The key stays inside `Target`, whose
//! `Debug` hides it. Results are appended per case, so a rerun only spends
//! calls on cases that have no result yet.

use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Instant;

use super::identify::{parse, prompt, request, tools};
use super::store::Store;
use super::{complete_with, Lane, Task};

fn research_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../research/llm-search")
}

fn cases() -> Vec<Value> {
    let text = std::fs::read_to_string(research_dir().join("data/cases.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

#[test]
#[ignore = "research: writes research/llm-search/results/regex_clean.json"]
fn dump_regex_clean() {
    let out: Vec<Value> = cases()
        .iter()
        .map(|c| {
            let raw = c["raw_title"].as_str().unwrap();
            json!({"id": c["id"], "cleaned": crate::cleaning::clean_track_name_sync(raw)})
        })
        .collect();
    let path = research_dir().join("results/regex_clean.json");
    std::fs::write(path, serde_json::to_string_pretty(&out).unwrap()).unwrap();
}

/// `LYRYC_SEARCH` = `on` | `off` | `both` (default `on`): search on, the
/// no-search ablation, or both. `LYRYC_MODEL` overrides the routed model,
/// `LYRYC_IDS=1,2` limits the run to those cases.
#[tokio::test]
#[ignore = "research: live LLM calls on the user's key"]
async fn identify_cases() {
    let settings = std::env::var("LYRYC_LLM_SETTINGS").map_or_else(
        |_| {
            PathBuf::from(std::env::var("APPDATA").unwrap()).join("com.lyryc.app/llm-settings.json")
        },
        PathBuf::from,
    );
    let copy = std::env::temp_dir().join("lyryc-llm-search-settings.json");
    std::fs::copy(&settings, &copy).expect("no llm-settings.json");
    let mut target = Store::load(copy.clone())
        .resolve(Task::Cleaning, &[], &[])
        .expect("no cleaning route");
    let _ = std::fs::remove_file(&copy);
    if let Ok(model) = std::env::var("LYRYC_MODEL") {
        target.model = model;
    }
    let modes: &[bool] = match std::env::var("LYRYC_SEARCH").as_deref() {
        Ok("off") => &[false],
        Ok("both") => &[true, false],
        _ => &[true],
    };

    let ids: Option<Vec<u64>> = std::env::var("LYRYC_IDS")
        .ok()
        .map(|v| v.split(',').filter_map(|i| i.trim().parse().ok()).collect());
    let path = research_dir().join("results/b_identify.jsonl");
    let done = std::fs::read_to_string(&path).unwrap_or_default();
    let seen: Vec<Value> = done
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    for case in cases() {
        if ids
            .as_ref()
            .is_some_and(|ids| !ids.contains(&case["id"].as_u64().unwrap()))
        {
            continue;
        }
        for &search in modes {
            let key = (&case["id"], search, target.model.as_str());
            if seen.iter().any(|r| {
                (
                    &r["id"],
                    r["search"] == true,
                    r["model"].as_str().unwrap_or(""),
                ) == key
            }) {
                continue;
            }
            let user = prompt(
                case["raw_title"].as_str().unwrap(),
                case["raw_artist"].as_str().unwrap(),
                case["duration"].as_f64(),
            );
            let started = Instant::now();
            let reply = complete_with(&target, &request(&user), tools(search), Lane::Quick).await;
            let latency_ms = started.elapsed().as_millis() as u64;
            let row = match reply {
                Ok(c) => json!({
                    "id": case["id"], "search": search, "model": target.model,
                    "latency_ms": latency_ms, "searches": c.searches,
                    "sources": c.sources.len(),
                    "reply": serde_json::from_str::<Value>(&c.text).unwrap_or(json!(c.text)),
                    "identity": parse(&c).map_err(|e| e.to_string()).ok().flatten(),
                }),
                Err(e) => json!({
                    "id": case["id"], "search": search, "model": target.model,
                    "latency_ms": latency_ms, "error": e.to_string(),
                }),
            };
            println!("{row}");
            // Free-tier keys allow only a few requests per minute.
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .unwrap();
            use std::io::Write;
            writeln!(file, "{row}").unwrap();
        }
    }
}
