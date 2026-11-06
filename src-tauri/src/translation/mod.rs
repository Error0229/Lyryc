//! Lyrics translation: the whole song in one request, numbered lines in,
//! numbered translations out (see `prompt`). A provider whose reply doesn't
//! fit (a refusal, a summary, skipped lines) or that fails outright hands the
//! song to the backup route; nothing is split up or guessed.

pub mod cache;
mod language;
mod parser;
mod prompt;
mod types;

use std::future::Future;

use log::{error, info, warn};

use crate::llm::{self, ChatRequest, Target};
use language::detect_language_fallback;
use parser::parse_translation_response;
use types::TranslatedLyricLine;

pub use types::{LyricLine, TranslationConfig, TranslationResult};

/// Translate with the first of `targets` that answers every line: the routed
/// provider, then the backup.
pub async fn translate_lyrics(lyrics: Vec<LyricLine>, config: TranslationConfig, targets: &[Target]) -> Result<TranslationResult, String> {
    // Room for every line of a long song: about 100 tokens a line, at least
    // the 8192 every provider accepts, at most 16384.
    let max_tokens = (lyrics.len() as u32 * 100).clamp(8192, 16384);
    translate_with(lyrics, config, targets, move |target, system, user| async move {
        let request = ChatRequest { system: Some(&system), user: &user, temperature: Some(0.3), max_tokens, json: false };
        llm::complete(&target, &request).await.map_err(|e| e.to_string())
    })
    .await
}

/// `translate_lyrics` with the request itself passed in (`ask(target,
/// system, user)` returns the reply), so the fallback order can be tested
/// without a network.
async fn translate_with<F, Fut>(lyrics: Vec<LyricLine>, config: TranslationConfig, targets: &[Target], ask: F) -> Result<TranslationResult, String>
where
    F: Fn(Target, String, String) -> Fut,
    Fut: Future<Output = Result<String, String>>,
{
    let start = std::time::Instant::now();
    if lyrics.is_empty() {
        return Ok(failed("No lyrics provided".to_string(), 0));
    }
    let language = if config.source_language == "auto" { detect_language_fallback(&lyrics) } else { config.source_language.clone() };
    if language == config.target_language {
        return Ok(TranslationResult { is_same_language: true, ..done(vec![], &language, start) });
    }

    // Only lines with words go out. A "♪" or blank line is its own
    // translation: models drop them, and a dropped line would leave the song
    // looking unfinished, so it would never be cached.
    let wordy: Vec<&LyricLine> = lyrics.iter().filter(|l| has_words(&l.text)).collect();
    let texts: Vec<&str> = wordy.iter().map(|l| l.text.as_str()).collect();
    let asked: Vec<LyricLine> = wordy.into_iter().cloned().collect();
    if asked.is_empty() {
        return Ok(done(merge(&lyrics, vec![], &language), &language, start));
    }
    let system = prompt::system_prompt(&config.target_language, &config.style, config.custom_prompt.as_deref());
    let user = prompt::user_prompt(&texts, &language, &config.target_language);

    let mut errors = Vec::new();
    let mut best: Option<(usize, Vec<TranslatedLyricLine>)> = None; // fewest lines skipped
    let mut asked_already: Vec<(&str, &str)> = Vec::new();
    for target in targets {
        // A backup that resolves to the same model would only repeat the answer.
        if asked_already.contains(&(target.base_url.as_str(), target.model.as_str())) {
            continue;
        }
        asked_already.push((target.base_url.as_str(), target.model.as_str()));
        info!("Translating {} lines from {} to {} via {} ({})", asked.len(), language, config.target_language, target.label, target.model);

        let reply = ask(target.clone(), system.clone(), user.clone()).await;
        let parsed = reply.and_then(|r| parse_translation_response(&r, &asked, &language));
        let lines = match parsed {
            Ok(lines) => lines,
            Err(e) => {
                warn!("{} ({}) didn't translate this song: {e}", target.label, target.model);
                errors.push(format!("{}: {e}", target.label));
                continue;
            }
        };
        let skipped = lines.iter().filter(|l| l.translation_confidence == Some(0.0)).count();
        if skipped == 0 {
            let result = done(merge(&lyrics, lines, &language), &language, start);
            info!("Translation done in {}ms by {} ({})", result.processing_time_ms, target.label, target.model);
            return Ok(result);
        }
        // Skipped lines (or a reply cut off at the token limit): the next
        // target may do better; this one is kept in case none does.
        warn!("{} ({}) skipped {skipped} of {} lines", target.label, target.model, lines.len());
        errors.push(format!("{}: skipped {skipped} lines", target.label));
        if best.as_ref().is_none_or(|(fewest, _)| skipped < *fewest) {
            best = Some((skipped, lines));
        }
    }

    if let Some((skipped, lines)) = best {
        warn!("Keeping a translation with {skipped} line(s) left untranslated; it isn't cached, so the next play asks again");
        return Ok(done(merge(&lyrics, lines, &language), &language, start));
    }
    let ms = start.elapsed().as_millis() as u64;
    let e = if errors.is_empty() { "no provider to translate with".to_string() } else { errors.join("; ") };
    error!("Translation failed after {ms}ms: {e}");
    Ok(failed(e, ms))
}

