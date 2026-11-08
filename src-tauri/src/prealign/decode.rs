//! echora-kara's lead-line decoder, from the clean-room spec
//! (docs/research/prealign/echora-spec.md §4–5) by way of our own
//! `research/prealign/bench/echo_decode.py`.
//!
//! Two Viterbi passes over a blank-free, duration-floored CTC chain: a plain
//! pass measures each line's pace, which sets per-letter frame floors; the final
//! pass adds the text head's token log-probability to every letter. Letters pay
//! for frames where the mix is quiet (the energy feature). Scores accumulate in
//! f32 and ties break as the reference does, so token times match it to the ms.

/// Vocabulary: `|`=0, a..z=1..26, `'`=27, UNK=28, blank=29.
pub const VOCAB: usize = 30;
pub const BLANK: usize = 29;
const VOWELS: [u8; 6] = [1, 5, 9, 15, 21, 25];
/// Weight of the text head's log P(token | singing) on each letter.
const BETA: f32 = 0.4;
/// What a letter frame costs at zero vocal activity.
const VA_WEIGHT: f32 = 2.0;
const PACE_RATIO: f64 = 0.9;

/// Frames `[start, end)`.
pub type Span = (usize, usize);

/// A song's tokens and lines, as the decoder sees them.
pub struct Tokens {
    /// Per token, its letters' vocabulary ids (never empty).
    pub ids: Vec<Vec<u8>>,
    /// `[a, b)` token ranges of the (non-empty) lines.
    pub lines: Vec<Span>,
}

impl Tokens {
    fn letters(&self) -> impl Iterator<Item = u8> + '_ {
        self.ids.iter().flatten().copied()
    }
}

pub struct Decoded {
    /// Pass 1 letter spans; with `floors`, read only by the vector test.
    #[allow(dead_code)]
    pub plain: Vec<Span>,
    /// Per-letter frame floors of pass 2.
    #[allow(dead_code)]
    pub floors: Vec<usize>,
    /// Pass 2 letter spans.
    pub letters: Vec<Span>,
    /// Token spans after the blank hold (display times).
    pub tokens: Vec<Span>,
}

/// §4.1: per-frame vocal activity in [0, 1], from the loudness of 400-sample
/// frames at hop 320 of the full mix, scaled between its 10th and 90th percentile.
pub fn activity(x: &[f32], frames: usize) -> Vec<f32> {
    let ldb: Vec<f64> = (0..frames)
        .map(|t| {
            let sum: f64 = (320 * t..320 * t + 400).map(|i| x.get(i).map_or(0.0, |&v| (v as f64).powi(2))).sum();
            10.0 * (sum / 400.0 + 1e-10).log10()
        })
        .collect();
    let mut sorted = ldb.clone();
    sorted.sort_by(f64::total_cmp);
    let (lo, hi) = (quantile(&sorted, 0.1), quantile(&sorted, 0.9));
    let range = (hi - lo).max(1e-6);
    ldb.iter().map(|&v| ((v - lo) / range).clamp(0.0, 1.0) as f32).collect()
}

/// numpy's default (linear) quantile of sorted values.
fn quantile(sorted: &[f64], q: f64) -> f64 {
    let pos = q * (sorted.len() - 1) as f64;
    let (i, frac) = (pos.floor() as usize, pos - pos.floor());
    let j = (i + 1).min(sorted.len() - 1);
    sorted[i] + (sorted[j] - sorted[i]) * frac
}

/// §4.2: letters pay `2·(1 − a_t)`, blank does not. `logp`: `[T, 30]` row-major.
fn penalise(logp: &[f32], activity: &[f32]) -> Vec<f32> {
    let mut out = logp.to_vec();
    for (row, &a) in out.chunks_mut(VOCAB).zip(activity) {
        let pay = VA_WEIGHT * (1.0 - a);
        row[..BLANK].iter_mut().for_each(|v| *v -= pay);
    }
    out
}

/// §4.3: (letter_cap, token_cap, pause, hold) in frames, for `delta` s per frame.
fn caps(delta: f64) -> (usize, usize, usize, usize) {
    let r = |s: f64| (s / delta).round_ties_even() as usize;
    (r(0.040).max(1), r(0.100).max(1), r(0.4), (0.2 / delta).floor() as usize)
}

