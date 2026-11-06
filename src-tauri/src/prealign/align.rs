//! Lyric lines + 16 kHz audio → per-line and per-word onsets, the study's
//! `bench/align.py` in Rust: a `<star>` wildcard before every line (and at the
//! end) soaks up intros and ad-libs, every piece's span comes straight off the
//! CTC path, and a line's confidence is the mean posterior of its tokens.
//! Lines may also be left out, at a price, for recordings that sing only some
//! of them (short versions, TV sizes, covers that cut a verse): see `SKIP`.

use super::ctc::{self, Emissions, SkipCost, Span};
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
/// A token whose span has at least this mean posterior was heard.
const HEARD: f32 = 0.05;
/// What leaving lines out costs (log-probability): `open` per run of lines
/// left out, `each` per letter in them. A cut is a run (a verse, the last
/// chorus); one faint line among sung ones is not, and opening a run for it
/// costs more than forcing it in. Calibrated with `bench::skips` on 84 cached
/// full songs and a short version: every setting from 400:4 to 200:5 leaves
/// no line of a full song out (each alone, 0:6, still drops 18 faint lines);
/// this one sits away from the range's edges. Uru's エイリアンズ (short ver.)
/// keeps its first 21 of 53 lines instead of being rejected.
pub const SKIP: SkipCost = SkipCost { open: 400.0, each: 4.5 };

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
/// to align (symbols only) and for lines the recording does not sing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AlignedLine {
    pub start: Option<f64>,
    pub end: Option<f64>,
    pub conf: f64,
    pub words: Vec<AlignedWord>,
    /// The recording does not sing this line: the best path went around it.
    /// (Absent in alignments cached before lines could be left out.)
    #[serde(default)]
    pub skipped: bool,
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
/// emissions of a track `samples` long. Leaving lines out costs `skip`
/// (`SKIP`; `SkipCost::NEVER`: every line is sung).
pub fn align_lines(
    em: &Emissions,
    lines: &[String],
    plan: &[Vec<Piece>],
    samples: usize,
    skip: SkipCost,
) -> Result<Vec<AlignedLine>, AlignError> {
    let dict: HashMap<char, usize> = VOCAB.iter().enumerate().filter_map(|(i, s)| Some((single(s)?, i))).collect();
    let star = VOCAB.len();

    let mut targets = Vec::new();
    let mut owner: Vec<Option<(usize, usize)>> = Vec::new();
    // Where each line's star is, and the closing one.
    let mut stars = Vec::new();
    for (li, ps) in plan.iter().enumerate() {
        stars.push(targets.len());
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
    stars.push(targets.len());
    targets.push(star);
    owner.push(None);

    // Lines are left out by going from a star straight to a later one.
    let spans = ctc::align(em, &targets, &stars, skip, BLANK).ok_or(AlignError::TooShort)?;
    let sec_per_frame = samples as f64 / em.frames as f64 / SAMPLE_RATE as f64;

    // (line, piece) -> its tokens' spans, in order
    let mut by_piece: HashMap<(usize, usize), Vec<Span>> = HashMap::new();
    for (sp, key) in spans.iter().zip(&owner) {
        if let (Some(sp), Some(key)) = (sp, key) {
            by_piece.entry(*key).or_default().push(*sp);
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
                    let at = by_piece.get(&(li, pi)).map(|s| (onset(s) as f64 * sec_per_frame, s[s.len() - 1].end as f64 * sec_per_frame));
                    (at, chars[p.pos..p.pos + p.len].iter().collect())
                })
                .collect();
            fill(&mut words);
            let scores: Vec<f32> = (0..ps.len()).flat_map(|pi| by_piece.get(&(li, pi)).into_iter().flatten().map(|s| s.score)).collect();
            let conf = if scores.is_empty() { 0.0 } else { scores.iter().map(|&s| s as f64).sum::<f64>() / scores.len() as f64 };
            let words: Vec<AlignedWord> =
                words.into_iter().filter_map(|(at, text)| at.map(|(start, end)| AlignedWord { start, end, text })).collect();
            let skipped = (stars[li] + 1..stars[li + 1]).any(|k| spans[k].is_none());
            AlignedLine { start: words.first().map(|w| w.start), end: words.last().map(|w| w.end), conf, words, skipped }
        })
        .collect())
}

