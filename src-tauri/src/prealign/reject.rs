//! The study's reject rule, and how aligned timing is laid over the lyrics the
//! lookup found (docs/research/prealign/REPORT.md, "Robustness and the reject rule").
//!
//! 1. More than 60% of a track's lines below confidence 0.15: drop the whole
//!    alignment (the model knows it is lost).
//! 2. Otherwise a line loses its words if the median confidence of it and its
//!    neighbours is below 0.10, or, when the lyrics came with real line timing,
//!    if its onset is more than 0.7 s off that timing (after removing the
//!    track's median offset, as the study measured it).
//! 3. A rejected line keeps the source's line time if it had real timing, else
//!    the aligner's line start. It only loses its words.

use super::align::AlignedLine;
use crate::types::{LyricLine, LyricWord};

const TRACK_LOW_CONF: f64 = 0.15;
const TRACK_LOW_SHARE: f64 = 0.6;
const LINE_MIN_CONF: f64 = 0.10;
const MAX_OFF_SOURCE: f64 = 0.7;
/// Same as `parse_lrc_format`: the last line's length when nothing says otherwise.
const LAST_LINE_SECS: f64 = 3.0;

/// Rule 1. A track with no aligned line at all is rejected too.
pub fn track_rejected(aligned: &[AlignedLine]) -> bool {
    let confs: Vec<f64> = aligned.iter().filter(|l| l.start.is_some()).map(|l| l.conf).collect();
    confs.is_empty() || confs.iter().filter(|&&c| c < TRACK_LOW_CONF).count() as f64 / confs.len() as f64 > TRACK_LOW_SHARE
}