/// Something to translate: a line with at least one letter.
fn has_words(text: &str) -> bool {
    text.chars().any(char::is_alphabetic)
}

/// Every original line, in order: the translated ones from `translated` (one
/// per line with words, in order), the rest as they are.
fn merge(lyrics: &[LyricLine], translated: Vec<TranslatedLyricLine>, language: &str) -> Vec<TranslatedLyricLine> {
    let mut translated = translated.into_iter();
    lyrics
        .iter()
        .map(|line| match has_words(&line.text) {
            true => translated.next().unwrap_or_else(|| as_is(line, language)),
            false => as_is(line, language),
        })
        .collect()
}

fn as_is(line: &LyricLine, language: &str) -> TranslatedLyricLine {
    TranslatedLyricLine {
        time: line.time,
        text: line.text.clone(),
        duration: line.duration,
        original_text: line.text.clone(),
        translated_text: line.text.clone(),
        translation_confidence: Some(1.0),
        detected_language: Some(language.to_string()),
    }
}

fn done(translated_lines: Vec<TranslatedLyricLine>, language: &str, start: std::time::Instant) -> TranslationResult {
    TranslationResult {
        success: true,
        translated_lines,
        detected_language: Some(language.to_string()),
        processing_time_ms: start.elapsed().as_millis() as u64,
        error: None,
        is_same_language: false,
    }
}

