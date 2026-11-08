//! The study's reject rule, and how aligned timing is laid over the lyrics the
//! lookup found (docs/research/prealign/REPORT.md, "Robustness and the reject rule").
//!
//! 1. More than 50% of a track's lines below confidence 0.20: drop the whole
//!    alignment (the model knows it is lost). The study's MMS rule was 60% below
//!    0.15; echora's confidence runs higher, see docs/research/prealign/ALIGNERS.md.
//! 2. Otherwise a line loses its aligned words if the median confidence of it
//!    and its neighbours is below 0.10, or, when the lyrics came with real line
//!    timing, if its onset is more than 0.7 s off that timing (after removing
//!    the track's median offset, as the study measured it). A line the model
//!    heard clearly (confidence 0.4+) is believed over the source either way,
//!    unless it sits nearer a neighbouring line's time than its own (the wrong
//!    phrase): sources stamp when a line should show, not when it is sung.
//! 3. A rejected line keeps the source's line time if it had real timing, else
//!    the aligner's line start, and gets estimated words from that time on, at
//!    the song's own pace, so it sweeps like its neighbours.

use super::align::{AlignedLine, AlignedWord};
use crate::timing::syllables;
use crate::types::{LyricLine, LyricWord};

const TRACK_LOW_CONF: f64 = 0.20;
const TRACK_LOW_SHARE: f64 = 0.5;
const LINE_MIN_CONF: f64 = 0.10;
const MAX_OFF_SOURCE: f64 = 0.7;
/// A line heard at least this clearly outvotes the source's timing.
const SURE_CONF: f64 = 0.4;
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
    let centre = offset(aligned, source);

    let mut keep = vec![false; aligned.len()];
    for (k, &i) in timed.iter().enumerate() {
        let confident = smoothed[k] >= LINE_MIN_CONF;
        let at = aligned[i].start.unwrap_or(0.0) - centre;
        let sure = confs[k] >= SURE_CONF && own_phrase(source, i, at);
        let agrees = !real_timing || (off[k] - centre).abs() <= MAX_OFF_SOURCE || sure;
        keep[i] = confident && agrees;
    }
    keep
}

/// How far the singing runs from the source's stamps, typically: the median of
/// aligned start minus source time. Sources stamp when a line should appear,
/// often a beat before it is sung.
fn offset(aligned: &[AlignedLine], source: &[LyricLine]) -> f64 {
    let mut off: Vec<f64> = aligned.iter().zip(source).filter_map(|(al, line)| Some(al.start? - line.time)).collect();
    if off.is_empty() {
        0.0
    } else {
        median(&mut off)
    }
}

/// Is `at` nearer line `i`'s source time than either neighbour's?
fn own_phrase(source: &[LyricLine], i: usize, at: f64) -> bool {
    let own = (at - source[i].time).abs();
    [i.wrapping_sub(1), i + 1].iter().filter_map(|&j| source.get(j)).all(|n| own < (at - n.time).abs())
}

