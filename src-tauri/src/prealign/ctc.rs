//! CTC forced alignment: the Viterbi path of a known token sequence through a
//! matrix of per-frame log-probabilities. A straight port of torchaudio's
//! `forced_align` (CPU) and `merge_tokens`, tie-breaking included, so the
//! Python study's numbers carry over; plus optional jumps over targets, so a
//! recording that leaves some out still aligns the rest.

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

/// What leaving targets out costs, as a log-probability: `open` per run of
/// targets left out, plus `each` per target in it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkipCost {
    pub open: f32,
    pub each: f32,
}

impl SkipCost {
    /// Plain forced alignment: every target is placed.
    pub const NEVER: SkipCost = SkipCost { open: f32::INFINITY, each: 0.0 };
}

/// The best path for `targets` (no blanks in it): each target's span, in
/// order, or None for a target the path left out. None when the audio is too
/// short to hold the targets.
///
/// The path may go from any of `gates` (target indices, ascending) straight to
/// a later one, leaving out the targets between, at `cost` (the gates
/// themselves don't count).
pub fn align(em: &Emissions, targets: &[usize], gates: &[usize], cost: SkipCost, blank: usize) -> Option<Vec<Option<Span>>> {
    let path = viterbi(em, targets, gates, cost, blank)?;
    Some(spans(&path, em, targets))
}

/// Trellis state per frame on the best path: even = blank, odd `2j + 1` = target `j`.
fn viterbi(em: &Emissions, targets: &[usize], gates: &[usize], cost: SkipCost, blank: usize) -> Option<Vec<usize>> {
    let (t_len, s_len) = (em.frames, 2 * targets.len() + 1);
    if t_len == 0 {
        return None;
    }
    let label = |i: usize| if i % 2 == 0 { blank } else { targets[i / 2] };
    // The blank before a label may be passed over only between two different labels.
    let blank_optional = |i: usize| i % 2 == 1 && i != 1 && targets[i / 2] != targets[i / 2 - 1];
    // Per state: its gate's number, if it is one.
    let mut gate = vec![None; s_len];
    for (k, &g) in gates.iter().enumerate() {
        gate[2 * g + 1] = Some(k);
    }
    // Leaving out gates[m]..gates[k] costs open + each * (behind[k] - behind[m]).
    let behind: Vec<f32> = gates.iter().enumerate().map(|(k, &g)| (g - k) as f32).collect();

    let mut prev = vec![f32::NEG_INFINITY; s_len];
    let mut cur = vec![f32::NEG_INFINITY; s_len];
    // 0..=2: steps back; 3: a jump, from the gate in `jumped`.
    let mut back = vec![0u8; t_len * s_len];
    let mut jumped = vec![0u32; t_len * gates.len()];
    for (i, p) in prev.iter_mut().enumerate().take(2) {
        *p = em.at(0, label(i));
    }
    for t in 1..t_len {
        cur[0] = prev[0] + em.at(t, blank);
        // The best gate to jump from so far, before paying for what it leaves out.
        let mut from = (f32::NEG_INFINITY, 0);
        for i in 1..s_len {
            let x0 = prev[i];
            let x1 = prev[i - 1];
            let x2 = if blank_optional(i) { prev[i - 2] } else { f32::NEG_INFINITY };
            // torchaudio's tie-breaking, for parity.
            let (mut best, mut step) = if x2 > x1 && x2 > x0 {
                (x2, 2)
            } else if x1 > x0 && x1 > x2 {
                (x1, 1)
            } else {
                (x0, 0)
            };
            if let Some(k) = gate[i] {
                let x3 = from.0 - cost.open - cost.each * behind[k];
                if x3 > best {
                    (best, step) = (x3, 3);
                    jumped[t * gates.len() + k] = from.1;
                }
                if prev[i] + cost.each * behind[k] > from.0 {
                    from = (prev[i] + cost.each * behind[k], k as u32);
                }
            }
            back[t * s_len + i] = step;
            cur[i] = best + em.at(t, label(i));
        }
        std::mem::swap(&mut prev, &mut cur);
    }

    let mut state = if s_len > 1 && prev[s_len - 2] >= prev[s_len - 1] { s_len - 2 } else { s_len - 1 };
    if !prev[state].is_finite() {
        return None;
    }
    let mut path = vec![0; t_len];
    for t in (0..t_len).rev() {
        path[t] = state;
        state = match (back[t * s_len + state], gate[state]) {
            (3, Some(k)) => 2 * gates[jumped[t * gates.len() + k] as usize] + 1,
            (step, _) => state - step as usize,
        };
    }
    Some(path)
}