fn failed(error: String, processing_time_ms: u64) -> TranslationResult {
    TranslationResult {
        success: false,
        translated_lines: vec![],
        detected_language: None,
        processing_time_ms,
        error: Some(error),
        is_same_language: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn target(label: &str, model: &str) -> Target {
        Target {
            label: label.into(),
            preset: crate::llm::presets::find("gemini").unwrap(),
            base_url: format!("https://{label}.example"),
            api_key: None,
            model: model.into(),
        }
    }

    fn song(texts: &[&str]) -> Vec<LyricLine> {
        texts.iter().enumerate().map(|(i, t)| LyricLine { time: i as f64, text: t.to_string(), duration: None }).collect()
    }

    fn config() -> TranslationConfig {
        TranslationConfig { source_language: "en".into(), target_language: "fr".into(), ..TranslationConfig::default() }
    }

    /// Runs `translate_with` where each target's reply is `replies[label]`
    /// (`Err` for a failed request); returns the result and who was asked, with what.
    async fn run(lyrics: &[&str], targets: &[Target], replies: &[(&str, Result<&str, &str>)]) -> (TranslationResult, Vec<(String, String)>) {
        let asked = Mutex::new(Vec::new());
        let result = translate_with(song(lyrics), config(), targets, |t, _system, user| {
            asked.lock().unwrap().push((t.label.clone(), user));
            let reply = replies.iter().find(|(l, _)| *l == t.label).map(|(_, r)| r.map(str::to_string).map_err(str::to_string));
            async move { reply.unwrap_or_else(|| Err("no reply".into())) }
        })
        .await
        .unwrap();
        (result, asked.into_inner().unwrap())
    }

    fn texts(r: &TranslationResult) -> Vec<&str> {
        r.translated_lines.iter().map(|l| l.translated_text.as_str()).collect()
    }

    #[tokio::test]
    async fn a_refusal_goes_to_the_backup() {
        let targets = [target("main", "m"), target("backup", "b")];
        let (r, asked) = run(&["hello", "world"], &targets, &[("main", Ok("I can't translate these lyrics.")), ("backup", Ok("1. bonjour
2. monde"))]).await;
        assert!(r.success);
        assert_eq!(texts(&r), ["bonjour", "monde"]);
        assert_eq!(asked.iter().map(|(l, _)| l.as_str()).collect::<Vec<_>>(), ["main", "backup"]);
    }

    #[tokio::test]
    async fn skipped_lines_go_to_the_backup_and_the_best_partial_is_the_last_resort() {
        let targets = [target("main", "m"), target("backup", "b")];
        let lines = ["one", "two", "three"];
        // The backup answers every line: it wins.
        let (r, _) = run(&lines, &targets, &[("main", Ok("1. un
2. deux")), ("backup", Ok("1. un
2. deux
3. trois"))]).await;
        assert_eq!(texts(&r), ["un", "deux", "trois"]);
        // The backup fails: the partial answer is kept, its gap left as the original.
        let (r, _) = run(&lines, &targets, &[("main", Ok("1. un
2. deux")), ("backup", Err("HTTP 503"))]).await;
        assert!(r.success);
        assert_eq!(texts(&r), ["un", "deux", "three"]);
        assert_eq!(r.translated_lines[2].translation_confidence, Some(0.0), "so it isn't cached");
    }

    #[tokio::test]
    async fn when_everyone_fails_the_errors_are_joined() {
        let targets = [target("main", "m"), target("backup", "b")];
        let (r, _) = run(&["hello"], &targets, &[("main", Err("HTTP 401")), ("backup", Ok("Sorry."))]).await;
        assert!(!r.success);
        let e = r.error.unwrap();
        assert!(e.contains("main: HTTP 401") && e.contains("backup: the reply translates only 0 of 1 lines"), "{e}");
    }

    #[tokio::test]
    async fn lines_without_words_are_not_sent_and_stay_as_they_are() {
        let (r, asked) = run(&["♪", "hello", "", "world", "♪ ♪"], &[target("main", "m")], &[("main", Ok("1. bonjour
2. monde"))]).await;
        assert!(!asked[0].1.contains('♪'), "{}", asked[0].1);
        assert_eq!(texts(&r), ["♪", "bonjour", "", "monde", "♪ ♪"]);
        assert!(r.translated_lines.iter().all(|l| l.translation_confidence != Some(0.0)), "a finished song is cached");
    }

    #[tokio::test]
    async fn the_same_model_twice_is_asked_once() {
        let (_, asked) = run(&["hello"], &[target("main", "m"), target("main", "m")], &[("main", Ok("Sorry."))]).await;
        assert_eq!(asked.len(), 1);
    }

    #[test]
    fn test_language_detection() {
        let japanese_lyrics = vec![LyricLine {
            time: 0.0,
            text: "これは日本語の歌詞です".to_string(),
            duration: Some(3.0),
        }];

        assert_eq!(detect_language_fallback(&japanese_lyrics), "ja");

        let english_lyrics = vec![LyricLine {
            time: 0.0,
            text: "This is an English song".to_string(),
            duration: Some(3.0),
        }];

        assert_eq!(detect_language_fallback(&english_lyrics), "en");
    }

    #[test]
    fn test_translation_config_default() {
        let config = TranslationConfig::default();
        assert_eq!(config.source_language, "auto");
        assert_eq!(config.target_language, "en");
        assert_eq!(config.style, "natural");
        assert!(config.preserve_meaning);
    }

    #[test]
    fn legacy_config_still_deserializes() {
        let legacy = r#"{
            "source_language": "auto", "target_language": "zh-tw", "style": "natural",
            "custom_prompt": null, "preserve_rhyme": true, "preserve_meaning": true,
            "maintain_structure": true, "provider": "gemini",
            "github_api_key": "ghp_x", "gemini_api_key": "AIza_y"
        }"#;
        let config: TranslationConfig = serde_json::from_str(legacy).unwrap();
        assert_eq!(config.provider, "gemini");
        let keys = config.legacy_keys();
        assert_eq!(keys[0], ("gemini", "AIza_y".to_string()));
        assert_eq!(keys[1], ("github", "ghp_x".to_string()));

        // A migrated frontend may drop the legacy fields entirely.
        let modern = r#"{
            "source_language": "ja", "target_language": "en", "style": "literal",
            "preserve_rhyme": false, "preserve_meaning": true, "maintain_structure": true
        }"#;
        let config: TranslationConfig = serde_json::from_str(modern).unwrap();
        assert!(config.legacy_keys().is_empty());
        assert!(config.custom_prompt.is_none());
    }
}
