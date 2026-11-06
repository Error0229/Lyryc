//! Translation module for lyrics translation
//!
//! This module provides comprehensive lyrics translation functionality with:
//! - Any BYOK provider via `crate::llm` (the caller resolves the route)
//! - Intelligent language detection
//! - One translation per lyric line, or a retry in smaller pieces
//! - Progressive retry strategies

pub mod cache;
mod language;
mod parser;
mod prompt;
mod strategy;
mod types;

use log::{error, info};

use crate::llm::Target;
use types::TranslationStrategy;

// Re-export public types and functions
pub use types::{LyricLine, TranslationConfig, TranslationResult};

/// Main translation function with enhanced mismatch resolution and fallback support
pub async fn translate_lyrics(
    lyrics: Vec<LyricLine>,
    config: TranslationConfig,
    target: &Target,
) -> Result<TranslationResult, String> {
    let start_time = std::time::Instant::now();

    if lyrics.is_empty() {
        return Ok(TranslationResult {
            success: false,
            translated_lines: vec![],
            detected_language: None,
            processing_time_ms: 0,
            error: Some("No lyrics provided".to_string()),
            is_same_language: false,
        });
    }

    info!(
        "Starting enhanced translation for {} lines from {} to {} via {} ({})",
        lyrics.len(),
        config.source_language,
        config.target_language,
        target.label,
        target.model
    );

    // Define progressive translation strategies
    let strategies = vec![
        TranslationStrategy::Combined,
        TranslationStrategy::Chunked(5),
        TranslationStrategy::LineByLine,
    ];

    let result = strategy::translate_with_strategies(&lyrics, &config, target, &strategies).await;

    match result {
        Ok(mut translation_result) => {
            translation_result.processing_time_ms = start_time.elapsed().as_millis() as u64;
            info!(
                "Translation completed successfully in {}ms",
                translation_result.processing_time_ms
            );
            Ok(translation_result)
        }
        Err(e) => {
            let processing_time_ms = start_time.elapsed().as_millis() as u64;
            error!("Translation failed after {}ms: {}", processing_time_ms, e);
            Ok(TranslationResult {
                success: false,
                translated_lines: vec![],
                detected_language: None,
                processing_time_ms,
                error: Some(e),
                is_same_language: false,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::translation::language::detect_language_fallback;

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

    #[test]
    fn test_separator_constant() {
        assert_eq!(prompt::TRANSLATION_SEPARATOR, "<sep>");
        assert!(prompt::TRANSLATION_SEPARATOR.len() >= 5);
    }
}
