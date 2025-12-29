//! Translation strategy execution

use log::{debug, info};

use super::providers::translate_with_provider;
use super::types::{
    LyricLine, TranslatedLyricLine, TranslationConfig, TranslationResult, TranslationStrategy,
};

/// Execute translation with progressive retry strategies
pub async fn translate_with_strategies(
    lyrics: &[LyricLine],
    config: &TranslationConfig,
    strategies: &[TranslationStrategy],
) -> Result<TranslationResult, String> {
    for (i, strategy) in strategies.iter().enumerate() {
        debug!("Trying translation strategy {}: {:?}", i + 1, strategy);

        let result = match strategy {
            TranslationStrategy::Combined => translate_with_provider(lyrics, config).await,
            TranslationStrategy::Chunked(chunk_size) => {
                translate_chunked(lyrics, config, *chunk_size).await
            }
            TranslationStrategy::LineByLine => translate_line_by_line(lyrics, config).await,
            TranslationStrategy::AlignmentFix => {
                Err("AlignmentFix strategy must be used with another strategy".to_string())
            }
        };

        match result {
            Ok(translation_result) => {
                if translation_result.success {
                    info!("Translation successful with strategy: {:?}", strategy);
                    return Ok(translation_result);
                }
                debug!("Strategy {:?} succeeded but with errors", strategy);
            }
            Err(e) => {
                debug!("Strategy {:?} failed: {}", strategy, e);
            }
        }
    }

    Err("All translation strategies failed".to_string())
}

async fn translate_chunked(
    lyrics: &[LyricLine],
    config: &TranslationConfig,
    chunk_size: usize,
) -> Result<TranslationResult, String> {
    let start_time = std::time::Instant::now();
    let mut all_translated_lines = Vec::new();
    let mut detected_language = None;

    for chunk in lyrics.chunks(chunk_size) {
        let chunk_result = translate_with_provider(chunk, config).await?;

        if !chunk_result.success {
            return Err("Chunk translation failed".to_string());
        }

        all_translated_lines.extend(chunk_result.translated_lines);
        if detected_language.is_none() {
            detected_language = chunk_result.detected_language;
        }
    }

    Ok(TranslationResult {
        success: true,
        translated_lines: all_translated_lines,
        detected_language,
        processing_time_ms: start_time.elapsed().as_millis() as u64,
        error: None,
        is_same_language: false,
    })
}

async fn translate_line_by_line(
    lyrics: &[LyricLine],
    config: &TranslationConfig,
) -> Result<TranslationResult, String> {
    let start_time = std::time::Instant::now();
    let mut all_translated_lines = Vec::new();
    let mut detected_language = None;

    for line in lyrics {
        let single_line = vec![line.clone()];
        let line_result = translate_with_provider(&single_line, config).await?;

        if !line_result.success || line_result.translated_lines.is_empty() {
            all_translated_lines.push(TranslatedLyricLine {
                time: line.time,
                text: line.text.clone(),
                duration: line.duration,
                original_text: line.text.clone(),
                translated_text: line.text.clone(),
                translation_confidence: Some(0.0),
                detected_language: detected_language.clone(),
            });
        } else {
            all_translated_lines.extend(line_result.translated_lines);
            if detected_language.is_none() {
                detected_language = line_result.detected_language;
            }
        }
    }

    Ok(TranslationResult {
        success: true,
        translated_lines: all_translated_lines,
        detected_language,
        processing_time_ms: start_time.elapsed().as_millis() as u64,
        error: None,
        is_same_language: false,
    })
}
