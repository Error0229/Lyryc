//! Translation module for lyrics translation
//!
//! This module provides comprehensive lyrics translation functionality with:
//! - GitHub AI as primary provider (tier-optimized model selection)
//! - Google Gemini as deprecated fallback (limited free tier)
//! - Intelligent language detection
//! - Line alignment and mismatch resolution
//! - Progressive retry strategies

mod alignment;
mod blacklist;
mod language;
mod parser;
mod prompt;
mod providers;
mod strategy;
mod types;

use log::{error, info};

use types::TranslationStrategy;

// Re-export public types and functions
pub use blacklist::{blacklist_model, is_model_blacklisted};
pub use types::{LyricLine, TranslationConfig, TranslationResult};

/// Main translation function with enhanced mismatch resolution and fallback support
pub async fn translate_lyrics(
    lyrics: Vec<LyricLine>,
    config: TranslationConfig,
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
        "Starting enhanced translation for {} lines from {} to {}",
        lyrics.len(),
        config.source_language,
        config.target_language
    );

    // Define progressive translation strategies
    let strategies = vec![
        TranslationStrategy::Combined,
        TranslationStrategy::Chunked(5),
        TranslationStrategy::LineByLine,
    ];

    let result = strategy::translate_with_strategies(&lyrics, &config, &strategies).await;

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
    fn test_separator_constant() {
        assert_eq!(prompt::TRANSLATION_SEPARATOR, "<sep>");
        assert!(prompt::TRANSLATION_SEPARATOR.len() > 5);
    }
}