/// One state of the chain: which letter it emits (None = blank), and its moves.
struct State {
    letter: Option<usize>,
    /// May stay put: a blank, or a letter's last sub-state.
    stay: bool,
    /// May be entered from two states back (over the blank between two tokens).
    skip: bool,
}

/// §5.1: `B₀ │ token 0 letters × floors │ B₁ │ … │ B_K`.
fn chain(tokens: &Tokens, floors: &[usize]) -> Vec<State> {
    let ids: Vec<u8> = tokens.letters().collect();
    let blank = || State { letter: None, stay: true, skip: false };
    let mut out = vec![blank()];
    let mut j = 0;
    for tok in &tokens.ids {
        let first = j;
        for _ in tok {
            for r in 0..floors[j] {
                let skip = r == 0 && j == first && j > 0 && ids[j - 1] != ids[j];
                out.push(State { letter: Some(j), stay: r + 1 == floors[j], skip });
            }
            j += 1;
        }
        out.push(blank());
    }
    out
}

/// A bit per (frame, state): did this move win?
struct Bits {
    words: Vec<u64>,
    n: usize,
}

impl Bits {
    fn new(frames: usize, n: usize) -> Bits {
        Bits { words: vec![0; (frames * n).div_ceil(64)], n }
    }
    fn set(&mut self, t: usize, s: usize) {
        let i = t * self.n + s;
        self.words[i / 64] |= 1 << (i % 64);
    }
    fn get(&self, t: usize, s: usize) -> bool {
        let i = t * self.n + s;
        (self.words[i / 64] >> (i % 64)) & 1 == 1
    }
}

/// §5.1: the best path's letter spans, or None if no path fits.
/// `emit(t, letter)`: the score of letter `j` (Some) or blank (None) at frame `t`.
fn viterbi(frames: usize, tokens: &Tokens, floors: &[usize], emit: impl Fn(usize, Option<usize>) -> f32) -> Option<Vec<Span>> {
    let states = chain(tokens, floors);
    let n = states.len();
    if n - tokens.ids.len() - 1 > frames || n < 2 {
        return None;
    }
    let mut stayed = Bits::new(frames, n);
    let mut skipped = Bits::new(frames, n);
    let mut score = vec![f32::NEG_INFINITY; n];
    score[0] = emit(0, states[0].letter);
    score[1] = emit(0, states[1].letter);
    let mut best = vec![f32::NEG_INFINITY; n];
    for t in 1..frames {
        for (s, b) in best.iter_mut().enumerate() {
            *b = step(&states, &score, s, t, &mut stayed, &mut skipped);
        }
        for (s, st) in states.iter().enumerate() {
            score[s] = best[s] + emit(t, st.letter);
        }
    }
    if score[n - 1] == f32::NEG_INFINITY && score[n - 2] == f32::NEG_INFINITY {
        return None;
    }
    let end = if score[n - 1] >= score[n - 2] { n - 1 } else { n - 2 };
    Some(letter_spans(&backtrack(&states, end, frames, &stayed, &skipped), &states, floors.len()))
}

/// The best way into state `s` at frame `t`: stay, then advance, then skip (earliest wins ties).
fn step(states: &[State], score: &[f32], s: usize, t: usize, stayed: &mut Bits, skipped: &mut Bits) -> f32 {
    let mut best = if s > 0 { score[s - 1] } else { f32::NEG_INFINITY };
    if states[s].stay && score[s] >= best {
        best = score[s];
        stayed.set(t, s);
    }
    if states[s].skip && score[s - 2] > best {
        best = score[s - 2];
        skipped.set(t, s);
    }
    best
}

fn backtrack(states: &[State], end: usize, frames: usize, stayed: &Bits, skipped: &Bits) -> Vec<usize> {
    let mut path = vec![0; frames];
    let mut s = end;
    for t in (1..frames).rev() {
        path[t] = s;
        if states[s].skip && skipped.get(t, s) {
            s -= 2;
        } else if !(states[s].stay && stayed.get(t, s)) {
            s -= 1;
        }
    }
    path[0] = s;
    path
}

/// Each letter's `[first frame, last frame + 1)` on the path.
fn letter_spans(path: &[usize], states: &[State], letters: usize) -> Vec<Span> {
    let mut out = vec![(usize::MAX, 0); letters];
    for (t, &s) in path.iter().enumerate() {
        if let Some(j) = states[s].letter {
            out[j].0 = out[j].0.min(t);
            out[j].1 = t + 1;
        }
    }
    out
}