/// The lookup's lines re-timed by the alignment. `real_timing`: the lines'
/// times are a real source's (LRCLIB synced, QQ, ...), not an estimate. None if
/// the alignment does not fit these lines or is rejected as a whole.
pub fn apply(source: &[LyricLine], real_timing: bool, aligned: &[AlignedLine]) -> Option<Vec<LyricLine>> {
    if aligned.len() != source.len() || track_rejected(aligned) {
        return None;
    }
    let keep = keep(aligned, source, real_timing);
    let pace = pace(aligned, &keep);
    // A line whose words are not kept starts where its neighbours do relative
    // to the source, so its estimated sweep keeps in step with theirs.
    let shift = if real_timing { offset(aligned, source) } else { 0.0 };
    let mut out: Vec<LyricLine> = source
        .iter()
        .zip(aligned)
        .zip(&keep)
        .map(|((line, al), &kept)| {
            let time = match al.start {
                Some(start) if kept || !real_timing => start,
                _ => line.time + shift,
            };
            LyricLine { time, text: line.text.clone(), duration: None, words: None }
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
    for ((line, al), &kept) in out.iter_mut().zip(aligned).zip(&keep) {
        let words = if kept { words(al) } else { estimate(al, line.time, line.duration.unwrap_or(0.0), pace) };
        line.words = (!words.is_empty()).then_some(words);
    }
    Some(out)
}

/// A word's share of a line's singing.
fn weight(words: &[AlignedWord]) -> f64 {
    words.iter().map(|w| syllables(&w.text) as f64).sum()
}

/// The song's singing pace, seconds per syllable, over its kept lines.
/// Unknown (infinite) when no line was kept.
fn pace(aligned: &[AlignedLine], keep: &[bool]) -> f64 {
    let mut v: Vec<f64> = aligned
        .iter()
        .zip(keep)
        .filter(|(_, &kept)| kept)
        .filter_map(|(al, _)| Some((al.end? - al.start?) / weight(&al.words)))
        .collect();
    if v.is_empty() {
        f64::INFINITY
    } else {
        median(&mut v)
    }
}

/// Words for a line whose aligned ones were not kept: the aligner's pieces
/// (so the text splits the same way), sung from `start` at the song's pace but
/// never past the line's `duration`, each as long as its syllables.
fn estimate(al: &AlignedLine, start: f64, duration: f64, pace: f64) -> Vec<LyricWord> {
    let total = weight(&al.words);
    let sung = (pace * total).min(duration);
    let mut t = start;
    let n = al.words.len();
    al.words
        .iter()
        .enumerate()
        .map(|(i, w)| {
            let len = sung * syllables(&w.text) as f64 / total;
            let word = LyricWord { time: t, text: w.text.clone(), duration: (i + 1 == n).then_some(len) };
            t += len;
            word
        })
        .collect()
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

    fn al(start: f64, conf: f64) -> AlignedLine {
        AlignedLine {
            start: Some(start),
            end: Some(start + 1.0),
            conf,
            words: vec![
                AlignedWord { start, end: start + 0.4, text: "a".into() },
                AlignedWord { start: start + 0.3, end: start + 1.0, text: "b".into() },
            ],
        }
    }

    fn src(times: &[f64]) -> Vec<LyricLine> {
        times.iter().map(|&t| LyricLine { time: t, text: "a b".into(), duration: None, words: None }).collect()
    }

    #[test]
    fn a_lost_track_is_dropped_whole() {
        // 6 of 10 lines below 0.20 (60% > 50%).
        let aligned: Vec<_> = (0..10).map(|i| al(i as f64 * 5.0, if i < 6 { 0.19 } else { 0.5 })).collect();
        assert!(track_rejected(&aligned));
        assert!(apply(&src(&[0.0; 10]), false, &aligned).is_none());
        // 5 of 10 (50%) is not more than 50%.
        let aligned: Vec<_> = (0..10).map(|i| al(i as f64 * 5.0, if i < 5 { 0.19 } else { 0.5 })).collect();
        assert!(!track_rejected(&aligned));
    }

    /// Per line: are its words the aligner's (true) or estimated (false)?
    fn aligned_words(out: &[LyricLine], aligned: &[AlignedLine]) -> Vec<bool> {
        let times = |w: &[LyricWord]| w.iter().map(|w| w.time).collect::<Vec<_>>();
        out.iter().zip(aligned).map(|(l, al)| times(l.words.as_ref().unwrap()) == al.words.iter().map(|w| w.start).collect::<Vec<_>>()).collect()
    }

    #[test]
    fn low_smoothed_confidence_drops_words_but_keeps_the_line() {
        // Lines 3-4 sit in a low neighbourhood (median 0.05): rejected. Line 0 is
        // low alone but the median of it and its one neighbour is 0.275.
        let confs = [0.05, 0.5, 0.5, 0.05, 0.05, 0.5];
        let aligned: Vec<_> = confs.iter().enumerate().map(|(i, &c)| al(i as f64 * 5.0 + 0.2, c)).collect();
        let estimate = src(&[0.0, 5.0, 10.0, 15.0, 20.0, 25.0]);
        let out = apply(&estimate, false, &aligned).unwrap();
        assert_eq!(aligned_words(&out, &aligned), [true, true, true, false, false, true]);
        // No real timing: the rejected line still takes the aligner's start.
        assert_eq!(out[3].time, 15.2);
        assert_eq!(out[1].words.as_ref().unwrap()[0].time, 5.2);
    }

    #[test]
    fn lines_far_from_the_source_timing_lose_their_words() {
        // The aligner runs a constant 0.3 s late (removed as the track's offset);
        // line 3, heard unsurely, is a further 1 s early.
        let lrclib = [0.0, 5.0, 10.0, 15.0, 20.0];
        let aligned: Vec<_> =
            lrclib.iter().enumerate().map(|(i, &t)| if i == 3 { al(t + 0.3 - 1.0, 0.3) } else { al(t + 0.3, 0.5) }).collect();
        let out = apply(&src(&lrclib), true, &aligned).unwrap();
        assert_eq!(aligned_words(&out, &aligned), [true, true, true, false, true]);
        assert_eq!(out[3].time, 15.3, "LRCLIB's time, moved by the track's 0.3 s offset like its neighbours");
        assert_eq!(out[1].time, 5.3, "kept lines take the aligned onset");
        // Without real timing there is nothing to cross-check.
        assert!(aligned_words(&apply(&src(&lrclib), false, &aligned).unwrap(), &aligned)[3]);
    }

    #[test]
    fn a_clearly_heard_line_outvotes_the_source() {
        // Line 2 is `shift` s off the source. Heard clearly (0.544, The Third
        // Planet's "ピアノみたい"), the aligner wins either way; heard faintly (Stay
        // at Your House 50-54: 0.08-0.19, 1.6-3 s early), the source does.
        let lrclib = [0.0, 5.0, 10.0, 15.0, 20.0];
        let run = |shift: f64, conf: f64| {
            let aligned: Vec<_> = lrclib.iter().enumerate().map(|(i, &t)| al(t + if i == 2 { shift } else { 0.0 }, if i == 2 { conf } else { 0.5 })).collect();
            aligned_words(&apply(&src(&lrclib), true, &aligned).unwrap(), &aligned)[2]
        };
        assert!(run(1.4, 0.544));
        assert!(run(-1.6, 0.544));
        assert!(!run(1.4, 0.3));
        assert!(!run(-1.6, 0.19));
        assert!(!run(-3.0, 0.08));
        // Nearer the next line's time than its own: the wrong phrase, however clear.
        assert!(run(2.4, 0.9));
        assert!(!run(2.6, 0.9));
        assert!(!run(-2.6, 0.9));
    }

    #[test]
    fn a_dropped_line_sweeps_at_the_songs_pace() {
        // Kept lines sing 2 syllables in 1 s. Line 1 comes 2 s early, unsure, and
        // is dropped; the next line is 15 s after its source time.
        let lrclib = [0.0, 5.0, 20.0, 25.0];
        let mut aligned: Vec<_> = lrclib.iter().map(|&t| al(t, 0.5)).collect();
        aligned[1] = al(3.0, 0.3);
        aligned[1].words.push(AlignedWord { start: 3.6, end: 3.9, text: "together".into() });
        let mut lines = src(&lrclib);
        lines[1].text = "a b, together!".into();
        let out = apply(&lines, true, &aligned).unwrap();
        let w = out[1].words.as_ref().unwrap();
        assert_eq!(w.iter().map(|w| w.text.as_str()).collect::<Vec<_>>(), ["a", "b", "together"], "the aligner's pieces");
        assert_eq!(w[0].time, 5.0, "starts at the line time");
        assert!(w.windows(2).all(|p| p[0].time < p[1].time));
        // a + b + to-ge-ther = 5 syllables at 0.5 s: sung by 7.5 s, not stretched to 20.
        let end = w[2].time + w[2].duration.unwrap();
        assert!((end - 7.5).abs() < 1e-9, "{end}");
        assert!((w[1].time - 5.5).abs() < 1e-9 && (w[2].time - 6.0).abs() < 1e-9);
        assert!(w[..2].iter().all(|w| w.duration.is_none()), "like aligned words, they run to the next one");
    }

    #[test]
    fn estimated_words_never_run_into_the_next_line() {
        // Lines 1.2 s apart; line 1 is dropped (early) and has 2 syllables: at 0.5 s
        // per syllable that would be 1 s, but no faster pace fits than the line's own.
        let lrclib = [0.0, 1.2, 1.8, 4.0, 8.0];
        let mut aligned: Vec<_> = lrclib.iter().map(|&t| al(t, 0.5)).collect();
        aligned[2] = al(0.9, 0.3);
        let out = apply(&src(&lrclib), true, &aligned).unwrap();
        assert!(!aligned_words(&out, &aligned)[2]);
        let w = out[2].words.as_ref().unwrap();
        let end = w[1].time + w[1].duration.unwrap();
        assert_eq!(w[0].time, out[2].time);
        assert!(end <= out[3].time + 1e-9, "{end} > {}", out[3].time);
    }

    #[test]
    fn with_no_line_kept_a_dropped_line_spans_its_whole_time() {
        let lrclib = [0.0, 5.0, 10.0, 15.0];
        // Each aligned line 3 s from its source, early or late and unsure: all dropped.
        let aligned = vec![al(-3.0, 0.3), al(8.0, 0.2), al(7.0, 0.3), al(18.0, 0.2)];
        let out = apply(&src(&lrclib), true, &aligned).unwrap();
        assert_eq!(aligned_words(&out, &aligned), [false, false, false, false]);
        let w = out[1].words.as_ref().unwrap();
        assert_eq!((w[0].time, w[1].time, w[1].duration), (5.0, 7.5, Some(2.5)));
    }

    #[test]
    fn words_run_until_the_next_one_and_times_never_go_backwards() {
        let aligned = vec![al(0.0, 0.5), al(5.0, 0.5), al(9.0, 0.5)];
        let out = apply(&src(&[0.0, 5.0, 9.0]), false, &aligned).unwrap();
        let w = out[0].words.as_ref().unwrap();
        assert_eq!(w[0].duration, None);
        assert!((w[1].duration.unwrap() - 0.7).abs() < 1e-9);
        assert_eq!(out[0].duration, Some(5.0));

        // A rejected line whose source time is earlier than the previous aligned line.
        let aligned = vec![al(10.0, 0.5), al(20.0, 0.5), al(30.0, 0.3), al(40.0, 0.5)];
        let mut lrclib = src(&[10.0, 20.0, 30.0, 40.0]);
        lrclib[2].time = 18.0; // 12 s off: rejected, keeps 18.0, clamped to 20.0
        let out = apply(&lrclib, true, &aligned).unwrap();
        assert_eq!(out[2].time, 20.0);
        assert!(out.windows(2).all(|w| w[0].time <= w[1].time));
    }

    #[test]
    fn nothing_aligned_is_rejected() {
        let none = AlignedLine { start: None, end: None, conf: 0.0, words: vec![] };
        assert!(apply(&src(&[0.0, 5.0, 10.0]), false, &[none.clone(), none.clone(), none]).is_none());
    }

    #[test]
    fn a_different_text_does_not_fit() {
        assert!(apply(&src(&[0.0, 1.0]), true, &[al(0.0, 0.5)]).is_none());
    }
}
