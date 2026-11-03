//! Lyric lines + 16 kHz audio → per-line and per-word onsets, the study's
//! `bench/align.py` in Rust: a `<star>` wildcard before every line (and at the
//! end) soaks up intros and ad-libs, every piece's span comes straight off the
//! CTC path, and a line's confidence is the mean posterior of its tokens.

use super::ctc::{self, Emissions};
use super::romanize::Piece;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

pub const SAMPLE_RATE: usize = 16_000;
/// wav2vec2's frame hop in samples (20 ms).
const HOP: usize = 320;
/// The shortest input the model's conv stack accepts.
const MIN_SAMPLES: usize = 400;
const BLANK: usize = 0;

/// The MMS forced-aligner's character vocabulary (`vocab.json`); `<star>` is appended after it.
pub const VOCAB: [&str; 31] = [
    "<blank>", "<pad>", "</s>", "<unk>", "a", "i", "e", "n", "o", "u", "t", "s", "r", "m", "k", "l", "d", "g", "h", "y", "b",
    "p", "w", "c", "v", "j", "z", "f", "'", "q", "x",
];

/// The study's production chunking: 15 s windows with 3 s of context each side.
#[derive(Clone, Copy, Debug)]
pub struct Chunking {
    pub chunk_s: usize,
    pub ctx_s: usize,
}

impl Default for Chunking {
    fn default() -> Self {
        Chunking { chunk_s: 15, ctx_s: 3 }
    }
}

#[derive(Debug)]
pub enum AlignError {
    Cancelled,
    TooShort,
    Model(String),
}

impl std::fmt::Display for AlignError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlignError::Cancelled => write!(f, "cancelled"),
            AlignError::TooShort => write!(f, "audio too short for the lyrics"),
            AlignError::Model(e) => write!(f, "model: {e}"),
        }
    }
}

/// A word (Latin word or one CJK character) and its onset/offset in seconds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AlignedWord {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

/// One lyric line as aligned. `start`/`end` are None for lines with nothing
/// to align (symbols only).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AlignedLine {
    pub start: Option<f64>,
    pub end: Option<f64>,
    pub conf: f64,
    pub words: Vec<AlignedWord>,
}

/// Log-posteriors for the whole track plus the `<star>` column (log p = 0).
/// `logits(x)` runs the model on normalized samples and returns `(frames, vocab, data)`.
pub fn emissions(
    wav: &[f32],
    chunking: Chunking,
    cancel: &AtomicBool,
    mut logits: impl FnMut(&[f32]) -> Result<(usize, usize, Vec<f32>), String>,
) -> Result<Emissions, AlignError> {
    let n = wav.len();
    let (hop, ctx) = (chunking.chunk_s * SAMPLE_RATE, chunking.ctx_s * SAMPLE_RATE);
    let mut data = Vec::new();
    let mut frames = 0;
    let mut vocab = 0;
    let mut s = 0;
    while s < n {
        if cancel.load(Ordering::Relaxed) {
            return Err(AlignError::Cancelled);
        }
        let a = s.saturating_sub(ctx);
        let e = (s + hop).min(n);
        let x = normalize(&wav[a..(e + ctx).min(n)]);
        if x.len() < MIN_SAMPLES {
            break;
        }
        let (t, v, mut lp) = logits(&x).map_err(AlignError::Model)?;
        log_softmax_rows(&mut lp, v);
        let lo = (s - a) / HOP;
        let keep = if e == n { t.saturating_sub(lo) } else { ((e - s) / HOP).min(t.saturating_sub(lo)) };
        for f in lo..lo + keep {
            data.extend_from_slice(&lp[f * v..(f + 1) * v]);
            data.push(0.0); // <star>
        }
        frames += keep;
        vocab = v + 1;
        s += hop;
    }
    if frames == 0 {
        return Err(AlignError::TooShort);
    }
    Ok(Emissions { data, frames, vocab })
}

