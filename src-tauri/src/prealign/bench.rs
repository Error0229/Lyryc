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

/// (current, peak) working set in MB.
#[cfg(windows)]
fn rss_mb() -> (f64, f64) {
    let pid = std::process::id();
    let out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", &format!("$p = Get-Process -Id {pid}; \"$($p.WorkingSet64) $($p.PeakWorkingSet64)\"")])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    let mut it = text.split_whitespace().map(|v| v.parse::<f64>().unwrap_or(0.0) / 1048576.0);
    (it.next().unwrap_or(0.0), it.next().unwrap_or(0.0))
}

#[cfg(not(windows))]
fn rss_mb() -> (f64, f64) {
    (0.0, 0.0)
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
    println!("after model load: RSS {:.0} MB (peak {:.0})", rss_mb().0, rss_mb().1);

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
        let (now, peak) = rss_mb();
        println!("  RSS {now:.0} MB, peak {peak:.0} MB");
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
    let rss = rss_mb().1;
    println!("threads {threads} load {load_s:.1}s peak RSS {rss:.0} MB");
    std::fs::write(env("PREALIGN_OUT"), serde_json::to_string(&Value::Object(preds)).unwrap()).unwrap();
    let speed_out = env("PREALIGN_OUT").replace(".json", ".speed.json");
    let summary = json!({"threads": threads, "load_s": load_s, "peak_rss_mb": rss, "tracks": speed});
    std::fs::write(speed_out, serde_json::to_string_pretty(&summary).unwrap()).unwrap();
}

/// `reject::apply` over stored alignments, as the app lays them over lyrics.
///
/// ```text
/// PREALIGN_TRACKS=<JSON [{lines: [LyricLine], real_timing, aligned: [AlignedLine]}]>
/// PREALIGN_OUT=<JSON [applied lines or null]>
/// cargo test --lib prealign::bench::replay -- --ignored
/// ```
#[test]
#[ignore = "bench: needs stored alignments"]
fn replay() {
    use super::align::AlignedLine;
    use crate::types::LyricLine;
    #[derive(serde::Deserialize)]
    struct Track {
        lines: Vec<LyricLine>,
        real_timing: bool,
        aligned: Vec<AlignedLine>,
    }
    let env = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("set {k}"));
    let tracks: Vec<Track> = serde_json::from_str(&std::fs::read_to_string(env("PREALIGN_TRACKS")).unwrap()).unwrap();
    let out: Vec<_> = tracks.iter().map(|t| super::reject::apply(&t.lines, t.real_timing, &t.aligned)).collect();
    std::fs::write(env("PREALIGN_OUT"), serde_json::to_string(&out).unwrap()).unwrap();
}

/// Lines left out vs kept, across skip costs, on the same emissions.
///
/// ```text
/// PREALIGN_DIR=<app prealign dir: model, runtime, yt-dlp (read only)>
/// PREALIGN_TRACKS=<JSON [{id, lines: [LyricLine], real_timing}]>
/// PREALIGN_EM=<dir: <id>.em emissions, computed once (downloads into <dir>/tmp)>
/// PREALIGN_COSTS=<open:each,...> e.g. inf:0,40:3   PREALIGN_OUT=<JSON per track, per cost>
/// cargo test --release --lib prealign::bench::skips -- --ignored --nocapture
/// ```
#[tokio::test]
#[ignore = "bench: network, minutes of CPU"]
async fn skips() {
    use super::align::{align_lines, AlignedLine};
    use super::ctc::{Emissions, SkipCost};
    use crate::types::LyricLine;
    let env = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("set {k}"));
    let paths = Paths { dir: PathBuf::from(env("PREALIGN_DIR")) };
    let em_dir = PathBuf::from(env("PREALIGN_EM"));
    let tracks: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(env("PREALIGN_TRACKS")).unwrap()).unwrap();
    let costs: Vec<SkipCost> = env("PREALIGN_COSTS")
        .split(',')
        .map(|c| {
            let (open, each) = c.split_once(':').unwrap();
            SkipCost { open: open.parse().unwrap(), each: each.parse().unwrap() }
        })
        .collect();
    let mut model: Option<model::Model> = None;
    let mut out = Vec::new();
    for t in &tracks {
        let id = t["id"].as_str().unwrap();
        let lines: Vec<LyricLine> = serde_json::from_value(t["lines"].clone()).unwrap();
        let real_timing = t["real_timing"].as_bool().unwrap();
        let file = em_dir.join(format!("{id}.em"));
        let (samples, em) = match std::fs::read(&file) {
            Ok(b) => {
                let n = |i: usize| u64::from_le_bytes(b[i * 8..i * 8 + 8].try_into().unwrap()) as usize;
                let data = b[24..].chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect();
                (n(0), Emissions { data, frames: n(1), vocab: n(2) })
            }
            Err(_) => {
                let fetched = match ytdlp::fetch(&paths.ytdlp(), &em_dir.join("tmp"), id).await {
                    Ok(f) => f,
                    Err(e) => {
                        println!("{id}: download failed: {e}");
                        out.push(json!({"id": id, "error": e}));
                        continue;
                    }
                };
                let wav = audio::decode_16k(&fetched.path).expect("decode");
                drop(fetched);
                let m = model.get_or_insert_with(|| model::Model::load(&paths.runtime(), &paths.model(), model::threads()).expect("model"));
                let em = m.emissions(&wav, &AtomicBool::new(false)).expect("emissions");
                let mut b = Vec::new();
                for v in [wav.len(), em.frames, em.vocab] {
                    b.extend((v as u64).to_le_bytes());
                }
                b.extend(em.data.iter().flat_map(|v| v.to_le_bytes()));
                std::fs::write(&file, b).unwrap();
                (wav.len(), em)
            }
        };
        let texts: Vec<String> = lines.iter().map(|l| l.text.clone()).collect();
        let plan = super::romanize::plan(&texts);
        let mut runs = Vec::new();
        for &cost in &costs {
            let aligned = align_lines(&em, &texts, &plan, samples, cost).expect("align");
            let applied = super::reject::apply(&lines, real_timing, &aligned);
            let sung: Vec<&AlignedLine> = aligned.iter().filter(|a| !a.skipped).collect();
            // Shown lines whose words are the aligner's own.
            let kept = applied.as_ref().map(|out| {
                out.iter()
                    .zip(&sung)
                    .filter(|(l, al)| !al.words.is_empty() && l.words.as_ref().is_some_and(|w| w[0].time == al.words[0].start))
                    .count()
            });
            let skipped: Vec<usize> = (0..aligned.len()).filter(|&i| aligned[i].skipped).collect();
            println!(
                "{id} cost {}:{}: {} lines, skipped {:?}, track {}, kept {:?}",
                cost.open,
                cost.each,
                lines.len(),
                skipped,
                if applied.is_some() { "accepted" } else { "rejected" },
                kept
            );
            runs.push(json!({"cost": format!("{}:{}", cost.open, cost.each), "skipped": skipped, "accepted": applied.is_some(), "shown": applied.as_ref().map(Vec::len),
                             "kept": kept, "aligned": aligned}));
        }
        out.push(json!({"id": id, "lines": lines.len(), "real_timing": real_timing, "runs": runs}));
        std::fs::write(env("PREALIGN_OUT"), serde_json::to_string(&out).unwrap()).unwrap();
    }
}