fn token_spans(letters: &[Span], tokens: &Tokens) -> Vec<Span> {
    let mut j = 0;
    tokens
        .ids
        .iter()
        .map(|tok| {
            let span = (letters[j].0, letters[j + tok.len() - 1].1);
            j += tok.len();
            span
        })
        .collect()
}

/// §5.3: per-letter floors from each line's pass-1 pace.
fn paced_floors(spans: &[Span], tokens: &Tokens, caps: (usize, usize, usize, usize)) -> Vec<usize> {
    let (letter_cap, token_cap, pause, _) = caps;
    let mut floors = Vec::new();
    for &(a, b) in &tokens.lines {
        let mut sp = spans[a..b].to_vec();
        sp.sort_by_key(|s| s.0);
        let pauses: usize = sp.windows(2).map(|w| w[1].0.saturating_sub(w[0].1)).filter(|&g| g > pause).sum();
        let total = sp.iter().map(|s| s.1).max().unwrap_or(0) - sp.iter().map(|s| s.0).min().unwrap_or(0);
        let sung = total.saturating_sub(pauses).max(1) as f64;
        let n_let: usize = tokens.ids[a..b].iter().map(Vec::len).sum();
        let f_tok = ((PACE_RATIO * sung / (b - a) as f64).floor() as usize).min(token_cap).max(1);
        let f_let = ((PACE_RATIO * sung / n_let.max(1) as f64).floor() as usize).min(letter_cap).max(1);
        for ids in &tokens.ids[a..b] {
            floors.extend(token_floors(ids, f_tok, f_let));
        }
    }
    floors
}

/// Every letter gets `f_let`; a shortfall to `f_tok` goes round-robin to the
/// vowels (or the first letter if there is none).
fn token_floors(ids: &[u8], f_tok: usize, f_let: usize) -> Vec<usize> {
    let mut m = vec![f_let; ids.len()];
    let mut vowels: Vec<usize> = (0..ids.len()).filter(|&i| VOWELS.contains(&ids[i])).collect();
    if vowels.is_empty() {
        vowels.push(0);
    }
    for x in 0..f_tok.saturating_sub(f_let * ids.len()) {
        m[vowels[x % vowels.len()]] += 1;
    }
    m
}

/// §5.5: a token that is not its line's last runs on into the gap after it, up to `hold` frames.
fn hold(acoustic: &[Span], tokens: &Tokens, hold: usize) -> Vec<Span> {
    let mut line_last = vec![false; acoustic.len()];
    tokens.lines.iter().for_each(|&(_, b)| line_last[b - 1] = true);
    let mut out = acoustic.to_vec();
    for k in 0..acoustic.len().saturating_sub(1) {
        if !line_last[k] && acoustic[k + 1].0 > acoustic[k].1 {
            out[k].1 = acoustic[k + 1].0.min(acoustic[k].1 + hold);
        }
    }
    out
}

/// Decode a song. `logp`: `[T, 30]` CTC log-probabilities, `activity`: `[T]`,
/// `share`: `[T, K]` the text head's log P(token | singing), `delta`: seconds
/// per frame (audio length / T). None if the letters do not fit in T frames.
pub fn decode(logp: &[f32], activity: &[f32], share: &[f32], tokens: &Tokens, delta: f64) -> Option<Decoded> {
    let frames = activity.len();
    let k_len = tokens.ids.len();
    let ids: Vec<usize> = tokens.letters().map(usize::from).collect();
    let own: Vec<usize> = tokens.ids.iter().enumerate().flat_map(|(k, tok)| std::iter::repeat_n(k, tok.len())).collect();
    let caps = caps(delta);
    let ep = penalise(logp, activity);
    let plain_emit = |t: usize, j: Option<usize>| ep[t * VOCAB + j.map_or(BLANK, |j| ids[j])];
    // §5.4: every letter also gets its token's text share; blank does not.
    let bonus = |t: usize, j: Option<usize>| match j {
        Some(j) => ep[t * VOCAB + ids[j]] + BETA * share[t * k_len + own[j]],
        None => ep[t * VOCAB + BLANK],
    };

    let ones = vec![1; ids.len()];
    let plain = viterbi(frames, tokens, &ones, plain_emit)?;
    let floors = paced_floors(&token_spans(&plain, tokens), tokens, caps);
    let letters = viterbi(frames, tokens, &floors, bonus).or_else(|| viterbi(frames, tokens, &ones, bonus))?;
    let tokens_out = hold(&token_spans(&letters, tokens), tokens, caps.3);
    Some(Decoded { plain, floors, letters, tokens: tokens_out })
}

