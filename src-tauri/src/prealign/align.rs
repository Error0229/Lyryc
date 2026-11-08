//! Lyric lines + 16 kHz audio → per-line and per-word onsets with
//! echora-kara-300m (`research/prealign/bench/run_echo.py` in Rust): the
//! encoder runs over one continuous lattice, the text head scores every frame
//! against every token, and `decode` places the tokens. Each romanized piece
//! (a Latin word, one CJK character) is one token; a line's confidence is the
//! mean posterior of its letters over their spans.

use super::decode::{self, Tokens, VOCAB};
use super::head::HIDDEN;
use super::romanize::Piece;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};

pub const SAMPLE_RATE: usize = 16_000;
/// wav2vec2's frame hop in samples (20 ms).
const HOP: usize = 320;
/// The shortest input the model's conv stack accepts.
const MIN_SAMPLES: usize = 400;
/// The centre of a frame's 400-sample receptive field, in samples.
const FRAME_CENTRE: f64 = 199.5;

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
    NoText,
    TooLong,
    Model(String),
}

impl std::fmt::Display for AlignError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlignError::Cancelled => write!(f, "cancelled"),
            AlignError::TooShort => write!(f, "audio too short for the lyrics"),
            AlignError::NoText => write!(f, "nothing in the lyrics to align"),
            AlignError::TooLong => write!(f, "track too long to align"),
            AlignError::Model(e) => write!(f, "model: {e}"),
        }
    }
}

/// Longest audio aligned: the encoder's hidden state alone is 4 KB a frame (~180 MB at 15 min).
pub const MAX_SECONDS: usize = 15 * 60;
/// Frames × tokens. Per cell: the text head's `[T, K]` output is 4 bytes; the
/// decoder keeps 2 path bits per (frame, state), with ~10 states a token
/// (~4 letters at floors of 2–3, plus a blank), so ~2.5 bytes. At the cap that
/// is ~160 + ~100 MB on top of the ~1 GB baseline. A 4-minute song is ~6M.
const MAX_CELLS: usize = 40_000_000;

/// Refuse, before any model work, a job whose memory would run away.
pub fn check_size(samples: usize, tokens: usize) -> Result<(), AlignError> {
    let frames = samples / HOP;
    if samples > MAX_SECONDS * SAMPLE_RATE || frames.saturating_mul(tokens) > MAX_CELLS {
        return Err(AlignError::TooLong);
    }
    Ok(())
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

/// The encoder's view of a whole track: `[frames, 30]` log-probabilities and
/// `[frames, 1024]` last hidden states, row-major.
pub struct Lattice {
    pub frames: usize,
    pub logp: Vec<f32>,
    pub hidden: Vec<f32>,
}

/// Run the encoder over 15 s windows with 3 s of context, keeping each
/// window's own frames: one continuous lattice of `⌊(N − 400)/320⌋ + 1` frames.
/// `encode(x)` runs it on normalized samples and returns `(frames, logits, hidden)`.
pub fn lattice(
    wav: &[f32],
    chunking: Chunking,
    cancel: &AtomicBool,
    mut encode: impl FnMut(&[f32]) -> Result<(usize, Vec<f32>, Vec<f32>), String>,
) -> Result<Lattice, AlignError> {
    let n = wav.len();
    let (hop, ctx) = (chunking.chunk_s * SAMPLE_RATE, chunking.ctx_s * SAMPLE_RATE);
    let mut lat = Lattice { frames: 0, logp: Vec::new(), hidden: Vec::new() };
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
        let (t, mut logits, hidden) = encode(&x).map_err(AlignError::Model)?;
        log_softmax_rows(&mut logits);
        let lo = (s - a) / HOP;
        let keep = if e == n { t.saturating_sub(lo) } else { ((e - s) / HOP).min(t.saturating_sub(lo)) };
        lat.logp.extend_from_slice(&logits[lo * VOCAB..(lo + keep) * VOCAB]);
        lat.hidden.extend_from_slice(&hidden[lo * HIDDEN..(lo + keep) * HIDDEN]);
        lat.frames += keep;
        s += hop;
    }
    if lat.frames == 0 {
        return Err(AlignError::TooShort);
    }
    Ok(lat)
}

/// Zero mean, unit variance, as wav2vec2's feature extractor does.
fn normalize(x: &[f32]) -> Vec<f32> {
    let n = x.len() as f64;
    let mean = x.iter().map(|&v| v as f64).sum::<f64>() / n;
    let var = x.iter().map(|&v| (v as f64 - mean).powi(2)).sum::<f64>() / n;
    let scale = 1.0 / (var + 1e-7).sqrt();
    x.iter().map(|&v| ((v as f64 - mean) * scale) as f32).collect()
}

fn log_softmax_rows(data: &mut [f32]) {
    for row in data.chunks_mut(VOCAB) {
        let max = row.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let lse = row.iter().map(|&x| (x - max).exp()).sum::<f32>().ln();
        row.iter_mut().for_each(|x| *x = *x - max - lse);
    }
}

