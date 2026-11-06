//! Translation response parsing

use log::{info, warn};
use unicode_normalization::UnicodeNormalization;

use super::prompt::TRANSLATION_SEPARATOR;
use super::types::{LyricLine, TranslatedLyricLine};

/// One translation per original line, or an error saying why the reply isn't
/// one. Never a guess: a reply that doesn't fit (a refusal, a summary, lines
/// merged or dropped) is a failed attempt, and the caller retries in smaller
/// pieces. Stretching it over the lyrics would show, and cache, nonsense.
pub fn parse_translation_response(
    response_text: &str,
    original_lyrics: &[LyricLine],
    detected_language: &str,
) -> Result<(Vec<TranslatedLyricLine>, bool), String> {
    let response_text = response_text.trim();

    // Clean the response text - split by lines and filter empty
    let cleaned_response: Vec<&str> = response_text
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect();

    // Check if the response uses the new format (original<sep>translated)
    let uses_new_format = cleaned_response
        .iter()
        .any(|line| line.contains(TRANSLATION_SEPARATOR));

    if uses_new_format {
        return parse_paired_format(&cleaned_response, original_lyrics, detected_language);
    }
    info!("[Parse Path: LEGACY_FORMAT] Falling back to legacy translation-only format");
    parse_legacy_format(&cleaned_response, original_lyrics, detected_language)
}

/// The LLM echoes each original line before its translation. Echoes are
/// matched loosely (`echo_key`), in order. An echo that doesn't match is still
/// this line's, slightly rewritten, unless it is a later line's: then the LLM
/// skipped this one, and it stays untranslated rather than shifting every
/// translation after it onto the wrong line.
fn parse_paired_format(
    cleaned_response: &[&str],
    original_lyrics: &[LyricLine],
    detected_language: &str,
) -> Result<(Vec<TranslatedLyricLine>, bool), String> {
    info!("Parsing paired format with separator: '{}'", TRANSLATION_SEPARATOR);
    let pairs: Vec<(String, &str, &str)> = cleaned_response
        .iter()
        .filter_map(|l| l.split_once(TRANSLATION_SEPARATOR))
        .map(|(echo, translated)| (echo_key(echo), echo.trim(), translated.trim()))
        .collect();
    let keys: Vec<String> = original_lyrics.iter().map(|l| echo_key(&l.text)).collect();

    let mut used = vec![false; pairs.len()];
    let mut next = 0;
    let mut misses = 0;
    let mut out = Vec::with_capacity(original_lyrics.len());
    for (i, line) in original_lyrics.iter().enumerate() {
        let exact = (next..pairs.len()).chain(0..next).find(|&j| !used[j] && pairs[j].0 == keys[i]);
        let in_order = (next..pairs.len()).find(|&j| !used[j]).filter(|&j| !keys[i + 1..].contains(&pairs[j].0));
        let (translated, confidence) = match exact.or(in_order) {
            Some(j) => {
                used[j] = true;
                next = j + 1;
                if exact.is_none() {
                    misses += 1;
                    warn!("[Parse Path: NEW_FORMAT_MISMATCH] Line {}: '{}' echoed as '{}'", i, line.text.trim(), pairs[j].1);
                }
                (pairs[j].2.to_string(), if exact.is_some() { 0.98 } else { 0.5 })
            }
            None => {
                misses += 1;
                warn!("[Parse Path: NEW_FORMAT_MISSING] Line {}: '{}' has no translation", i, line.text.trim());
                (line.text.clone(), 0.0)
            }
        };
        out.push(TranslatedLyricLine {
            time: line.time,
            text: translated.clone(),
            duration: line.duration,
            original_text: line.text.clone(),
            translated_text: translated,
            translation_confidence: Some(confidence),
            detected_language: Some(detected_language.to_string()),
        });
    }

    if misses == 0 {
        info!("[Parse Path: NEW_FORMAT_SUCCESS] Paired format parsing successful: {} lines", out.len());
    } else if misses * 2 > out.len() {
        return Err(format!("the reply fits only {} of {} lines", out.len() - misses, out.len()));
    } else {
        warn!("[Parse Path: NEW_FORMAT_PARTIAL] Completed with {} mismatches out of {} lines", misses, out.len());
    }
    Ok((out, false))
}

/// What an echo is compared by: width and case folded (NFKC, lowercase),
/// spaces and punctuation dropped. LLMs swap full-width and ASCII spaces,
/// add or drop a comma; the line is still the same line.
fn echo_key(s: &str) -> String {
    s.nfkc().flat_map(char::to_lowercase).filter(|c| c.is_alphanumeric()).collect()
}

