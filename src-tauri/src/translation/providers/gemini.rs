//! Google Gemini provider for lyrics translation

use log::{debug, info};
use reqwest::Client;
use serde_json::{json, Value};
use std::time::Duration;

use crate::translation::language::detect_language_fallback;
use crate::translation::parser::parse_translation_response;
use crate::translation::prompt::{build_translation_prompt, SYSTEM_PROMPT};
use crate::translation::types::{LyricLine, TranslationConfig, TranslationResult};

/// Available Gemini models
const GEMINI_MODELS: &[&str] = &[
    "gemini-flash-lite-latest",
    "gemini-flash-latest",
    "gemini-2.5-flash",
    "gemini-2.5-flash-lite",
    "gemini-2.5-pro",
];

/// Translate lyrics using Google Gemini API
pub async fn translate_lyrics_gemini(
    lyrics: &[LyricLine],
    config: &TranslationConfig,
) -> Result<TranslationResult, String> {
    let gemini_api_key = config
        .gemini_api_key
        .clone()
        .filter(|key| !key.trim().is_empty())
        .or_else(|| std::env::var("GEMINI_API_KEY").ok())
        .ok_or_else(|| {
            "Gemini API key required for translation. Please enter your API key in the Translation settings.".to_string()
        })?;

    let source_language = config.source_language.clone();

    for (i, model_id) in GEMINI_MODELS.iter().enumerate() {
        match try_gemini_translation(lyrics, config, &source_language, model_id, &gemini_api_key)
            .await
        {
            Ok(result) => {
                info!(
                    "Translation successful with {}: {} lines translated",
                    model_id,
                    result.translated_lines.len()
                );
                return Ok(result);
            }
            Err(e) => {
                debug!(
                    "Translation failed with {}: {} (attempt {}/{})",
                    model_id,
                    e,
                    i + 1,
                    GEMINI_MODELS.len()
                );
                if i < GEMINI_MODELS.len() - 1 {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            }
        }
    }

    Err("All Gemini translation attempts failed".to_string())
}

async fn try_gemini_translation(
    lyrics: &[LyricLine],
    config: &TranslationConfig,
    source_language: &str,
    model_id: &str,
    gemini_api_key: &str,
) -> Result<TranslationResult, String> {
    let detected_language = if source_language == "auto" {
        detect_language_fallback(lyrics)
    } else {
        source_language.to_string()
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

    let client = Client::builder()
        .timeout(Duration::from_secs(45))
        .build()
        .map_err(|e| e.to_string())?;

    let lyrics_text: Vec<String> = lyrics.iter().map(|line| line.text.clone()).collect();
    let lyrics_input = lyrics_text.join("\n");

    let translation_prompt = build_translation_prompt(
        &lyrics_input,
        &detected_language,
        &config.target_language,
        &config.style,
        config.custom_prompt.as_deref(),
    );

    let payload = json!({
        "contents": [{
            "parts": [{
                "text": format!("{}\n\n{}", SYSTEM_PROMPT, translation_prompt)
            }]
        }],
        "generationConfig": {
            "temperature": 0.3,
            "maxOutputTokens": 8192,
            "topP": 0.95,
            "topK": 40
        },
        "safetySettings": [
            {"category": "HARM_CATEGORY_HARASSMENT", "threshold": "BLOCK_MEDIUM_AND_ABOVE"},
            {"category": "HARM_CATEGORY_HATE_SPEECH", "threshold": "BLOCK_MEDIUM_AND_ABOVE"},
            {"category": "HARM_CATEGORY_SEXUALLY_EXPLICIT", "threshold": "BLOCK_MEDIUM_AND_ABOVE"},
            {"category": "HARM_CATEGORY_DANGEROUS_CONTENT", "threshold": "BLOCK_MEDIUM_AND_ABOVE"}
        ]
    });

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        model_id, gemini_api_key
    );

    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(format!("HTTP {}: {}", status, error_text));
    }

    let result: Value = response.json().await.map_err(|e| e.to_string())?;

    // Check for finish reason issues
    if let Some(finish_reason) = result["candidates"][0]["finishReason"].as_str() {
        if finish_reason == "MAX_TOKENS" {
            return Err("Gemini response truncated due to token limit".to_string());
        }
        if finish_reason != "STOP" {
            return Err(format!("Gemini response incomplete: {}", finish_reason));
        }
    }

    // Parse Gemini response format
    let response_text = result["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .or_else(|| result["candidates"][0]["content"]["text"].as_str())
        .or_else(|| {
            result["candidates"][0]["content"]["parts"]
                .as_array()
                .and_then(|parts| parts.first())
                .and_then(|p| p["text"].as_str())
        })
        .ok_or_else(|| {
            format!(
                "No text content found in Gemini response: {:?}",
                result["candidates"][0]["content"]
            )
        })?;

    let (translated_lines, _is_same_language) =
        parse_translation_response(response_text, lyrics, &detected_language, Some(config)).await?;

    Ok(TranslationResult {
        success: true,
        translated_lines,
        detected_language: Some(detected_language),
        processing_time_ms: 0,
        error: None,
        is_same_language: false,
    })
}

/// Try Gemini alignment for line count mismatch
pub async fn try_gemini_alignment(prompt: &str, api_key: &str) -> Result<Vec<String>, String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;

    let payload = json!({
        "contents": [{
            "parts": [{"text": prompt}]
        }],
        "generationConfig": {
            "temperature": 0.1,
            "maxOutputTokens": 4096,
            "topP": 0.8,
            "topK": 10
        }
    });

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash-lite:generateContent?key={}",
        api_key
    );

    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        return Err(format!(
            "Gemini alignment request failed: {}",
            response.status()
        ));
    }

    let result: Value = response.json().await.map_err(|e| e.to_string())?;
    let response_text = result["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .ok_or("No text in Gemini alignment response")?;

    Ok(response_text
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect())
}