/// Zero mean, unit (sample) variance, as wav2vec2's feature extractor does.
fn normalize(x: &[f32]) -> Vec<f32> {
    let n = x.len() as f64;
    let mean = x.iter().map(|&v| v as f64).sum::<f64>() / n;
    let var = x.iter().map(|&v| (v as f64 - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0);
    let scale = 1.0 / (var.sqrt() + 1e-7);
    x.iter().map(|&v| ((v as f64 - mean) * scale) as f32).collect()
}

fn log_softmax_rows(data: &mut [f32], v: usize) {
    for row in data.chunks_mut(v) {
        let max = row.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let lse = max + row.iter().map(|&x| (x - max).exp()).sum::<f32>().ln();
        row.iter_mut().for_each(|x| *x -= lse);
    }
}

/// Force-align `lines` (cut into `plan`, see `romanize::plan`) to the
/// emissions of a track `samples` long.
pub fn align_lines(em: &Emissions, lines: &[String], plan: &[Vec<Piece>], samples: usize) -> Result<Vec<AlignedLine>, AlignError> {
    let dict: HashMap<char, usize> = VOCAB.iter().enumerate().filter_map(|(i, s)| Some((single(s)?, i))).collect();
    let star = VOCAB.len();

    let mut targets = Vec::new();
    let mut owner: Vec<Option<(usize, usize)>> = Vec::new();
    for (li, ps) in plan.iter().enumerate() {
        targets.push(star);
        owner.push(None);
        for (pi, p) in ps.iter().enumerate() {
            for c in p.rom.chars() {
                if let Some(&id) = dict.get(&c) {
                    targets.push(id);
                    owner.push(Some((li, pi)));
                }
            }
        }
    }
    targets.push(star);
    owner.push(None);

    let spans = ctc::align(em, &targets, BLANK).ok_or(AlignError::TooShort)?;
    let sec_per_frame = samples as f64 / em.frames as f64 / SAMPLE_RATE as f64;

    // (line, piece) -> first frame, end frame, token scores
    let mut by_piece: HashMap<(usize, usize), (usize, usize, Vec<f32>)> = HashMap::new();
    for (k, sp) in spans.iter().enumerate() {
        if let Some(key) = owner[k] {
            let e = by_piece.entry(key).or_insert((sp.start, sp.end, Vec::new()));
            e.1 = sp.end;
            e.2.push(sp.score);
        }
    }

    Ok(lines
        .iter()
        .zip(plan)
        .enumerate()
        .map(|(li, (text, ps))| {
            let chars: Vec<char> = text.chars().collect();
            let mut words: Vec<(Option<(f64, f64)>, String)> = ps
                .iter()
                .enumerate()
                .map(|(pi, p)| {
                    let at = by_piece.get(&(li, pi)).map(|s| (s.0 as f64 * sec_per_frame, s.1 as f64 * sec_per_frame));
                    (at, chars[p.pos..p.pos + p.len].iter().collect())
                })
                .collect();
            fill(&mut words);
            let scores: Vec<f32> = (0..ps.len()).flat_map(|pi| by_piece.get(&(li, pi)).map(|s| s.2.clone()).unwrap_or_default()).collect();
            let conf = if scores.is_empty() { 0.0 } else { scores.iter().map(|&s| s as f64).sum::<f64>() / scores.len() as f64 };
            let words: Vec<AlignedWord> =
                words.into_iter().filter_map(|(at, text)| at.map(|(start, end)| AlignedWord { start, end, text })).collect();
            AlignedLine { start: words.first().map(|w| w.start), end: words.last().map(|w| w.end), conf, words }
        })
        .collect())
}

fn single(s: &str) -> Option<char> {
    let mut it = s.chars();
    let c = it.next()?;
    it.next().is_none().then_some(c)
}

/// Pieces with no tokens (digits, symbols) take the next timed piece's start.
fn fill(words: &mut [(Option<(f64, f64)>, String)]) {
    let mut next: Option<f64> = None;
    for w in words.iter_mut().rev() {
        match w.0 {
            Some((start, _)) => next = Some(start),
            None => w.0 = next.map(|t| (t, t)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_keep_only_their_own_frames() {
        // 40 s of audio in 15 s chunks with 3 s context: each window's frames are
        // trimmed back to the chunk, so the total matches one pass over the track.
        let wav = vec![0.1f32; 40 * SAMPLE_RATE];
        let mut windows = Vec::new();
        let em = emissions(&wav, Chunking::default(), &AtomicBool::new(false), |x| {
            windows.push(x.len());
            let t = (x.len() - MIN_SAMPLES) / HOP + 1;
            Ok((t, 31, vec![0.0; t * 31]))
        })
        .unwrap();
        assert_eq!(windows, vec![18 * SAMPLE_RATE, 21 * SAMPLE_RATE, 13 * SAMPLE_RATE]);
        assert_eq!(em.vocab, 32);
        assert!((em.frames as i64 - (40 * 50) as i64).abs() <= 2, "{}", em.frames);
        // Rows are log-softmaxed and the star column is 0.
        assert!((em.data[0] - (1.0f32 / 31.0).ln()).abs() < 1e-5);
        assert_eq!(em.data[31], 0.0);
    }

    #[test]
    fn cancel_stops_between_chunks() {
        let wav = vec![0.0f32; 20 * SAMPLE_RATE];
        let r = emissions(&wav, Chunking::default(), &AtomicBool::new(true), |_| unreachable!());
        assert!(matches!(r, Err(AlignError::Cancelled)));
    }

    /// Emissions that spell `text` one letter per `per` frames, with blanks between.
    fn spelled(text: &str, per: usize) -> Emissions {
        let v = 32;
        let mut data = Vec::new();
        let mut frames = 0;
        let mut row = |id: usize| {
            let mut r = vec![(0.01f32).ln(); v];
            r[id] = (0.9f32).ln();
            r[31] = 0.0;
            data.extend(r);
            frames += 1;
        };
        for c in text.chars() {
            let id = VOCAB.iter().position(|s| *s == c.to_string()).unwrap_or(0);
            for _ in 0..per {
                row(id);
            }
            row(0);
        }
        Emissions { data, frames, vocab: v }
    }

    #[test]
    fn words_land_on_their_letters() {
        let em = spelled("hey you", 2);
        let samples = em.frames * HOP;
        let lines = ["hey you".to_string()];
        let out = align_lines(&em, &lines, &super::super::romanize::plan(&lines), samples).unwrap();
        let w = &out[0].words;
        assert_eq!(w.len(), 2);
        assert_eq!(w[0].text, "hey");
        assert!(w[0].start < w[1].start);
        // "you" starts after "hey " (4 symbols × 3 frames, 20 ms each), give or take the star.
        assert!((w[1].start - 0.24).abs() <= 0.06, "{}", w[1].start);
        assert!(out[0].conf > 0.5);
    }

    #[test]
    fn symbol_only_pieces_take_the_next_onset() {
        let mut words = vec![(None, "1".to_string()), (Some((2.0, 2.5)), "a".to_string()), (None, "!".to_string())];
        fill(&mut words);
        assert_eq!(words[0].0, Some((2.0, 2.0)));
        assert_eq!(words[2].0, None);
    }
}