fn median(v: &mut [f64]) -> f64 {
    v.sort_by(f64::total_cmp);
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

/// Rule 2 for every aligned line: true = keep its words.
fn keep(aligned: &[AlignedLine], source: &[LyricLine], real_timing: bool) -> Vec<bool> {
    let timed: Vec<usize> = (0..aligned.len()).filter(|&i| aligned[i].start.is_some()).collect();
    let confs: Vec<f64> = timed.iter().map(|&i| aligned[i].conf).collect();
    let smoothed: Vec<f64> = (0..confs.len())
        .map(|k| median(&mut confs[k.saturating_sub(1)..(k + 2).min(confs.len())].to_vec()))
        .collect();

    let off: Vec<f64> = timed.iter().map(|&i| aligned[i].start.unwrap_or(0.0) - source[i].time).collect();
    let centre = if off.is_empty() { 0.0 } else { median(&mut off.clone()) };

    let mut keep = vec![false; aligned.len()];
    for (k, &i) in timed.iter().enumerate() {
        let confident = smoothed[k] >= LINE_MIN_CONF;
        let agrees = !real_timing || (off[k] - centre).abs() <= MAX_OFF_SOURCE;
        keep[i] = confident && agrees;
    }
    keep
}

/// The lookup's lines re-timed by the alignment. `real_timing`: the lines'
/// times are a real source's (LRCLIB synced, QQ, ...), not an estimate.
/// None if the alignment does not fit these lines or is rejected as a whole.
pub fn apply(source: &[LyricLine], real_timing: bool, aligned: &[AlignedLine]) -> Option<Vec<LyricLine>> {
    if aligned.len() != source.len() || track_rejected(aligned) {
        return None;
    }
    let keep = keep(aligned, source, real_timing);
    let mut out: Vec<LyricLine> = source
        .iter()
        .zip(aligned)
        .zip(&keep)
        .map(|((line, al), &kept)| {
            let time = match al.start {
                Some(start) if kept || !real_timing => start,
                _ => line.time,
            };
            let words = kept.then(|| words(al));
            LyricLine { time, text: line.text.clone(), duration: None, words }
        })
        .collect();

    // A rejected line's source time may disagree with its aligned neighbours: never run backwards.
    for i in 1..out.len() {
        if out[i].time < out[i - 1].time {
            out[i].time = out[i - 1].time;
        }
    }
    for i in 0..out.len() {
        out[i].duration = Some(match out.get(i + 1) {
            Some(next) => next.time - out[i].time,
            None => aligned[i].end.zip(aligned[i].start).map_or(LAST_LINE_SECS, |(e, s)| (e - s).max(LAST_LINE_SECS)),
        });
    }
    Some(out)
}

/// Onsets only: the study found reference word ends are fill, so a word runs
/// until the next one starts. The last word keeps its aligned length.
fn words(al: &AlignedLine) -> Vec<LyricWord> {
    let n = al.words.len();
    al.words
        .iter()
        .enumerate()
        .map(|(i, w)| LyricWord { time: w.start, text: w.text.clone(), duration: (i + 1 == n).then(|| (w.end - w.start).max(0.0)) })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prealign::align::AlignedWord;

    fn al(start: f64, conf: f64) -> AlignedLine {
        AlignedLine {
            start: Some(start),
            end: Some(start + 1.0),
            conf,
            words: vec![
                AlignedWord { start, end: start + 0.4, text: "a".into() },
                AlignedWord { start: start + 0.5, end: start + 1.0, text: "b".into() },
            ],
        }
    }

    fn src(times: &[f64]) -> Vec<LyricLine> {
        times.iter().map(|&t| LyricLine { time: t, text: "a b".into(), duration: None, words: None }).collect()
    }

    #[test]
    fn a_lost_track_is_dropped_whole() {
        // 7 of 10 lines below 0.15 (70% > 60%).
        let aligned: Vec<_> = (0..10).map(|i| al(i as f64 * 5.0, if i < 7 { 0.1 } else { 0.5 })).collect();
        assert!(track_rejected(&aligned));
        assert!(apply(&src(&[0.0; 10]), false, &aligned).is_none());
        // 6 of 10 (60%) is not more than 60%.
        let aligned: Vec<_> = (0..10).map(|i| al(i as f64 * 5.0, if i < 6 { 0.1 } else { 0.5 })).collect();
        assert!(!track_rejected(&aligned));
    }

    #[test]
    fn low_smoothed_confidence_drops_words_but_keeps_the_line() {
        // Lines 3-4 sit in a low neighbourhood (median 0.05): rejected. Line 0 is
        // low alone but the median of it and its one neighbour is 0.275.
        let confs = [0.05, 0.5, 0.5, 0.05, 0.05, 0.5];
        let aligned: Vec<_> = confs.iter().enumerate().map(|(i, &c)| al(i as f64 * 5.0 + 0.2, c)).collect();
        let estimate = src(&[0.0, 5.0, 10.0, 15.0, 20.0, 25.0]);
        let out = apply(&estimate, false, &aligned).unwrap();
        assert_eq!(out.iter().map(|l| l.words.is_some()).collect::<Vec<_>>(), [true, true, true, false, false, true]);
        // No real timing: the rejected line still takes the aligner's start.
        assert_eq!(out[3].time, 15.2);
        assert_eq!(out[1].words.as_ref().unwrap()[0].time, 5.2);
    }

    #[test]
    fn lines_far_from_the_source_timing_lose_their_words() {
        // The aligner runs a constant 0.3 s late (removed as the track's offset);
        // line 3 is a further 1 s off.
        let lrclib = [0.0, 5.0, 10.0, 15.0, 20.0];
        let aligned: Vec<_> = lrclib.iter().enumerate().map(|(i, &t)| al(t + 0.3 + if i == 3 { 1.0 } else { 0.0 }, 0.5)).collect();
        let out = apply(&src(&lrclib), true, &aligned).unwrap();
        assert_eq!(out.iter().map(|l| l.words.is_some()).collect::<Vec<_>>(), [true, true, true, false, true]);
        assert_eq!(out[3].time, 15.0, "keeps LRCLIB's line time");
        assert_eq!(out[1].time, 5.3, "kept lines take the aligned onset");
        // Without real timing there is nothing to cross-check.
        assert!(apply(&src(&lrclib), false, &aligned).unwrap()[3].words.is_some());
    }

    #[test]
    fn words_run_until_the_next_one_and_times_never_go_backwards() {
        let aligned = vec![al(0.0, 0.5), al(5.0, 0.5), al(9.0, 0.5)];
        let out = apply(&src(&[0.0, 5.0, 9.0]), false, &aligned).unwrap();
        let w = out[0].words.as_ref().unwrap();
        assert_eq!((w[0].duration, w[1].duration), (None, Some(0.5)));
        assert_eq!(out[0].duration, Some(5.0));

        // A rejected line whose source time is earlier than the previous aligned line.
        let aligned = vec![al(10.0, 0.5), al(20.0, 0.5), al(30.0, 0.5), al(40.0, 0.5)];
        let mut lrclib = src(&[10.0, 20.0, 30.0, 40.0]);
        lrclib[2].time = 18.0; // 12 s off: rejected, keeps 18.0, clamped to 20.0
        let out = apply(&lrclib, true, &aligned).unwrap();
        assert_eq!(out[2].time, 20.0);
        assert!(out.windows(2).all(|w| w[0].time <= w[1].time));
    }

    #[test]
    fn a_different_text_does_not_fit() {
        assert!(apply(&src(&[0.0, 1.0]), true, &[al(0.0, 0.5)]).is_none());
    }
}
