//! Reading the model's numbered reply back onto the lyric lines.

use log::warn;
use unicode_normalization::UnicodeNormalization;

use super::types::{LyricLine, TranslatedLyricLine};

/// One translation per original line, or an error saying why the reply isn't
/// one. Never a guess: a reply that doesn't fit (a refusal, a summary) is a
/// failed attempt, and the caller asks another provider. Stretching it over
/// the lyrics would show, and cache, nonsense.
///
/// Lines are matched by their number. A line the reply skips keeps its
/// original text (confidence 0); more than half skipped is an error.
pub fn parse_translation_response(
    reply: &str,
    original_lyrics: &[LyricLine],
    detected_language: &str,
) -> Result<Vec<TranslatedLyricLine>, String> {
    let n = original_lyrics.len();
    let mut found: Vec<Option<String>> = vec![None; n];
    for (number, text) in reply.lines().filter_map(numbered) {
        if let Some(slot) = number.checked_sub(1).and_then(|i| found.get_mut(i)) {
            slot.get_or_insert(text); // a repeated number keeps its first answer
        }
    }

    let missing = found.iter().filter(|t| t.is_none()).count();
    if missing * 2 > n {
        let start: String = reply.trim().chars().take(120).collect();
        warn!("Translation reply fits {} of {} lines; it begins: {}", n - missing, n, start);
        return Err(format!("the reply translates only {} of {} lines", n - missing, n));
    }
    if missing > 0 {
        warn!("Translation reply skipped {missing} of {n} lines; they stay untranslated");
    }

    Ok(original_lyrics
        .iter()
        .zip(found)
        .map(|(line, text)| {
            let confidence = if text.is_some() { 0.95 } else { 0.0 };
            let text = text.unwrap_or_else(|| line.text.clone());
            TranslatedLyricLine {
                time: line.time,
                text: text.clone(),
                duration: line.duration,
                original_text: line.text.clone(),
                translated_text: text,
                translation_confidence: Some(confidence),
                detected_language: Some(detected_language.to_string()),
            }
        })
        .collect())
}

/// `12. text` (also `12)`, `12、`, `12:`, full-width digits) as (12, "text").
/// Only the number and its mark are normalized: the translation keeps its own
/// characters (full-width `，` stays). A numbered line with nothing after the
/// number is an empty translation of an empty or instrumental line, and
/// still counts.
fn numbered(line: &str) -> Option<(usize, String)> {
    // Markdown around the number ("**1.**", "- 1.", "[1]") is still a number.
    let line = line.trim().trim_start_matches(['*', '-', '#', '[', ' ']);
    // ASCII or full-width digits only: "²" or "½" would normalize to a number too.
    let digits = line.find(|c: char| !(c.is_ascii_digit() || ('０'..='９').contains(&c)))?;
    let number = line[..digits].nfkc().collect::<String>().parse().ok()?;
    let mut rest = line[digits..].chars();
    let mark = rest.next()?.nfkc().next()?;
    let text = rest.as_str().trim_start_matches(['*', ' ']).trim();
    ['.', ')', ']', '、', ':'].contains(&mark).then(|| (number, text.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(texts: &[&str]) -> Vec<LyricLine> {
        texts
            .iter()
            .enumerate()
            .map(|(i, t)| LyricLine { time: i as f64, text: t.to_string(), duration: None })
            .collect()
    }

    fn parse(reply: &[&str], originals: &[&str]) -> Result<Vec<(String, f64)>, String> {
        let out = parse_translation_response(&reply.join("\n"), &lines(originals), "ja")?;
        Ok(out.into_iter().map(|l| (l.translated_text, l.translation_confidence.unwrap())).collect())
    }

    #[test]
    fn lines_are_matched_by_number_not_position() {
        let out = parse(&["2. B", "1. A", "3) C"], &["a", "b", "c"]).unwrap();
        assert_eq!(out, vec![("A".into(), 0.95), ("B".into(), 0.95), ("C".into(), 0.95)]);
    }

    #[test]
    fn full_width_numbers_and_other_punctuation_count() {
        let out = parse(&["１．你好，世界", "2、世界？", "3: !"], &["a", "b", "c"]).unwrap();
        // The translation's own full-width punctuation is kept as written.
        assert_eq!(out.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>(), ["你好，世界", "世界？", "!"]);
    }

    #[test]
    fn markdown_around_the_number_is_still_a_number() {
        let out = parse(&["**1.** A", "- 2. B", "[3] C", "### 4) D"], &["a", "b", "c", "d"]).unwrap();
        assert_eq!(out.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>(), ["A", "B", "C", "D"]);
    }

    #[test]
    fn a_skipped_line_keeps_its_original_and_does_not_shift_the_rest() {
        let out = parse(&["1. A", "3. C"], &["a", "b", "c"]).unwrap();
        assert_eq!(out, vec![("A".into(), 0.95), ("b".into(), 0.0), ("C".into(), 0.95)]);
    }

    #[test]
    fn a_reply_that_is_not_a_translation_is_an_error() {
        let five = ["a", "b", "c", "d", "e"];
        // A refusal with a summary: no numbers at all.
        assert!(parse(&["I can't reproduce these lyrics.", "Here is a summary:", "It is about loneliness."], &five).is_err());
        // Numbered, but most lines missing: 3 of 5 is an error, 2 of 5 is kept.
        assert!(parse(&["1. A", "2. B"], &five).is_err());
        assert!(parse(&["1. A", "2. B", "3. C"], &five).is_ok());
        // A summary with a numbered list in it doesn't pass for the lyrics either.
        assert!(parse(&["Here is what the song covers:", "1. Loneliness", "2. Loss"], &five).is_err());
    }

    #[test]
    fn only_numbered_lines_count() {
        // A translation wrapped onto a second line keeps only its first part:
        // folding unnumbered lines in would also fold in a model's notes.
        let out = parse(&["1. first half", "second half", "2. B"], &["a", "b"]).unwrap();
        assert_eq!(out.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>(), ["first half", "B"]);
        // A year opening a line is out of range, and other numerals aren't numbers.
        let out = parse(&["1. A", "2024. was a year", "½. half", "²) two", "2. B"], &["a", "b"]).unwrap();
        assert_eq!(out.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>(), ["A", "B"]);
    }

    #[test]
    fn notes_and_out_of_range_numbers_are_ignored() {
        let out = parse(&["Here you go:", "1. A", "2. B", "9. Z", "(note)"], &["a", "b"]).unwrap();
        assert_eq!(out, vec![("A".into(), 0.95), ("B".into(), 0.95)]);
    }
}
