//! Put a third-party LRC's timestamps onto another text of the same song.
//!
//! Port of research/timing/bench/align_text.py and common.parse_lrc. Sources
//! differ from LRCLIB in credits, punctuation, script (trad/simp) and repeats,
//! so lines are aligned monotonically by character-bigram similarity and the
//! gaps are interpolated.

use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

/// Below this bigram similarity two lines are never paired.
const MIN_SIM: f64 = 0.35;

/// Credit / header lines that NetEase and QQ put at 0:00.
fn credit_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)^(作词|作詞|作曲|编曲|編曲|词|詞|曲|制作人|製作人|监制|合声|吉他|贝斯|鼓|录音|混音|母带|和声|(Lyrics|Composed|Arranged|Produced|Written|Music)( by)?|Composer|Producer)\s*[:：]",
        )
        .unwrap()
    })
}

fn squash(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_lowercase()
}

/// Credits, and the "Title - Artist" banner line sources put first.
fn is_credit(text: &str, track: &str, artist: &str) -> bool {
    if credit_re().is_match(text) {
        return true;
    }
    let t = squash(text);
    let named = |s: &str| {
        let s = squash(s);
        !s.is_empty() && t.contains(&s)
    };
    text.contains(" - ") && (named(track) || named(artist))
}

/// One `[mm:ss.xx]` tag at the start of `s`: (seconds, rest).
fn leading_tag(s: &str) -> Option<(f64, &str)> {
    let close = s.strip_prefix('[')?.find(']')? + 1;
    let (min, sec) = s[1..close].split_once(':')?;
    if min.is_empty() || !min.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let sec: f64 = sec
        .parse()
        .ok()
        .filter(|v: &f64| v.is_finite() && *v >= 0.0)?;
    Some((min.parse::<f64>().ok()? * 60.0 + sec, &s[close + 1..]))
}

/// Timed lines of an LRC text, sorted, without credits, blanks and the
/// `^translation` suffix some bilingual files carry.
pub fn parse_lrc(lrc: &str, track: &str, artist: &str) -> Vec<(f64, String)> {
    let mut out = Vec::new();
    for raw in lrc.lines() {
        let mut rest = raw.trim();
        let mut times = Vec::new();
        while let Some((t, r)) = leading_tag(rest) {
            times.push(t);
            rest = r;
        }
        let text = rest.split('^').next().unwrap_or("").trim();
        if times.is_empty() || text.is_empty() || is_credit(text, track, artist) {
            continue;
        }
        out.extend(times.into_iter().map(|t| (t, text.to_string())));
    }
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out
}

/// Lowercase letters and digits only, Traditional Chinese and Japanese kanji
/// folded to Simplified: players show 告白氣球 / 周杰倫 / 茅原実里, the
/// sources list 告白气球 / 周杰伦 / 茅原实里.
pub fn norm(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .map(simplified)
        .collect()
}

fn simplified(c: char) -> char {
    static TABLE: OnceLock<HashMap<char, char>> = OnceLock::new();
    let table = TABLE.get_or_init(|| {
        include_str!("ts_chars.txt")
            .lines()
            .filter(|l| !l.starts_with('#'))
            .filter_map(|l| {
                let mut pair = l.chars();
                Some((pair.next()?, pair.next()?))
            })
            .collect()
    });
    table.get(&c).copied().unwrap_or(c)
}

fn grams(s: &str) -> HashSet<String> {
    let c: Vec<char> = norm(s).chars().collect();
    match c.len() {
        0 => HashSet::new(),
        1 => HashSet::from([c[0].to_string()]),
        _ => c.windows(2).map(|w| w.iter().collect()).collect(),
    }
}

/// Dice coefficient over character bigrams.
pub fn dice(a: &str, b: &str) -> f64 {
    let (a, b) = (grams(a), grams(b));
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    2.0 * a.intersection(&b).count() as f64 / (a.len() + b.len()) as f64
}

