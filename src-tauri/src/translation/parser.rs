//! Translation response parsing

use log::{info, warn};
use std::collections::{HashMap, HashSet};

use super::alignment::resolve_line_count_mismatch;
use super::prompt::TRANSLATION_SEPARATOR;
use super::types::{LyricLine, TranslatedLyricLine};
use crate::llm::Target;

/// Parse translation response and align with original lyrics
pub async fn parse_translation_response(
    response_text: &str,
    original_lyrics: &[LyricLine],
    detected_language: &str,
    target: Option<&Target>,
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
        let result = parse_paired_format(&cleaned_response, original_lyrics, detected_language);

        if let Ok((lines, _is_same)) = &result {
            if lines.len() == original_lyrics.len() {
                return result;
            }
        }
    }

    // Fallback to legacy format
    info!("[Parse Path: LEGACY_FORMAT] Falling back to legacy translation-only format");
    parse_legacy_format(
        &cleaned_response,
        original_lyrics,
        detected_language,
        target,
    )
    .await
}

fn parse_paired_format(
    cleaned_response: &[&str],
    original_lyrics: &[LyricLine],
    detected_language: &str,
) -> Result<(Vec<TranslatedLyricLine>, bool), String> {
    info!(
        "Parsing paired format with separator: '{}'",
        TRANSLATION_SEPARATOR
    );

    // Build a map of original text -> (response_index, translated_text)
    let mut response_map: HashMap<String, Vec<(usize, String)>> = HashMap::new();

    for (resp_idx, response_line) in cleaned_response.iter().enumerate() {
        if let Some(separator_pos) = response_line.find(TRANSLATION_SEPARATOR) {
            let original_part = response_line[..separator_pos].trim().to_string();
            let translated_part = response_line[separator_pos + TRANSLATION_SEPARATOR.len()..]
                .trim()
                .to_string();

            response_map
                .entry(original_part)
                .or_default()
                .push((resp_idx, translated_part));
        }
    }

    let mut translated_lines = Vec::new();
    let mut mismatch_count = 0;
    let mut used_response_indices = HashSet::new();

    for (orig_idx, original_line) in original_lyrics.iter().enumerate() {
        let original_text_trimmed = original_line.text.trim();

        // Try exact match first
        if let Some(matches) = response_map.get(original_text_trimmed) {
            if let Some((resp_idx, translated_text)) = matches
                .iter()
                .find(|(idx, _)| !used_response_indices.contains(idx))
            {
                used_response_indices.insert(*resp_idx);

                translated_lines.push(TranslatedLyricLine {
                    time: original_line.time,
                    text: translated_text.clone(),
                    duration: original_line.duration,
                    original_text: original_line.text.clone(),
                    translated_text: translated_text.clone(),
                    translation_confidence: Some(0.98),
                    detected_language: Some(detected_language.to_string()),
                });
                continue;
            }
        }

        // No exact match found - this is a real mismatch
        mismatch_count += 1;

        // Try to find the best match from unused response lines
        let best_match = cleaned_response
            .iter()
            .enumerate()
            .filter(|(idx, _)| !used_response_indices.contains(idx))
            .filter_map(|(idx, line)| {
                line.find(TRANSLATION_SEPARATOR).map(|sep_pos| {
                    let orig_part = line[..sep_pos].trim();
                    let trans_part = line[sep_pos + TRANSLATION_SEPARATOR.len()..].trim();
                    (idx, orig_part, trans_part)
                })
            })
            .next();

        if let Some((resp_idx, _orig_part, trans_part)) = best_match {
            warn!(
                "[Parse Path: NEW_FORMAT_MISMATCH] Line {}: Original '{}' not found, using fallback",
                orig_idx, original_text_trimmed
            );
            used_response_indices.insert(resp_idx);

            translated_lines.push(TranslatedLyricLine {
                time: original_line.time,
                text: trans_part.to_string(),
                duration: original_line.duration,
                original_text: original_line.text.clone(),
                translated_text: trans_part.to_string(),
                translation_confidence: Some(0.5),
                detected_language: Some(detected_language.to_string()),
            });
        } else {
            warn!(
                "[Parse Path: NEW_FORMAT_MISSING] Line {}: No translation found, using original",
                orig_idx
            );
            translated_lines.push(TranslatedLyricLine {
                time: original_line.time,
                text: original_line.text.clone(),
                duration: original_line.duration,
                original_text: original_line.text.clone(),
                translated_text: original_line.text.clone(),
                translation_confidence: Some(0.0),
                detected_language: Some(detected_language.to_string()),
            });
        }
    }

    if translated_lines.len() == original_lyrics.len() && mismatch_count == 0 {
        info!(
            "[Parse Path: NEW_FORMAT_SUCCESS] Paired format parsing successful: {} lines",
            translated_lines.len()
        );
    } else {
        warn!(
            "[Parse Path: NEW_FORMAT_PARTIAL] Completed with {} mismatches out of {} lines",
            mismatch_count,
            original_lyrics.len()
        );
    }

    Ok((translated_lines, false))
}

async fn parse_legacy_format(
    cleaned_response: &[&str],
    original_lyrics: &[LyricLine],
    detected_language: &str,
    target: Option<&Target>,
) -> Result<(Vec<TranslatedLyricLine>, bool), String> {
    // Handle line count mismatch
    if cleaned_response.len() != original_lyrics.len() {
        warn!(
            "[Parse Path: LEGACY_MISMATCH] Line count mismatch: original {} vs translated {}",
            original_lyrics.len(),
            cleaned_response.len()
        );

        return resolve_line_count_mismatch(
            cleaned_response,
            original_lyrics,
            detected_language,
            target,
        )
        .await;
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