/// echora's id for a romanized letter: a..z = 1..26, `'` = 27.
fn letter_id(c: char) -> Option<u8> {
    match c {
        'a'..='z' => Some(c as u8 - b'a' + 1),
        '\'' => Some(27),
        _ => None,
    }
}

/// The decoder's tokens for a song's pieces: one per piece with letters.
/// Pieces without (digits, symbols) and lines without any are left out.
pub struct Labels {
    pub tokens: Tokens,
    /// Each token's line number, counting only lines with tokens.
    pub line_of: Vec<usize>,
    /// Per line, per piece: its token.
    piece_token: Vec<Vec<Option<usize>>>,
}

pub fn labels(plan: &[Vec<Piece>]) -> Labels {
    let mut tokens = Tokens { ids: Vec::new(), lines: Vec::new() };
    let mut line_of = Vec::new();
    let mut piece_token = Vec::new();
    for pieces in plan {
        let first = tokens.ids.len();
        let row = pieces
            .iter()
            .map(|p| {
                let ids: Vec<u8> = p.rom.chars().filter_map(letter_id).collect();
                (!ids.is_empty()).then(|| {
                    tokens.ids.push(ids);
                    line_of.push(tokens.lines.len());
                    tokens.ids.len() - 1
                })
            })
            .collect();
        piece_token.push(row);
        if tokens.ids.len() > first {
            tokens.lines.push((first, tokens.ids.len()));
        }
    }
    Labels { tokens, line_of, piece_token }
}

/// Place `labels` (from `plan`, cut from `lines`) on the lattice of `wav`.
/// `share`: the text head's `[frames, tokens]` output.
pub fn align_lines(
    lat: &Lattice,
    share: &[f32],
    wav: &[f32],
    lines: &[String],
    plan: &[Vec<Piece>],
    labels: &Labels,
) -> Result<Vec<AlignedLine>, AlignError> {
    let frames = lat.frames;
    let delta = wav.len() as f64 / SAMPLE_RATE as f64 / frames as f64;
    let activity = decode::activity(wav, frames);
    let out = decode::decode(&lat.logp, &activity, share, &labels.tokens, delta).ok_or(AlignError::TooShort)?;
    let clock: Vec<f64> = (0..frames).map(|t| (FRAME_CENTRE + (HOP * t) as f64) / SAMPLE_RATE as f64).collect();
    let sec = |f: usize| decode::seconds(f, &clock, delta);
    let posterior = letter_posteriors(lat, &out.letters, &labels.tokens);
    let first_letter: Vec<usize> = labels.tokens.ids.iter().scan(0, |j, t| Some(std::mem::replace(j, *j + t.len()))).collect();

    Ok(lines
        .iter()
        .zip(plan)
        .zip(&labels.piece_token)
        .map(|((text, pieces), toks)| {
            let chars: Vec<char> = text.chars().collect();
            let mut words: Vec<(Option<(f64, f64)>, String)> = pieces
                .iter()
                .zip(toks)
                .map(|(p, k)| (k.map(|k| (sec(out.tokens[k].0), sec(out.tokens[k].1))), chars[p.pos..p.pos + p.len].iter().collect()))
                .collect();
            fill(&mut words);
            let letters: Vec<f64> = toks.iter().flatten().flat_map(|&k| &posterior[first_letter[k]..][..labels.tokens.ids[k].len()]).copied().collect();
            let conf = if letters.is_empty() { 0.0 } else { letters.iter().sum::<f64>() / letters.len() as f64 };
            let words: Vec<AlignedWord> =
                words.into_iter().filter_map(|(at, text)| at.map(|(start, end)| AlignedWord { start, end, text })).collect();
            AlignedLine { start: words.first().map(|w| w.start), end: words.last().map(|w| w.end), conf, words }
        })
        .collect())
}