/// Each target's run of frames on the path, with its mean frame posterior.
fn spans(path: &[usize], em: &Emissions, targets: &[usize]) -> Vec<Option<Span>> {
    let mut out = vec![None; targets.len()];
    let mut t = 0;
    while t < path.len() {
        let (state, start) = (path[t], t);
        while t < path.len() && path[t] == state {
            t += 1;
        }
        if state % 2 == 1 {
            let token = targets[state / 2];
            let sum: f32 = (start..t).map(|f| em.at(f, token).exp()).sum();
            out[state / 2] = Some(Span { start, end: t, score: sum / (t - start) as f32 });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Log-probabilities from per-frame probability rows (blank = column 0).
    fn em(rows: &[&[f32]]) -> Emissions {
        let vocab = rows[0].len();
        Emissions { data: rows.iter().flat_map(|r| r.iter().map(|p| p.ln())).collect(), frames: rows.len(), vocab }
    }

    /// Plain forced alignment: every target placed.
    fn forced(e: &Emissions, targets: &[usize]) -> Option<Vec<Span>> {
        align(e, targets, &[], SkipCost::NEVER, 0).map(|s| s.into_iter().map(Option::unwrap).collect())
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
        let spans = forced(&e, &[1, 2]).unwrap();
        assert_eq!((spans[0].start, spans[0].end), (1, 3));
        assert_eq!((spans[1].start, spans[1].end), (4, 5));
        assert!((spans[0].score - 0.8).abs() < 1e-5);
    }

    #[test]
    fn a_repeated_token_needs_a_blank_between() {
        // "1 1" on 3 frames: the only legal path is 1, blank, 1.
        let e = em(&[&[0.1, 0.9], &[0.1, 0.9], &[0.1, 0.9]]);
        let spans = forced(&e, &[1, 1]).unwrap();
        assert_eq!((spans[0].start, spans[0].end, spans[1].start, spans[1].end), (0, 1, 2, 3));
        // Two frames cannot hold it.
        let e = em(&[&[0.1, 0.9], &[0.1, 0.9]]);
        assert!(forced(&e, &[1, 1]).is_none());
    }

    #[test]
    fn every_target_gets_a_frame_even_against_the_evidence() {
        // Token 2 never peaks, but forced alignment still places it.
        let e = em(&[&[0.1, 0.8, 0.1], &[0.1, 0.8, 0.1], &[0.8, 0.15, 0.05], &[0.8, 0.15, 0.05]]);
        let spans = forced(&e, &[1, 2]).unwrap();
        assert_eq!(spans.len(), 2);
        assert!(spans[0].end <= spans[1].start);
        assert!(spans[1].score < 0.1);
    }

    #[test]
    fn a_jump_leaves_out_what_was_not_said_when_that_is_cheaper() {
        // 1 then 3 is heard; 2 never is. Leaving 2 out (gate 0 to gate 2)
        // costs open + each; forcing 2 in costs about ln(0.01) = -4.6.
        let e = em(&[
            &[0.1, 0.88, 0.01, 0.01],
            &[0.1, 0.88, 0.01, 0.01],
            &[0.88, 0.1, 0.01, 0.01],
            &[0.1, 0.01, 0.01, 0.88],
            &[0.1, 0.01, 0.01, 0.88],
        ]);
        let run = |open: f32, each: f32| align(&e, &[1, 2, 3], &[0, 2], SkipCost { open, each }, 0).unwrap();
        let s = run(0.5, 0.5);
        assert_eq!(s[1], None);
        assert_eq!(s[0].map(|s| s.start), Some(0));
        assert_eq!(s[2].map(|s| (s.start, s.end)), Some((3, 5)));
        assert!(run(10.0, 0.5).iter().all(Option::is_some), "too dear: forced in after all");
        assert!(run(0.5, 10.0).iter().all(Option::is_some));
    }

    #[test]
    fn one_run_left_out_pays_to_open_once() {
        // Token 3 is a free wildcard (log p = 0) gating the lines; 1 is heard,
        // 2 never is (ln 0.05 = -3.0 forced in). Leaving out one line of 2s
        // costs 3.5: dearer than forcing it in. Two in a row cost 4 together.
        let mut rows: Vec<&[f32]> = vec![&[0.1, 0.88, 0.05, 1.0]; 2];
        rows.extend([&[0.9, 0.01, 0.05, 1.0][..]; 6]);
        let e = em(&rows);
        let cost = SkipCost { open: 3.0, each: 0.5 };
        let placed = |targets: &[usize], gates: &[usize]| align(&e, targets, gates, cost, 0).unwrap().iter().map(Option::is_some).collect::<Vec<_>>();
        assert_eq!(placed(&[3, 1, 3, 2, 3], &[0, 2, 4]), [true; 5]);
        assert_eq!(placed(&[3, 1, 3, 2, 3, 2, 3], &[0, 2, 4, 6]), [true, true, true, false, false, false, true]);
    }

    #[test]
    fn too_short_is_none() {
        let e = em(&[&[0.5, 0.5, 0.0001]]);
        assert!(forced(&e, &[1, 2]).is_none());
    }
}