/// For each target line, the time of the source line it pairs with, if any.
/// Monotonic: pairs never cross, and each source line is used at most once.
pub fn map_lines(target: &[String], source: &[(f64, String)]) -> Vec<Option<f64>> {
    let (n, m) = (target.len(), source.len());
    // Best total similarity of target[..i] against source[..j], and the move
    // that got there: 0 pair, 1 skip target, 2 skip source.
    let mut score = vec![vec![0.0f64; m + 1]; n + 1];
    let mut step = vec![vec![0u8; m + 1]; n + 1];
    for i in 1..=n {
        for j in 1..=m {
            let mut best = (score[i - 1][j], 1u8);
            // Ties go to the larger move code, as Python's max() over tuples does.
            let skip_source = (score[i][j - 1], 2u8);
            if skip_source.0 >= best.0 {
                best = skip_source;
            }
            let s = dice(&target[i - 1], &source[j - 1].1);
            if s >= MIN_SIM && score[i - 1][j - 1] + s > best.0 {
                best = (score[i - 1][j - 1] + s, 0);
            }
            (score[i][j], step[i][j]) = best;
        }
    }
    let mut out = vec![None; n];
    let (mut i, mut j) = (n, m);
    while i > 0 && j > 0 {
        match step[i][j] {
            0 => {
                out[i - 1] = Some(source[j - 1].0);
                i -= 1;
                j -= 1;
            }
            1 => i -= 1,
            _ => j -= 1,
        }
    }
    out
}

/// Fill unpaired lines linearly between their paired neighbours, with 0 s
/// and the track end as the outer anchors.
pub fn interpolate(times: &[Option<f64>], duration: f64) -> Vec<f64> {
    let n = times.len() as isize;
    let mut anchors = vec![(-1isize, 0.0f64)];
    anchors.extend(
        times
            .iter()
            .enumerate()
            .filter_map(|(i, t)| t.map(|t| (i as isize, t))),
    );
    anchors.push((n, duration));
    let mut out: Vec<f64> = times.iter().map(|t| t.unwrap_or(0.0)).collect();
    for w in anchors.windows(2) {
        let ((i0, t0), (i1, t1)) = (w[0], w[1]);
        for k in i0 + 1..i1 {
            out[k as usize] = t0 + (t1 - t0) * (k - i0) as f64 / (i1 - i0) as f64;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn parses_tags_and_drops_credits() {
        let lrc = "[ti:恋]\n[00:00.00]恋 - 茅原实里 (ちはらみのり)\n[00:08.15]词：茅原実里\n\
                   [00:01.00] 作曲 : 堀江晶太\n[00:24.45]眠れない夜\n[00:29.61]\n\
                   [01:02.5][02:10.123]repeat^translation\n[00:30.16]今頃";
        let got = parse_lrc(lrc, "恋", "茅原実里");
        assert_eq!(
            got,
            vec![
                (24.45, "眠れない夜".to_string()),
                (30.16, "今頃".to_string()),
                (62.5, "repeat".to_string()),
                (130.123, "repeat".to_string()),
            ]
        );
    }

    #[test]
    fn norm_folds_traditional_to_simplified() {
        assert_eq!(norm("告白氣球"), norm("告白气球"));
        assert_eq!(norm("草東沒有派對"), "草东没有派对");
        assert_eq!(norm("茅原実里"), norm("茅原实里"));
        assert_eq!(norm("ヨルシカ 雪国!"), "ヨルシカ雪国");
    }

    #[test]
    fn dice_ignores_case_and_punctuation() {
        assert_eq!(dice("Hello, World!", "hello world"), 1.0);
        assert_eq!(dice("...", "..."), 0.0);
        assert!(dice("眠れない夜 脳裏をよぎる", "眠れない夜、脳裏をよぎる") > 0.99);
        assert!(dice("abc", "xyz") == 0.0);
    }

    #[test]
    fn maps_monotonically_and_skips_extras() {
        let target = s(&[
            "first line here",
            "second line here",
            "a line not in source",
            "last line here",
        ]);
        let source = vec![
            (1.0, "Intro credit".to_string()),
            (10.0, "First line here!".to_string()),
            (20.0, "second line here".to_string()),
            (30.0, "last line here".to_string()),
        ];
        assert_eq!(
            map_lines(&target, &source),
            vec![Some(10.0), Some(20.0), None, Some(30.0)]
        );
    }

    #[test]
    fn repeated_lines_take_successive_source_times() {
        let target = s(&["chorus line", "verse", "chorus line"]);
        let source = vec![
            (5.0, "chorus line".to_string()),
            (9.0, "verse".to_string()),
            (14.0, "chorus line".to_string()),
        ];
        assert_eq!(
            map_lines(&target, &source),
            vec![Some(5.0), Some(9.0), Some(14.0)]
        );
    }

    #[test]
    fn interpolates_between_anchors() {
        let got = interpolate(&[None, Some(10.0), None, None, Some(40.0), None], 100.0);
        assert_eq!(got, vec![5.0, 10.0, 20.0, 30.0, 40.0, 70.0]);
        assert_eq!(
            interpolate(&[None, None, None], 40.0),
            vec![10.0, 20.0, 30.0]
        );
    }
}