/// Each letter's mean posterior over its span.
fn letter_posteriors(lat: &Lattice, spans: &[decode::Span], tokens: &Tokens) -> Vec<f64> {
    let ids = tokens.ids.iter().flatten();
    spans
        .iter()
        .zip(ids)
        .map(|(&(s, e), &id)| (s..e).map(|t| (lat.logp[t * VOCAB + id as usize] as f64).exp()).sum::<f64>() / (e - s) as f64)
        .collect()
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
    use super::super::romanize;
    use super::*;

    #[test]
    fn size_guard_takes_songs_and_refuses_runaways() {
        let secs = |s: usize| s * SAMPLE_RATE;
        assert!(check_size(secs(240), 600).is_ok()); // a dense 4-minute song
        assert!(check_size(secs(14 * 60), 800).is_ok()); // a long one
        assert!(matches!(check_size(secs(60 * 60), 50), Err(AlignError::TooLong))); // an hour-long mix
        assert!(matches!(check_size(secs(10 * 60), 3000), Err(AlignError::TooLong))); // wrong, huge lyrics
    }

    #[test]
    fn chunks_keep_only_their_own_frames() {
        // 40 s of audio in 15 s chunks with 3 s context: each window's frames are
        // trimmed back to the chunk, so the total matches one pass over the track.
        let wav = vec![0.1f32; 40 * SAMPLE_RATE];
        let mut windows = Vec::new();
        let lat = lattice(&wav, Chunking::default(), &AtomicBool::new(false), |x| {
            windows.push(x.len());
            let t = (x.len() - MIN_SAMPLES) / HOP + 1;
            Ok((t, vec![0.0; t * VOCAB], vec![1.0; t * HIDDEN]))
        })
        .unwrap();
        assert_eq!(windows, vec![18 * SAMPLE_RATE, 21 * SAMPLE_RATE, 13 * SAMPLE_RATE]);
        assert_eq!(lat.frames, (wav.len() - MIN_SAMPLES) / HOP + 1);
        assert_eq!((lat.logp.len(), lat.hidden.len()), (lat.frames * VOCAB, lat.frames * HIDDEN));
        // Rows are log-softmaxed.
        assert!((lat.logp[0] - (1.0f32 / 30.0).ln()).abs() < 1e-5);
    }

    #[test]
    fn cancel_stops_between_chunks() {
        let wav = vec![0.0f32; 20 * SAMPLE_RATE];
        let r = lattice(&wav, Chunking::default(), &AtomicBool::new(true), |_| unreachable!());
        assert!(matches!(r, Err(AlignError::Cancelled)));
    }

    #[test]
    fn tokens_are_pieces_with_letters_and_lines_count_only_those_with_tokens() {
        let lines: Vec<String> = ["don't 2 go", "♪", "go"].map(String::from).to_vec();
        let plan = romanize::plan(&lines);
        let l = labels(&plan);
        assert_eq!(l.tokens.ids, vec![vec![4, 15, 14, 27, 20], vec![7, 15], vec![7, 15]], "apostrophe kept");
        assert_eq!(l.tokens.lines, vec![(0, 2), (2, 3)]);
        assert_eq!(l.line_of, vec![0, 0, 1]);
        assert_eq!(l.piece_token, vec![vec![Some(0), None, Some(1)], vec![], vec![Some(2)]]);
    }

    /// A lattice that spells `text` one letter per `per` frames, blank between
    /// letters' runs and for spaces; and audio that is loud where letters are.
    fn spelled(text: &str, per: usize) -> (Lattice, Vec<f32>) {
        let mut logp = Vec::new();
        let mut row = |id: usize| logp.extend((0..VOCAB).map(|c| if c == id { 0.9f32.ln() } else { 0.003f32.ln() }));
        for c in text.chars() {
            let id = letter_id(c).map_or(decode::BLANK, usize::from);
            (0..per).for_each(|_| row(id));
            row(decode::BLANK);
        }
        let frames = logp.len() / VOCAB;
        let wav: Vec<f32> = (0..frames * HOP + MIN_SAMPLES).map(|i| if logp[(i / HOP).min(frames - 1) * VOCAB + decode::BLANK] > -1.0 { 0.0 } else { 0.3 }).collect();
        (Lattice { frames, logp, hidden: vec![] }, wav)
    }

    #[test]
    fn words_land_on_their_letters() {
        let lines = ["hey you".to_string(), "2 me".to_string()];
        let plan = romanize::plan(&lines);
        let l = labels(&plan);
        let (lat, wav) = spelled("   hey you    me   ", 3);
        let share = vec![(1.0f32 / 3.0).ln(); lat.frames * 3];
        let out = align_lines(&lat, &share, &wav, &lines, &plan, &l).unwrap();
        let w = &out[0].words;
        assert_eq!(w.iter().map(|w| w.text.as_str()).collect::<Vec<_>>(), ["hey", "you"]);
        // "hey" starts after 3 spaces of 4 frames: frame 12, centred at 0.2525 s.
        assert!((w[0].start - (199.5 + 320.0 * 12.0) / 16000.0).abs() < 1e-9, "{}", w[0].start);
        assert!((w[1].start - (199.5 + 320.0 * 28.0) / 16000.0).abs() < 1e-9, "{}", w[1].start);
        assert!(out[0].conf > 0.5, "{}", out[0].conf);
        // "2" has no letters: it takes "me"'s start.
        assert_eq!(out[1].words[0].start, out[1].words[1].start);
        assert_eq!(out[1].start, out[1].words[1].start.into());
    }

    #[test]
    fn symbol_only_pieces_take_the_next_onset() {
        let mut words = vec![(None, "1".to_string()), (Some((2.0, 2.5)), "a".to_string()), (None, "!".to_string())];
        fill(&mut words);
        assert_eq!(words[0].0, Some((2.0, 2.0)));
        assert_eq!(words[2].0, None);
    }
}
