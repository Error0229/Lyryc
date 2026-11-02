//! Translation strategy execution

use log::{debug, info};

use super::language::detect_language_fallback;
use super::parser::parse_translation_response;
use super::prompt::{build_translation_prompt, SYSTEM_PROMPT};
use super::types::{
    LyricLine, TranslatedLyricLine, TranslationConfig, TranslationResult, TranslationStrategy,
};
use crate::llm::{self, ChatRequest, Target};

/// Translate one batch of lines with a single LLM call
async fn translate_once(
    lyrics: &[LyricLine],
    config: &TranslationConfig,
    target: &Target,
) -> Result<TranslationResult, String> {
    let detected_language = if config.source_language == "auto" {
        detect_language_fallback(lyrics)
    } else {
        config.source_language.clone()
    };

    // Skip if same language
    if detected_language == config.target_language {
        debug!(
            "Detected language {} matches target {}, skipping",
            detected_language, config.target_language
        );
        return Ok(TranslationResult {
            success: true,
            translated_lines: vec![],
            detected_language: Some(detected_language),
            processing_time_ms: 0,
            error: None,
            is_same_language: true,
        });
    }

    let lyrics_input = lyrics
        .iter()
        .map(|line| line.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    let translation_prompt = build_translation_prompt(
        &lyrics_input,
        &detected_language,
        &config.target_language,
        &config.style,
        config.custom_prompt.as_deref(),
    );

    let response_text = llm::complete(
        target,
        &ChatRequest {
            system: Some(SYSTEM_PROMPT),
            user: &translation_prompt,
            temperature: Some(0.3),
            max_tokens: 8192,
            json: false,
        },
    )
    .await?;

    let (translated_lines, _is_same_language) =
        parse_translation_response(&response_text, lyrics, &detected_language, Some(target))
            .await?;

    Ok(TranslationResult {
        success: true,
        translated_lines,
        detected_language: Some(detected_language),
        processing_time_ms: 0,
        error: None,
        is_same_language: false,
    })
}

/// Execute translation with progressive retry strategies
pub async fn translate_with_strategies(
    lyrics: &[LyricLine],
    config: &TranslationConfig,
    target: &Target,
    strategies: &[TranslationStrategy],
) -> Result<TranslationResult, String> {
    let mut last_error = String::from("no strategy produced a result");
    for (i, strategy) in strategies.iter().enumerate() {
        debug!("Trying translation strategy {}: {:?}", i + 1, strategy);

        let result = match strategy {
            TranslationStrategy::Combined => translate_once(lyrics, config, target).await,
            TranslationStrategy::Chunked(chunk_size) => {
                translate_chunked(lyrics, config, target, *chunk_size).await
            }
            TranslationStrategy::LineByLine => translate_line_by_line(lyrics, config, target).await,
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
                last_error = e;
            }
        }
    }

    Err(format!("All translation strategies failed: {last_error}"))
}

async fn translate_chunked(
    lyrics: &[LyricLine],
    config: &TranslationConfig,
    target: &Target,
    chunk_size: usize,
) -> Result<TranslationResult, String> {
    let start_time = std::time::Instant::now();
    let mut all_translated_lines = Vec::new();
    let mut detected_language = None;

    for chunk in lyrics.chunks(chunk_size) {
        let chunk_result = translate_once(chunk, config, target).await?;

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
    target: &Target,
) -> Result<TranslationResult, String> {
    let start_time = std::time::Instant::now();
    let mut all_translated_lines = Vec::new();
    let mut detected_language = None;

    for line in lyrics {
        let single_line = vec![line.clone()];
        let line_result = translate_once(&single_line, config, target).await?;

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
