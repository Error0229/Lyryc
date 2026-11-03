//! Parity bench against the Python study (ignored test; needs the network or
//! the study's audio). Writes `data/pred/<name>.json` in the study's format so
//! `research/prealign/bench/evaluate.py` scores it against the same references.
//!
//! ```text
//! PREALIGN_DIR=<scratch app dir>  PREALIGN_INPUTS=<inputs.json>  PREALIGN_OUT=<pred.json>
//! PREALIGN_AUDIO=ytdlp | wav:<dir with <id>.wav>
//! cargo test --release --lib prealign::bench -- --ignored --nocapture
//! ```
//! `inputs.json`: `{id: {text_src, lines, videoId, duration}}`, the study's
//! aligner input (`bench/run.py` `source_text`). See docs/prealign.md.

use super::align::SAMPLE_RATE;
use super::assets::{self, Paths};
use super::{audio, model, ytdlp};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Instant;

/// 16-bit PCM mono WAV (what the study's ffmpeg wrote) → f32.
fn read_wav(path: &Path) -> Vec<f32> {
    let b = std::fs::read(path).unwrap();
    let mut i = 12;
    while i + 8 <= b.len() {
        let len = u32::from_le_bytes(b[i + 4..i + 8].try_into().unwrap()) as usize;
        if &b[i..i + 4] == b"data" {
            return b[i + 8..(i + 8 + len).min(b.len())].chunks_exact(2).map(|s| i16::from_le_bytes([s[0], s[1]]) as f32 / 32768.0).collect();
        }
        i += 8 + len + (len & 1);
    }
    panic!("no data chunk in {}", path.display());
}

#[cfg(windows)]
fn peak_rss_mb() -> f64 {
    let pid = std::process::id();
    let out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", &format!("(Get-Process -Id {pid}).PeakWorkingSet64")])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().parse::<f64>().unwrap_or(0.0) / 1048576.0
}

#[cfg(not(windows))]
fn peak_rss_mb() -> f64 {
    0.0
}

#[tokio::test]
#[ignore = "bench: network / study audio, minutes of CPU"]
async fn parity() {
    let env = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("set {k}"));
    let paths = Paths { dir: PathBuf::from(env("PREALIGN_DIR")) };
    let inputs: Value = serde_json::from_str(&std::fs::read_to_string(env("PREALIGN_INPUTS")).unwrap()).unwrap();
    let audio_src = env("PREALIGN_AUDIO");

    let t0 = Instant::now();
    assets::ensure(&paths, |p| {
        if p.done == p.total.unwrap_or(0) {
            println!("downloaded {} ({} bytes)", p.file, p.done);
        }
    })
    .await
    .expect("assets");
    println!("assets ready in {:.1}s; on disk {} bytes", t0.elapsed().as_secs_f64(), paths.disk_bytes());
    assert!(assets::verify_model(&paths), "model checksum");

    let threads = model::threads();
    let t0 = Instant::now();
    let mut m = model::Model::load(&paths.runtime(), &paths.model(), threads).expect("model");
    let load_s = t0.elapsed().as_secs_f64();

    let mut preds = serde_json::Map::new();
    let mut speed = serde_json::Map::new();
    let ids: Vec<String> = inputs.as_object().unwrap().keys().cloned().collect();
    for id in ids {
        let t = &inputs[&id];
        let lines: Vec<String> = t["lines"].as_array().unwrap().iter().map(|l| l.as_str().unwrap().to_string()).collect();
        let (wav, download_s, decode_s) = if let Some(dir) = audio_src.strip_prefix("wav:") {
            (read_wav(&Path::new(dir).join(format!("{id}.wav"))), 0.0, 0.0)
        } else {
            let t0 = Instant::now();
            let fetched = ytdlp::fetch(&paths.ytdlp(), &paths.tmp(), t["videoId"].as_str().unwrap()).await.expect("yt-dlp");
            let dl = t0.elapsed().as_secs_f64();
            let t0 = Instant::now();
            let wav = audio::decode_16k(&fetched.path).expect("decode");
            let dec = t0.elapsed().as_secs_f64();
            drop(fetched);
            tokio::time::sleep(std::time::Duration::from_secs(2)).await; // the study's gap between downloads
            (wav, dl, dec)
        };
        let secs = wav.len() as f64 / SAMPLE_RATE as f64;
        let t0 = Instant::now();
        let out = m.align(&wav, &lines, &AtomicBool::new(false)).expect("align");
        let align_s = t0.elapsed().as_secs_f64();
        println!(
            "{id} {} {:.0}s audio: download {download_s:.1}s decode {decode_s:.2}s align {align_s:.1}s rtf {:.3}",
            t["track"].as_str().unwrap_or(""),
            secs,
            align_s / secs
        );
        let lines_json: Vec<Value> = lines
            .iter()
            .zip(&out)
            .map(|(text, l)| {
                json!({"start": l.start, "end": l.end, "text": text, "conf": l.conf,
                       "words": l.words.iter().map(|w| json!({"start": w.start, "end": w.end, "text": w.text})).collect::<Vec<_>>()})
            })
            .collect();
        preds.insert(id.clone(), json!({"text_src": t["text_src"], "lines": lines_json}));
        speed.insert(id, json!({"audio_s": secs, "download_s": download_s, "decode_s": decode_s, "align_s": align_s, "rtf": align_s / secs}));
    }
    let rss = peak_rss_mb();
    println!("threads {threads} load {load_s:.1}s peak RSS {rss:.0} MB");
    std::fs::write(env("PREALIGN_OUT"), serde_json::to_string(&Value::Object(preds)).unwrap()).unwrap();
    let speed_out = env("PREALIGN_OUT").replace(".json", ".speed.json");
    let summary = json!({"threads": threads, "load_s": load_s, "peak_rss_mb": rss, "tracks": speed});
    std::fs::write(speed_out, serde_json::to_string_pretty(&summary).unwrap()).unwrap();
}
