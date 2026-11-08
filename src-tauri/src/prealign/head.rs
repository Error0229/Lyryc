//! echora-kara's text head around its two ONNX halves (echora-spec.md §2–3,
//! `research/prealign/bench/echo_head.py`): the text side runs once per song,
//! the frame side over 1250-frame windows that overlap by 250, stitched by
//! keeping each window's middle.

use super::decode::VOCAB;

/// The encoder's hidden size.
pub const HIDDEN: usize = 1024;
/// Letters per token the head sees (the decoder still uses all of them).
const MAX_LETTERS: usize = 16;
const PAD: i64 = 32;
const WIN: usize = 1250;
const STEP: usize = 1000;
const KEEP: usize = 125;

/// The head's text input (§3.1): `[K, 16]` letters padded with 32, line number, position in line.
pub struct Packed {
    pub letters: Vec<i64>,
    pub line: Vec<i64>,
    pub pos: Vec<i64>,
}

/// The text side's outputs, shared by every window: `text`, `kappa` `[K, 512]`, `m` `[K, 30]`.
pub struct Text {
    pub text: Vec<f32>,
    pub kappa: Vec<f32>,
    pub m: Vec<f32>,
}

/// `ids`: each token's letters; `line_of`: each token's (non-empty) line number.
pub fn pack(ids: &[Vec<u8>], line_of: &[usize]) -> Packed {
    let mut letters = vec![PAD; ids.len() * MAX_LETTERS];
    let mut pos = vec![0i64; ids.len()];
    for (k, tok) in ids.iter().enumerate() {
        for (i, &c) in tok.iter().take(MAX_LETTERS).enumerate() {
            letters[k * MAX_LETTERS + i] = i64::from(c);
        }
        if k > 0 && line_of[k] == line_of[k - 1] {
            pos[k] = pos[k - 1] + 1;
        }
    }
    Packed { letters, line: line_of.iter().map(|&l| l as i64).collect(), pos }
}

/// §3.2: `(start, end, keep_from, keep_to)` in song frames.
fn windows(frames: usize) -> Vec<(usize, usize, usize, usize)> {
    let starts: Vec<usize> = (0..(frames.saturating_sub(WIN - STEP)).max(1)).step_by(STEP).collect();
    let last = starts.len() - 1;
    starts
        .iter()
        .enumerate()
        .map(|(n, &s)| {
            let e = (s + WIN).min(frames);
            (s, e, s + if n > 0 { KEEP } else { 0 }, e - if n < last { KEEP } else { 0 })
        })
        .collect()
}

/// `S [T, K]`: log P(token | singing) per frame. `run(hidden, logp)` runs the
/// frame side on one window (`[W, 1024]`, `[W, 30]`) and returns `[W, K]`.
pub fn text_share<E>(
    hidden: &[f32],
    logp: &[f32],
    tokens: usize,
    mut run: impl FnMut(&[f32], &[f32]) -> Result<Vec<f32>, E>,
) -> Result<Vec<f32>, E> {
    let frames = logp.len() / VOCAB;
    let mut share = Vec::with_capacity(frames * tokens);
    for (s, e, lo, hi) in windows(frames) {
        let out = run(&hidden[s * HIDDEN..e * HIDDEN], &logp[s * VOCAB..e * VOCAB])?;
        share.extend_from_slice(&out[(lo - s) * tokens..(hi - s) * tokens]);
    }
    Ok(share)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_cover_every_frame_once() {
        for frames in [1, 900, 1250, 1251, 2249, 10_657] {
            let w = windows(frames);
            let kept: Vec<(usize, usize)> = w.iter().map(|&(_, _, lo, hi)| (lo, hi)).collect();
            assert_eq!(kept[0].0, 0);
            assert_eq!(kept.last().unwrap().1, frames);
            assert!(kept.windows(2).all(|p| p[0].1 == p[1].0), "{frames}: {kept:?}");
            assert!(w.iter().all(|&(s, e, _, _)| e - s <= WIN));
        }
        assert_eq!(windows(2249), vec![(0, 1250, 0, 1125), (1000, 2249, 1125, 2249)]);
    }

    #[test]
    fn packing_follows_the_spec_example() {
        // Track 13: "hou lai liu ruo ying rene liu | hou lai | wo".
        let ids: Vec<Vec<u8>> = ["hou", "lai", "liu", "ruo", "ying", "rene", "liu", "hou", "lai", "wo"]
            .iter()
            .map(|w| w.bytes().map(|b| b - b'a' + 1).collect())
            .collect();
        let p = pack(&ids, &[0, 0, 0, 0, 0, 0, 0, 1, 1, 2]);
        assert_eq!(&p.letters[..4], &[8, 15, 21, 32]);
        assert_eq!(&p.letters[16..20], &[12, 1, 9, 32]);
        assert_eq!(p.pos, vec![0, 1, 2, 3, 4, 5, 6, 0, 1, 0]);
        let long = pack(&[vec![1; 20]], &[0]);
        assert_eq!(long.letters, vec![1; 16], "the head sees 16 letters");
    }

    #[test]
    fn stitching_keeps_each_windows_middle() {
        // Each window answers with its own start frame in every cell.
        let frames = 2600;
        let (hidden, logp) = (vec![0.0; frames * HIDDEN], vec![0.0; frames * VOCAB]);
        let mut starts = Vec::new();
        let share = text_share::<()>(&hidden, &logp, 2, |h, _| {
            let s = (h.as_ptr() as usize - hidden.as_ptr() as usize) / 4 / HIDDEN;
            starts.push(s);
            Ok(vec![s as f32; h.len() / HIDDEN * 2])
        })
        .unwrap();
        assert_eq!(starts, vec![0, 1000, 2000]);
        assert_eq!(share.len(), frames * 2);
        assert_eq!((share[1124 * 2], share[1125 * 2], share[2125 * 2]), (0.0, 1000.0, 2000.0));
    }
}
