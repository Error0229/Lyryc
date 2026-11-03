//! CTC forced alignment: the Viterbi path of a known token sequence through a
//! matrix of per-frame log-probabilities. A straight port of torchaudio's
//! `forced_align` (CPU) and `merge_tokens`, tie-breaking included, so the
//! Python study's numbers carry over.

/// Emission matrix: `frames` rows of `vocab` log-probabilities, row-major.
pub struct Emissions {
    pub data: Vec<f32>,
    pub frames: usize,
    pub vocab: usize,
}

impl Emissions {
    fn at(&self, t: usize, v: usize) -> f32 {
        self.data[t * self.vocab + v]
    }
}

/// One target token's run of frames: `[start, end)` and its mean posterior.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub score: f32,
}

/// The best path for `targets` (no blanks in it), one span per target, in order.
/// None when the audio is too short to hold the targets.
pub fn align(em: &Emissions, targets: &[usize], blank: usize) -> Option<Vec<Span>> {
    let path = viterbi(em, targets, blank)?;
    let spans = merge(&path, em, blank);
    (spans.len() == targets.len()).then_some(spans)
}

/// Token per frame on the best path.
fn viterbi(em: &Emissions, targets: &[usize], blank: usize) -> Option<Vec<usize>> {
    let (t_len, l) = (em.frames, targets.len());
    let s_len = 2 * l + 1;
    let repeats = targets.windows(2).filter(|w| w[0] == w[1]).count();
    if t_len == 0 || t_len < l + repeats {
        return None;
    }
    let label = |i: usize| if i % 2 == 0 { blank } else { targets[i / 2] };
    // The blank before a label may be skipped only between two different labels.
    let can_skip = |i: usize| i % 2 == 1 && i != 1 && targets[i / 2] != targets[i / 2 - 1];

    let mut prev = vec![f32::NEG_INFINITY; s_len];
    let mut cur = vec![f32::NEG_INFINITY; s_len];
    let mut back = vec![0u8; t_len * s_len];

    // The band of states that can still reach the end in time.
    let mut start = if t_len > l + repeats { 0 } else { 1 };
    let mut end = if s_len == 1 { 1 } else { 2 };
    for (i, p) in prev.iter_mut().enumerate().take(end).skip(start) {
        *p = em.at(0, label(i));
    }
    for t in 1..t_len {
        if t_len - t <= l + repeats {
            if start % 2 == 1 && start / 2 + 1 < l && targets[start / 2] != targets[start / 2 + 1] {
                start += 1;
            }
            start += 1;
        }
        if t <= l + repeats {
            if end % 2 == 0 && end < 2 * l && targets[end / 2 - 1] != targets[end / 2] {
                end += 1;
            }
            end += 1;
        }
        cur.fill(f32::NEG_INFINITY);
        let mut from = start;
        if start == 0 {
            cur[0] = prev[0] + em.at(t, blank);
            from = 1;
        }
        for i in from..end.min(s_len) {
            let x0 = prev[i];
            let x1 = prev[i - 1];
            let x2 = if can_skip(i) { prev[i - 2] } else { f32::NEG_INFINITY };
            let (best, step) = if x2 > x1 && x2 > x0 {
                (x2, 2)
            } else if x1 > x0 && x1 > x2 {
                (x1, 1)
            } else {
                (x0, 0)
            };
            back[t * s_len + i] = step;
            cur[i] = best + em.at(t, label(i));
        }
        std::mem::swap(&mut prev, &mut cur);
    }

    let mut state = if s_len > 1 && prev[s_len - 2] >= prev[s_len - 1] { s_len - 2 } else { s_len - 1 };
    if !prev[state].is_finite() {
        return None;
    }
    let mut path = vec![blank; t_len];
    for t in (0..t_len).rev() {
        path[t] = label(state);
        state -= back[t * s_len + state] as usize;
    }
    Some(path)
}

/// Runs of the same non-blank token, each with its mean frame posterior.
fn merge(path: &[usize], em: &Emissions, blank: usize) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut t = 0;
    while t < path.len() {
        let token = path[t];
        let start = t;
        while t < path.len() && path[t] == token {
            t += 1;
        }
        if token != blank {
            let sum: f32 = (start..t).map(|f| em.at(f, token).exp()).sum();
            spans.push(Span { start, end: t, score: sum / (t - start) as f32 });
        }
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Log-probabilities from per-frame probability rows (blank = column 0).
    fn em(rows: &[&[f32]]) -> Emissions {
        let vocab = rows[0].len();
        Emissions { data: rows.iter().flat_map(|r| r.iter().map(|p| p.ln())).collect(), frames: rows.len(), vocab }
    }

    #[test]
    fn follows_the_peaks() {
        // Tokens 1 then 2: frames 1-2 favour 1, frame 4 favours 2.
        let e = em(&[
            &[0.8, 0.1, 0.1],
            &[0.1, 0.8, 0.1],
            &[0.1, 0.8, 0.1],
            &[0.8, 0.1, 0.1],
            &[0.1, 0.1, 0.8],
            &[0.8, 0.1, 0.1],
        ]);
        let spans = align(&e, &[1, 2], 0).unwrap();
        assert_eq!((spans[0].start, spans[0].end), (1, 3));
        assert_eq!((spans[1].start, spans[1].end), (4, 5));
        assert!((spans[0].score - 0.8).abs() < 1e-5);
    }

    #[test]
    fn a_repeated_token_needs_a_blank_between() {
        // "1 1" on 3 frames: the only legal path is 1, blank, 1.
        let e = em(&[&[0.1, 0.9], &[0.1, 0.9], &[0.1, 0.9]]);
        let spans = align(&e, &[1, 1], 0).unwrap();
        assert_eq!((spans[0].start, spans[0].end, spans[1].start, spans[1].end), (0, 1, 2, 3));
        // Two frames cannot hold it.
        let e = em(&[&[0.1, 0.9], &[0.1, 0.9]]);
        assert!(align(&e, &[1, 1], 0).is_none());
    }

    #[test]
    fn every_target_gets_a_frame_even_against_the_evidence() {
        // Token 2 never peaks, but forced alignment still places it.
        let e = em(&[&[0.1, 0.8, 0.1], &[0.1, 0.8, 0.1], &[0.8, 0.15, 0.05], &[0.8, 0.15, 0.05]]);
        let spans = align(&e, &[1, 2], 0).unwrap();
        assert_eq!(spans.len(), 2);
        assert!(spans[0].end <= spans[1].start);
        assert!(spans[1].score < 0.1);
    }

    #[test]
    fn too_short_is_none() {
        let e = em(&[&[0.5, 0.5, 0.0001]]);
        assert!(align(&e, &[1, 2], 0).is_none());
    }
}
