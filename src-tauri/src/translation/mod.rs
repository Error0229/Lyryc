//! Lyrics translation: the whole song in one request, numbered lines in,
//! numbered translations out (see `prompt`). A provider whose reply doesn't
//! fit (a refusal, a summary) or that fails outright hands the song to the
//! next configured one; nothing is split up or guessed.

pub mod cache;
mod language;
mod parser;
mod prompt;
mod types;

use log::{error, info, warn};

use crate::llm::{self, ChatRequest, Target};
use language::detect_language_fallback;
use parser::parse_translation_response;

pub use types::{LyricLine, TranslationConfig, TranslationResult};

/// Translate with the first of `targets` that gives a usable answer: the
/// routed provider, then the fallbacks.
pub async fn translate_lyrics(
    lyrics: Vec<LyricLine>,
    config: TranslationConfig,
    targets: &[Target],
) -> Result<TranslationResult, String> {
    let start_time = std::time::Instant::now();
    if lyrics.is_empty() {
        return Ok(failed("No lyrics provided".to_string(), 0));
    }

    let mut errors = Vec::new();
    for target in targets {
        info!(
            "Translating {} lines from {} to {} via {} ({})",
            lyrics.len(),
            config.source_language,
            config.target_language,
            target.label,
            target.model
        );
        match translate_once(&lyrics, &config, target).await {
            Ok(mut result) => {
                result.processing_time_ms = start_time.elapsed().as_millis() as u64;
                info!("Translation done in {}ms by {} ({})", result.processing_time_ms, target.label, target.model);
                return Ok(result);
            }
            Err(e) => {
                warn!("{} ({}) didn't translate this song: {e}", target.label, target.model);
                errors.push(format!("{}: {e}", target.label));
            }
        }
    }

    let ms = start_time.elapsed().as_millis() as u64;
    let e = if errors.is_empty() { "no provider to translate with".to_string() } else { errors.join("; ") };
    error!("Translation failed after {ms}ms: {e}");
    Ok(failed(e, ms))
}

/// One request for the whole song.
async fn translate_once(lyrics: &[LyricLine], config: &TranslationConfig, target: &Target) -> Result<TranslationResult, String> {
    let detected_language = if config.source_language == "auto" {
        detect_language_fallback(lyrics)
    } else {
        config.source_language.clone()
    };
    if detected_language == config.target_language {
        return Ok(TranslationResult {
            success: true,
            translated_lines: vec![],
            detected_language: Some(detected_language),
            processing_time_ms: 0,
            error: None,
            is_same_language: true,
        });
    }

    let lines: Vec<&str> = lyrics.iter().map(|l| l.text.as_str()).collect();
    let system = prompt::system_prompt(&config.target_language, &config.style, config.custom_prompt.as_deref());
    let user = prompt::user_prompt(&lines, &detected_language, &config.target_language);
    let reply = llm::complete(
        target,
        &ChatRequest { system: Some(&system), user: &user, temperature: Some(0.3), max_tokens: 8192, json: false },
    )
    .await
    .map_err(|e| e.to_string())?;

    let translated_lines = parse_translation_response(&reply, lyrics, &detected_language)?;
    Ok(TranslationResult {
        success: true,
        translated_lines,
        detected_language: Some(detected_language),
        processing_time_ms: 0,
        error: None,
        is_same_language: false,
    })
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