/// §5.6: frame index → seconds. Past the last frame, extrapolate by `delta`.
pub fn seconds(frame: usize, frame_times: &[f64], delta: f64) -> f64 {
    match frame_times.get(frame) {
        Some(&t) => t,
        None => frame_times[frame_times.len() - 1] + (frame + 1 - frame_times.len()) as f64 * delta,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `T` frames where `rows[t]` is the one likely id (prob 0.9); everything else 0.001.
    fn logp(rows: &[usize]) -> Vec<f32> {
        rows.iter().flat_map(|&id| (0..VOCAB).map(move |c| if c == id { 0.9f32.ln() } else { 0.001f32.ln() })).collect()
    }

    fn one_line(ids: Vec<Vec<u8>>) -> Tokens {
        let k = ids.len();
        Tokens { ids, lines: vec![(0, k)] }
    }

    #[test]
    fn letters_follow_the_evidence() {
        // "ab" "c": a a b b _ c c _
        let rows = [1, 1, 2, 2, BLANK, 3, 3, BLANK];
        let toks = one_line(vec![vec![1, 2], vec![3]]);
        let lp = logp(&rows);
        let ones = vec![1; 3];
        let spans = viterbi(rows.len(), &toks, &ones, |t, j| lp[t * VOCAB + j.map_or(BLANK, |j| [1, 2, 3][j])]).unwrap();
        assert_eq!(spans, vec![(0, 2), (2, 4), (5, 7)]);
    }

    #[test]
    fn the_same_letter_across_tokens_needs_a_blank() {
        // "a" "a" in 2 frames cannot be; in 3 it is a _ a.
        let toks = one_line(vec![vec![1], vec![1]]);
        let lp = logp(&[1, 1, 1]);
        let emit = |t: usize, j: Option<usize>| lp[t * VOCAB + j.map_or(BLANK, |_| 1)];
        assert_eq!(viterbi(2, &toks, &[1, 1], emit), None);
        assert_eq!(viterbi(3, &toks, &[1, 1], emit), Some(vec![(0, 1), (2, 3)]));
        // Different letters may touch.
        let toks = one_line(vec![vec![1], vec![2]]);
        let lp = logp(&[1, 2]);
        assert_eq!(viterbi(2, &toks, &[1, 1], |t, j| lp[t * VOCAB + j.map_or(BLANK, |j| j + 1)]), Some(vec![(0, 1), (1, 2)]));
    }

    #[test]
    fn a_floor_holds_a_letter_however_brief_its_evidence() {
        // "b" heard for one frame, floored at 3.
        let toks = one_line(vec![vec![2]]);
        let lp = logp(&[BLANK, BLANK, 2, BLANK, BLANK, BLANK]);
        let emit = |t: usize, j: Option<usize>| lp[t * VOCAB + j.map_or(BLANK, |_| 2)];
        assert_eq!(viterbi(6, &toks, &[1], emit), Some(vec![(2, 3)]));
        let (s, e) = viterbi(6, &toks, &[3], emit).unwrap()[0];
        assert_eq!(e - s, 3);
        assert!(s <= 2 && e > 2);
    }

    #[test]
    fn a_shortfall_goes_to_the_vowels_in_turn() {
        // "teki": t e k i, 1 frame each, 6 wanted: e and i get one more each.
        assert_eq!(token_floors(&[20, 5, 11, 9], 6, 1), vec![1, 2, 1, 2]);
        assert_eq!(token_floors(&[20, 5, 11, 9], 7, 1), vec![1, 3, 1, 2]);
        assert_eq!(token_floors(&[14, 7], 5, 2), vec![3, 2], "no vowel: the first letter");
        assert_eq!(token_floors(&[1, 2, 3], 2, 2), vec![2, 2, 2], "never taken away");
    }

    #[test]
    fn a_token_holds_into_the_gap_but_not_at_a_line_end() {
        let toks = Tokens { ids: vec![vec![1], vec![1], vec![1]], lines: vec![(0, 2), (2, 3)] };
        let out = hold(&[(0, 2), (20, 22), (40, 41)], &toks, 9);
        assert_eq!(out, vec![(0, 11), (20, 22), (40, 41)]);
        assert_eq!(hold(&[(0, 2), (5, 6), (40, 41)], &toks, 9)[0], (0, 5), "up to the next start");
    }

    #[test]
    fn caps_match_the_spec_for_track_5() {
        assert_eq!(caps(0.020_008_820_493_572_3), (2, 5, 20, 9));
    }

    #[test]
    fn activity_is_scaled_between_quiet_and_loud() {
        let mut x = vec![0.0f32; 320 * 100 + 400];
        x[320 * 50..320 * 100].iter_mut().for_each(|v| *v = 0.5);
        let a = activity(&x, 100);
        assert_eq!((a[0], a[90]), (0.0, 1.0));
    }

    #[test]
    fn seconds_extrapolate_past_the_end() {
        let ft = [0.01, 0.03];
        assert_eq!(seconds(1, &ft, 0.02), 0.03);
        assert!((seconds(3, &ft, 0.02) - 0.07).abs() < 1e-12);
    }

    /// The spec's test vectors (research/prealign/data/spec_vectors, git-ignored):
    /// given their own log_probs, activity, text_share and frame_times, every
    /// token's display time must match echora's to the millisecond.
    #[test]
    #[ignore = "needs research/prealign/data/spec_vectors"]
    fn spec_vectors() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../research/prealign/data/spec_vectors");
        for id in [5, 13, 18] {
            let j: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join(format!("track_{id}.json"))).unwrap()).unwrap();
            let mut npz = npz::Npz::open(&dir.join(format!("track_{id}.npz")));
            let ids: Vec<Vec<u8>> = serde_json::from_value(j["encoded"].clone()).unwrap();
            let lines: Vec<Span> = serde_json::from_value(j["line_ranges"].clone()).unwrap();
            let tokens = Tokens { ids, lines };
            let delta = j["frame_shift"].as_f64().unwrap();
            let frame_times = npz.f64("frame_times");
            let out = decode(&npz.f32("log_probs"), &npz.f32("activity"), &npz.f32("text_share"), &tokens, delta).unwrap();
            let want = |k: &str| -> Vec<Span> { serde_json::from_value(j[k].clone()).unwrap() };
            assert_eq!(out.plain, want("plain_pass_letter_spans"), "track {id}: pass 1");
            assert_eq!(out.floors, serde_json::from_value::<Vec<usize>>(j["min_frames_per_letter"].clone()).unwrap(), "track {id}: floors");
            assert_eq!(out.letters, want("final_letter_spans"), "track {id}: pass 2");
            let ms = |f: usize| (1000.0 * seconds(f, &frame_times, delta)).round_ties_even() as i64;
            let reference: Vec<(i64, i64)> = serde_json::from_value(j["token_times_ms"].clone()).unwrap();
            let bad = out.tokens.iter().zip(&reference).filter(|(&(s, e), &r)| (ms(s), ms(e)) != r).count();
            println!("track {id}: {bad}/{} token-ms mismatches", reference.len());
            assert_eq!((bad, out.tokens.len()), (0, reference.len()), "track {id}");
        }
    }

    /// Just enough of numpy's `.npz` (a zip of `.npy`) for the vectors: little-endian f4/f8, C order.
    mod npz {
        use std::io::Read;

        pub struct Npz(zip::ZipArchive<std::fs::File>);

        impl Npz {
            pub fn open(path: &std::path::Path) -> Npz {
                Npz(zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap())
            }

            fn raw(&mut self, key: &str, descr: &str) -> Vec<u8> {
                let mut b = Vec::new();
                self.0.by_name(&format!("{key}.npy")).unwrap().read_to_end(&mut b).unwrap();
                let header_len = u16::from_le_bytes([b[8], b[9]]) as usize;
                let header = std::str::from_utf8(&b[10..10 + header_len]).unwrap();
                assert!(header.contains(&format!("'descr': '{descr}'")) && header.contains("'fortran_order': False"), "{key}: {header}");
                b.split_off(10 + header_len)
            }

            pub fn f32(&mut self, key: &str) -> Vec<f32> {
                self.raw(key, "<f4").chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()
            }

            pub fn f64(&mut self, key: &str) -> Vec<f64> {
                self.raw(key, "<f8").chunks_exact(8).map(|c| f64::from_le_bytes(c.try_into().unwrap())).collect()
            }
        }
    }
}
