//! Line count mismatch resolution for translation alignment

use log::{debug, info, warn};

use super::providers::{try_gemini_alignment, try_github_alignment};
use super::types::{
    LyricLine, MismatchResolution, MismatchType, TranslatedLyricLine, TranslationConfig,
};

/// Classify the type of line count mismatch
pub fn classify_mismatch(translated_count: usize, original_count: usize) -> MismatchType {
    match translated_count.cmp(&original_count) {
        std::cmp::Ordering::Less => MismatchType::LinesShort,
        std::cmp::Ordering::Greater => MismatchType::LinesExtra,
        std::cmp::Ordering::Equal => MismatchType::LinesSame,
    }
}

/// Comprehensive line count mismatch resolution system
pub async fn resolve_line_count_mismatch(
    translated_lines: &[&str],
    original_lyrics: &[LyricLine],
    detected_language: &str,
    config: Option<&TranslationConfig>,
) -> Result<(Vec<TranslatedLyricLine>, bool), String> {
    let mismatch_type = classify_mismatch(translated_lines.len(), original_lyrics.len());

    info!(
        "Resolving {:?} mismatch: {} translated vs {} original lines",
        mismatch_type,
        translated_lines.len(),
        original_lyrics.len()
    );

    // Strategy 1: LLM-based alignment resolution
    if let Some(config) = config {
        if let Ok((resolved_lines, is_same)) =
            resolve_alignment_with_llm(translated_lines, original_lyrics, detected_language, config)
                .await
        {
            info!("LLM-based alignment resolution successful");
            return Ok((resolved_lines, is_same));
        }
        debug!("LLM-based alignment failed, trying other strategies");
    }

    // Strategy 2: Smart alignment mapping
    if let Ok(alignment) = create_smart_alignment(translated_lines, original_lyrics) {
        if alignment.confidence > 0.7 {
            info!(
                "Smart alignment successful with confidence {:.2}",
                alignment.confidence
            );
            return apply_alignment_mapping(
                translated_lines,
                original_lyrics,
                detected_language,
                &alignment,
            );
        }
    }

    // Strategy 3: Intelligent merging/splitting
    match mismatch_type {
        MismatchType::LinesShort => {
            if let Ok(resolved) = resolve_by_intelligent_splitting(
                translated_lines,
                original_lyrics,
                detected_language,
            ) {
                return Ok(resolved);
            }
        }
        MismatchType::LinesExtra => {
            if let Ok(resolved) =
                resolve_by_intelligent_merging(translated_lines, original_lyrics, detected_language)
            {
                return Ok(resolved);
            }
        }
        MismatchType::LinesSame => unreachable!(),
    }

    // Strategy 4: Conservative fallback
    warn!("Using conservative fallback due to alignment failure");
    Ok(create_conservative_fallback(
        translated_lines,
        original_lyrics,
        detected_language,
    ))
}

/// Create smart alignment mapping between original and translated lines
fn create_smart_alignment(
    translated_lines: &[&str],
    original_lyrics: &[LyricLine],
) -> Result<MismatchResolution, String> {
    let original_count = original_lyrics.len();
    let translated_count = translated_lines.len();

    let ratio = if original_count > translated_count {
        translated_count as f64 / original_count as f64
    } else {
        original_count as f64 / translated_count as f64
    };

    let base_confidence = if ratio > 0.8 {
        0.8
    } else if ratio > 0.5 {
        0.6
    } else {
        0.3
    };

    let mut alignment_map = Vec::new();

    for i in 0..original_count {
        let translated_index =
            (i as f64 * translated_count as f64 / original_count as f64) as usize;
        let mapped_index = translated_index.min(translated_count.saturating_sub(1));
        alignment_map.push((i, mapped_index));
    }

    Ok(MismatchResolution {
        strategy_used: "smart_alignment".to_string(),
        confidence: base_confidence,
        original_count,
        translated_count,
        alignment_map,
        success: true,
    })
}

fn apply_alignment_mapping(
    translated_lines: &[&str],
    original_lyrics: &[LyricLine],
    detected_language: &str,
    alignment: &MismatchResolution,
) -> Result<(Vec<TranslatedLyricLine>, bool), String> {
    let mut result_lines = Vec::new();

    for (original_idx, translated_idx) in &alignment.alignment_map {
        let original_line = &original_lyrics[*original_idx];
        let translated_text = translated_lines
            .get(*translated_idx)
            .unwrap_or(&original_line.text.as_str())
            .to_string();

        result_lines.push(TranslatedLyricLine {
            time: original_line.time,
            text: translated_text.clone(),
            duration: original_line.duration,
            original_text: original_line.text.clone(),
            translated_text,
            translation_confidence: Some(alignment.confidence),
            detected_language: Some(detected_language.to_string()),
        });
    }

    Ok((result_lines, false))
}