/// A reply without the separators: usable only if it has exactly one line per
/// original, in order.
fn parse_legacy_format(
    cleaned_response: &[&str],
    original_lyrics: &[LyricLine],
    detected_language: &str,
) -> Result<(Vec<TranslatedLyricLine>, bool), String> {
    if cleaned_response.len() != original_lyrics.len() {
        let first = cleaned_response.first().map(|l| l.chars().take(120).collect::<String>()).unwrap_or_default();
        warn!(
            "[Parse Path: LEGACY_MISMATCH] {} line(s) for {}, not a translation; it begins: {}",
            cleaned_response.len(),
            original_lyrics.len(),
            first
        );
        return Err(format!("the reply has {} lines for {}", cleaned_response.len(), original_lyrics.len()));
    }

    // Perfect match - proceed normally
    info!(
        "[Parse Path: LEGACY_SUCCESS] Legacy format parsing successful: {} lines",
        cleaned_response.len()
    );

    let mut translated_lines = Vec::new();

    for (i, original_line) in original_lyrics.iter().enumerate() {
        let translated_text = cleaned_response[i].to_string();

        translated_lines.push(TranslatedLyricLine {
            time: original_line.time,
            text: translated_text.clone(),
            duration: original_line.duration,
            original_text: original_line.text.clone(),
            translated_text,
            translation_confidence: Some(0.95),
            detected_language: Some(detected_language.to_string()),
        });
    }

    Ok((translated_lines, false))
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

    /// `|` stands for the separator.
    fn parse(reply: &[&str], originals: &[&str]) -> Vec<(String, f64)> {
        let reply: Vec<String> = reply.iter().map(|r| r.replace('|', TRANSLATION_SEPARATOR)).collect();
        let reply: Vec<&str> = reply.iter().map(String::as_str).collect();
        let (out, _) = parse_paired_format(&reply, &lines(originals), "ja").unwrap();
        out.into_iter().map(|l| (l.translated_text, l.translation_confidence.unwrap())).collect()
    }

    #[test]
    fn echoes_match_across_space_width_and_punctuation() {
        let out = parse(&["あの日の 森の中|那天 森林中", "Hello, world!|你好世界"], &["あの日の　森の中", "hello world"]);
        assert_eq!(out, vec![("那天 森林中".into(), 0.98), ("你好世界".into(), 0.98)]);
    }

    #[test]
    fn a_rewritten_echo_is_still_its_lines() {
        let out = parse(&["a|A", "b rewritten|B", "c|C"], &["a", "b", "c"]);
        assert_eq!(out, vec![("A".into(), 0.98), ("B".into(), 0.5), ("C".into(), 0.98)]);
    }

    #[test]
    fn a_skipped_line_does_not_shift_the_rest() {
        let out = parse(&["a|A", "c|C", "d|D"], &["a", "b", "c", "d"]);
        assert_eq!(out, vec![("A".into(), 0.98), ("b".into(), 0.0), ("C".into(), 0.98), ("D".into(), 0.98)]);
    }

    #[test]
    fn a_reply_that_is_not_a_translation_is_an_error_not_a_guess() {
        // A refusal with a summary: 3 lines of prose for 5 lyric lines.
        let refusal = "I can't reproduce these lyrics.
Here is a summary:
The song is about loneliness.";
        assert!(parse_translation_response(refusal, &lines(&["a", "b", "c", "d", "e"]), "en").is_err());
        // Paired, but most lines missing.
        let partial = format!("a{s}A
b{s}B", s = TRANSLATION_SEPARATOR);
        assert!(parse_translation_response(&partial, &lines(&["a", "b", "c", "d", "e"]), "en").is_err());
        // One line per original, without echoes: fine.
        let (out, _) = parse_translation_response("A
B", &lines(&["a", "b"]), "en").unwrap();
        assert_eq!(out.iter().map(|l| l.translated_text.as_str()).collect::<Vec<_>>(), ["A", "B"]);
    }

    #[test]
    fn repeated_lines_take_their_own_translations_in_order() {
        let out = parse(&["la|啦1", "x|X", "la|啦2"], &["la", "x", "la"]);
        assert_eq!(out, vec![("啦1".into(), 0.98), ("X".into(), 0.98), ("啦2".into(), 0.98)]);
    }
}