/// A piece starts at its first token the model actually heard, not where the
/// path happened to enter it: a word's first span can stretch back over
/// silence. A piece with no heard token keeps its first span's start.
fn onset(spans: &[Span]) -> usize {
    spans.iter().find(|s| s.score >= HEARD).unwrap_or(&spans[0]).start
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

    /// Emissions that spell `text` one letter per `per` frames, with blanks between
    /// (spaces are blanks too). Any other letter is near impossible, as in real posteriors.
    fn spelled(text: &str, per: usize) -> Emissions {
        let v = 32;
        let mut data = Vec::new();
        let mut frames = 0;
        let mut row = |id: usize| {
            let mut r = vec![(0.001f32).ln(); v];
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
        let out = align_lines(&em, &lines, &super::super::romanize::plan(&lines), samples, SKIP).unwrap();
        let w = &out[0].words;
        assert_eq!(w.len(), 2);
        assert_eq!(w[0].text, "hey");
        assert!(w[0].start < w[1].start);
        // "you" starts after "hey " (4 symbols × 3 frames, 20 ms each), give or take the star.
        assert!((w[1].start - 0.24).abs() <= 0.06, "{}", w[1].start);
        assert!(out[0].conf > 0.5);
    }

    /// 53 different made-up lines ("kamito resuno haryu"), and the plan for them.
    fn song() -> (Vec<String>, Vec<Vec<Piece>>) {
        const SYL: [&str; 16] = ["ka", "mi", "to", "re", "su", "no", "ha", "yu", "se", "ri", "po", "de", "ga", "bu", "ne", "zo"];
        let mut x: u32 = 7;
        let mut syl = || {
            x = x.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            SYL[(x >> 16) as usize % SYL.len()]
        };
        let lines: Vec<String> = (0..53).map(|_| (0..3).map(|_| format!("{}{}{}", syl(), syl(), syl())).collect::<Vec<_>>().join(" ")).collect();
        let plan = super::super::romanize::plan(&lines);
        (lines, plan)
    }

    /// A recording that sings `which` of `lines`, in order, with pauses
    /// between lines and some music after.
    fn recording(lines: &[String], which: impl Iterator<Item = usize>) -> Emissions {
        let pause = " ".repeat(4);
        let sung: Vec<&str> = which.map(|i| lines[i].as_str()).collect();
        spelled(&format!("{pause}{}{}", sung.join(&pause), pause.repeat(20)), 2)
    }

    fn sung_lines(out: &[AlignedLine]) -> Vec<usize> {
        (0..out.len()).filter(|&i| !out[i].skipped).collect()
    }

    #[test]
    fn a_short_version_aligns_the_lines_it_sings_and_leaves_out_the_rest() {
        let (lines, plan) = song();
        let em = recording(&lines, 0..15);
        let out = align_lines(&em, &lines, &plan, em.frames * HOP, SKIP).unwrap();
        assert_eq!(sung_lines(&out), (0..15).collect::<Vec<_>>());
        assert!(out[..15].iter().all(|l| l.conf > 0.5 && l.start.is_some()));
        assert!(out[..15].windows(2).all(|w| w[0].end <= w[1].start), "in order");
        assert!(out[15..].iter().all(|l| l.start.is_none() && l.words.is_empty()));
        // Forced through every line instead, the track is lost.
        let forced = align_lines(&em, &lines, &plan, em.frames * HOP, SkipCost::NEVER).unwrap();
        assert!(forced.iter().all(|l| !l.skipped));
        assert!(super::super::reject::track_rejected(&forced));
        assert!(!super::super::reject::track_rejected(&out));
    }

    #[test]
    fn a_full_song_is_aligned_as_before() {
        let (lines, plan) = song();
        let em = recording(&lines, 0..53);
        let out = align_lines(&em, &lines, &plan, em.frames * HOP, SKIP).unwrap();
        assert_eq!(out, align_lines(&em, &lines, &plan, em.frames * HOP, SkipCost::NEVER).unwrap());
        assert_eq!(sung_lines(&out).len(), 53);
    }

    #[test]
    fn a_cut_verse_is_left_out() {
        let (lines, plan) = song();
        let em = recording(&lines, (0..20).chain(30..53));
        let out = align_lines(&em, &lines, &plan, em.frames * HOP, SKIP).unwrap();
        assert_eq!(sung_lines(&out), (0..20).chain(30..53).collect::<Vec<_>>());
        assert!(out[19].end <= out[30].start);
    }

    #[test]
    fn a_symbol_only_line_is_never_left_out() {
        // Nothing to hear, nothing to leave out: it stays a line with no timing.
        let lines: Vec<String> = ["hey you", "♪", "hey me"].map(String::from).to_vec();
        let em = spelled("    hey you    hey me    ", 2);
        let out = align_lines(&em, &lines, &super::super::romanize::plan(&lines), em.frames * HOP, SKIP).unwrap();
        assert_eq!(out.iter().map(|l| l.skipped).collect::<Vec<_>>(), [false, false, false]);
        assert_eq!(out[1].start, None);
    }

    #[test]
    fn alignments_cached_before_skipping_still_load() {
        let old: AlignedLine = serde_json::from_str(r#"{"start": 1.0, "end": 2.0, "conf": 0.5, "words": []}"#).unwrap();
        assert!(!old.skipped);
    }

    #[test]
    fn a_word_starts_at_its_first_heard_letter() {
        // A first letter stretched over near-silence (score 0.01) does not start
        // the word; the first heard one (0.2) does.
        let span = |start, score| Span { start, end: start + 1, score };
        assert_eq!(onset(&[span(3, 0.01), span(5, 0.2)]), 5);
        assert_eq!(onset(&[span(3, 0.01), span(5, 0.02)]), 3, "nothing heard: the first span");
    }

    #[test]
    fn symbol_only_pieces_take_the_next_onset() {
        let mut words = vec![(None, "1".to_string()), (Some((2.0, 2.5)), "a".to_string()), (None, "!".to_string())];
        fill(&mut words);
        assert_eq!(words[0].0, Some((2.0, 2.0)));
        assert_eq!(words[2].0, None);
    }
}