fn resolve_by_intelligent_splitting(
    translated_lines: &[&str],
    original_lyrics: &[LyricLine],
    detected_language: &str,
) -> Result<(Vec<TranslatedLyricLine>, bool), String> {
    debug!("Attempting intelligent splitting to resolve line shortage");

    let mut result_lines = Vec::new();

    for (i, original_line) in original_lyrics.iter().enumerate() {
        let translated_text = if i < translated_lines.len() {
            translated_lines[i].to_string()
        } else {
            translated_lines
                .last()
                .unwrap_or(&original_line.text.as_str())
                .to_string()
        };

        result_lines.push(TranslatedLyricLine {
            time: original_line.time,
            text: translated_text.clone(),
            duration: original_line.duration,
            original_text: original_line.text.clone(),
            translated_text,
            translation_confidence: Some(0.6),
            detected_language: Some(detected_language.to_string()),
        });
    }

    Ok((result_lines, false))
}

fn resolve_by_intelligent_merging(
    translated_lines: &[&str],
    original_lyrics: &[LyricLine],
    detected_language: &str,
) -> Result<(Vec<TranslatedLyricLine>, bool), String> {
    debug!("Attempting intelligent merging to resolve line excess");

    let mut result_lines = Vec::new();
    let excess_ratio = translated_lines.len() as f64 / original_lyrics.len() as f64;

    for (i, original_line) in original_lyrics.iter().enumerate() {
        let start_idx = (i as f64 * excess_ratio) as usize;
        let end_idx = ((i + 1) as f64 * excess_ratio) as usize;

        let merged_translation = translated_lines[start_idx..end_idx.min(translated_lines.len())]
            .join(" ")
            .trim()
            .to_string();

        let final_translation = if merged_translation.is_empty() {
            original_line.text.clone()
        } else {
            merged_translation
        };

        result_lines.push(TranslatedLyricLine {
            time: original_line.time,
            text: final_translation.clone(),
            duration: original_line.duration,
            original_text: original_line.text.clone(),
            translated_text: final_translation,
            translation_confidence: Some(0.6),
            detected_language: Some(detected_language.to_string()),
        });
    }

    Ok((result_lines, false))
}

fn create_conservative_fallback(
    translated_lines: &[&str],
    original_lyrics: &[LyricLine],
    detected_language: &str,
) -> (Vec<TranslatedLyricLine>, bool) {
    let mut result_lines = Vec::new();

    for (i, original_line) in original_lyrics.iter().enumerate() {
        let translated_text = translated_lines
            .get(i)
            .unwrap_or(&original_line.text.as_str())
            .to_string();

        result_lines.push(TranslatedLyricLine {
            time: original_line.time,
            text: translated_text.clone(),
            duration: original_line.duration,
            original_text: original_line.text.clone(),
            translated_text,
            translation_confidence: Some(0.4),
            detected_language: Some(detected_language.to_string()),
        });
    }

    (result_lines, false)
}

/// LLM-based alignment resolution
async fn resolve_alignment_with_llm(
    translated_lines: &[&str],
    original_lyrics: &[LyricLine],
    detected_language: &str,
    config: &TranslationConfig,
) -> Result<(Vec<TranslatedLyricLine>, bool), String> {
    info!(
        "Attempting LLM-based alignment resolution for {} -> {} line mismatch",
        original_lyrics.len(),
        translated_lines.len()
    );

    let original_text: Vec<String> = original_lyrics.iter().map(|l| l.text.clone()).collect();
    let alignment_prompt = super::prompt::build_alignment_prompt(&original_text, translated_lines);

    let alignment_result = match config.provider.as_str() {
        "gemini" => {
            if let Some(api_key) = &config.gemini_api_key {
                try_gemini_alignment(&alignment_prompt, api_key).await
            } else if let Ok(env_key) = std::env::var("GEMINI_API_KEY") {
                try_gemini_alignment(&alignment_prompt, &env_key).await
            } else {
                Err("No Gemini API key available".to_string())
            }
        }
        "github" | _ => {
            if let Some(api_key) = &config.github_api_key {
                try_github_alignment(&alignment_prompt, api_key).await
            } else if let Ok(env_key) = std::env::var("GITHUB_TOKEN") {
                try_github_alignment(&alignment_prompt, &env_key).await
            } else {
                Err("No GitHub token available".to_string())
            }
        }
    };

    match alignment_result {
        Ok(corrected_lines) => {
            info!(
                "LLM alignment successful: {} corrected lines",
                corrected_lines.len()
            );

            if corrected_lines.len() == original_lyrics.len() {
                let mut result_lines = Vec::new();

                for (i, original_line) in original_lyrics.iter().enumerate() {
                    let translated_text = corrected_lines
                        .get(i)
                        .unwrap_or(&original_line.text)
                        .clone();

                    result_lines.push(TranslatedLyricLine {
                        time: original_line.time,
                        text: translated_text.clone(),
                        duration: original_line.duration,
                        original_text: original_line.text.clone(),
                        translated_text,
                        translation_confidence: Some(0.8),
                        detected_language: Some(detected_language.to_string()),
                    });
                }

                Ok((result_lines, false))
            } else {
                warn!(
                    "LLM alignment failed: expected {} lines, got {}",
                    original_lyrics.len(),
                    corrected_lines.len()
                );
                Err("LLM alignment returned incorrect line count".to_string())
            }
        }
        Err(e) => {
            warn!("LLM alignment failed: {}", e);
            Err(e)
        }
    }
}
