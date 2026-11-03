//! Timing for lyrics that arrive without timestamps.
//!
//! See docs/research/timing/REPORT.md for the benchmark behind these choices.
//! Order of preference for a plain-only LRCLIB hit:
//!   1. another LRCLIB entry of the same song (same length +-3 s) that is synced;
//!   2. a syllable-weighted estimate over the track length (last resort; still
//!      ~10 s off on average, so callers must not cache it as if it were real).

use crate::types::LyricLine;

/// Max length difference for two LRCLIB entries to count as the same recording.
const SAME_RECORDING_SECS: f64 = 3.0;
/// Share of the track assumed to be intro / outro when estimating.
const INTRO_SHARE: f64 = 0.08;
const OUTRO_SHARE: f64 = 0.08;
/// Per-line weight added to the syllable count (breath / gap between lines).
const LINE_GAP_WEIGHT: f64 = 2.0;
/// Used when the track length is unknown (the old behaviour).
const FALLBACK_LINE_SECS: f64 = 5.0;

/// Rough count of sung syllables: one per CJK / kana / hangul character, one
/// per Latin vowel group.
fn syllables(text: &str) -> usize {
    let mut count = 0;
    let mut in_vowels = false;
    for c in text.chars() {
        let cjk = matches!(c, '\u{3040}'..='\u{30ff}' | '\u{3400}'..='\u{9fff}' | '\u{ac00}'..='\u{d7af}');
        let vowel = matches!(c.to_ascii_lowercase(), 'a' | 'e' | 'i' | 'o' | 'u' | 'y');
        if cjk {
            count += 1;
        }
        if vowel && !in_vowels {
            count += 1;
        }
        in_vowels = vowel;
    }
    count.max(1)
}

/// Spread plain lyric lines over the track, weighted by syllables.
pub fn estimate_plain(plain: &str, track_secs: f64) -> Vec<LyricLine> {
    let lines: Vec<&str> = plain.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let weights: Vec<f64> = if track_secs > 0.0 {
        lines.iter().map(|l| syllables(l) as f64 + LINE_GAP_WEIGHT).collect()
    } else {
        vec![1.0; lines.len()]
    };
    let total: f64 = weights.iter().sum();
    let (start, span) = if track_secs > 0.0 {
        let start = track_secs * INTRO_SHARE;
        (start, track_secs * (1.0 - OUTRO_SHARE) - start)
    } else {
        (0.0, total * FALLBACK_LINE_SECS)
    };

    let mut t = start;
    lines
        .iter()
        .zip(&weights)
        .map(|(text, w)| {
            let duration = w / total * span;
            let line = LyricLine { time: t, text: text.to_string(), duration: Some(duration) };
            t += duration;
            line
        })
        .collect()
}

/// From LRCLIB search results, pick the synced entry closest in length to
/// `track_secs`, if one is within `SAME_RECORDING_SECS`.
pub fn pick_synced_alternative(results: &[serde_json::Value], track_secs: f64) -> Option<String> {
    results
        .iter()
        .filter_map(|r| {
            let synced = r["syncedLyrics"].as_str().filter(|s| !s.trim().is_empty())?;
            let gap = (r["duration"].as_f64()? - track_secs).abs();
            (gap <= SAME_RECORDING_SECS).then_some((gap, synced))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, synced)| synced.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn syllables_cover_scripts() {
        assert_eq!(syllables("夢ならば"), 4);
        assert_eq!(syllables("보고 싶다"), 4);
        assert_eq!(syllables("I've been tryna call"), 6);
        assert_eq!(syllables("..."), 1);
    }

    #[test]
    fn estimate_skips_intro_and_fills_track() {
        let lines = estimate_plain("first line\n\nsecond line here\nthird", 200.0);
        assert_eq!(lines.len(), 3);
        assert!((lines[0].time - 16.0).abs() < 1e-9);
        let end = lines.last().map(|l| l.time + l.duration.unwrap()).unwrap();
        assert!((end - 184.0).abs() < 1e-9);
        assert!(lines.windows(2).all(|w| w[0].time < w[1].time));
    }

    #[test]
    fn estimate_without_length_keeps_old_spacing() {
        let lines = estimate_plain("a\nb\nc", 0.0);
        let times: Vec<f64> = lines.iter().map(|l| l.time).collect();
        assert_eq!(times, vec![0.0, 5.0, 10.0]);
    }

    #[test]
    fn alternative_must_be_same_recording() {
        let results = vec![
            json!({"duration": 300.0, "syncedLyrics": "[00:01.00]live"}),
            json!({"duration": 201.0, "syncedLyrics": "[00:01.00]studio"}),
            json!({"duration": 200.0, "syncedLyrics": null, "plainLyrics": "plain"}),
            json!({"duration": 199.5, "syncedLyrics": "  "}),
        ];
        assert_eq!(pick_synced_alternative(&results, 200.0).as_deref(), Some("[00:01.00]studio"));
        assert_eq!(pick_synced_alternative(&results, 250.0), None);
    }
}
