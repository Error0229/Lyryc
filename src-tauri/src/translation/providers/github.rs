//! GitHub AI provider for lyrics translation

use log::{debug, info, warn};
use reqwest::Client;
use serde_json::{json, Value};
use std::time::Duration;

use crate::translation::blacklist::{
    blacklist_model, is_model_blacklisted, RATE_LIMIT_BLACKLIST_DURATION,
};
use crate::translation::language::detect_language_fallback;
use crate::translation::parser::parse_translation_response;
use crate::translation::prompt::{build_translation_prompt, SYSTEM_PROMPT};
use crate::translation::types::{LyricLine, TranslationConfig, TranslationResult};

/// GitHub AI models organized by rate limit tier (free tier optimized)
///
/// LOW TIER (15 req/min, 150 req/day) - Primary models, use first
/// HIGH TIER (10 req/min, 50 req/day) - Fallback for quality
/// SPECIAL TIER (1-2 req/min, 8-30 req/day) - Last resort only
const GITHUB_MODELS: &[&str] = &[
    // === LOW TIER (150 req/day) - Burn these first ===
    "openai/gpt-4.1-mini",
    "openai/gpt-4.1-nano",
    "openai/gpt-4o-mini",
    "microsoft/Phi-4",
    "mistralai/Mistral-Small-3.1",
    "meta/Meta-Llama-3.1-8B-Instruct",
    "meta/Llama-3.2-11B-Vision-Instruct",
    "meta/Llama-4-Scout-17B-16E-Instruct",
    "cohere/Cohere-command-a",
    // === HIGH TIER (50 req/day) - Quality fallback ===
    "openai/gpt-4.1",
    "openai/gpt-4o",
    "meta/Llama-4-Maverick-17B-128E-Instruct-FP8",
    "meta/Llama-3.3-70B-Instruct",
    "meta/Llama-3.2-90B-Vision-Instruct",
    "mistralai/Mistral-Medium-3",
    "deepseek/DeepSeek-V3-0324",
    "cohere/Cohere-command-r-08-2024",
    "mistralai/Codestral-2501",
    // === SPECIAL TIER (8-30 req/day) - Last resort ===
    "xai/grok-3-mini",
    "xai/grok-3",
    "deepseek/DeepSeek-R1",
];

/// Translate lyrics using GitHub AI models
pub async fn translate_lyrics_github(
    lyrics: &[LyricLine],
    config: &TranslationConfig,
) -> Result<TranslationResult, String> {
    let github_token = config
        .github_api_key
        .clone()
        .filter(|key| !key.trim().is_empty())
        .or_else(|| std::env::var("GITHUB_TOKEN").ok())
        .ok_or_else(|| {
            "GitHub API key required for translation. Please enter your API key in the Translation settings.".to_string()
        })?;

    let available_models: Vec<&str> = GITHUB_MODELS
        .iter()
        .filter(|model| !is_model_blacklisted(model))
        .copied()
        .collect();

    if available_models.is_empty() {
        return Err("All GitHub AI models are currently rate-limited".to_string());
    }

    info!(
        "Available GitHub models: {} out of {} total",
        available_models.len(),
        GITHUB_MODELS.len()
    );

    let source_language = config.source_language.clone();

    for (i, model_id) in available_models.iter().enumerate() {
        match try_llm_translation(lyrics, config, &source_language, model_id, &github_token).await {
            Ok(result) => {
                info!(
                    "Translation successful with {}: {} lines translated",
                    model_id,
                    result.translated_lines.len()
                );
                return Ok(result);
            }
            Err(e) => {
                if e.contains("429") && e.contains("Rate limit") {
                    blacklist_model(model_id, RATE_LIMIT_BLACKLIST_DURATION);
                    warn!("Model {} rate limited, blacklisted for 24 hours", model_id);
                }

                debug!(
                    "Translation failed with {}: {} (attempt {}/{})",
                    model_id,
                    e,
                    i + 1,
                    available_models.len()
                );

                if i < available_models.len() - 1 {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            }
        }
    }

    Err("All LLM translation attempts failed".to_string())
}

async fn try_llm_translation(
    lyrics: &[LyricLine],
    config: &TranslationConfig,
    source_language: &str,
    model_id: &str,
    github_token: &str,
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
        "messages": [
            {"role": "system", "content": SYSTEM_PROMPT},
            {"role": "user", "content": translation_prompt}
        ],
        "temperature": 0.3,
        "max_tokens": 4096,
        "model": model_id
    });

    let response = client
        .post("https://models.github.ai/inference/chat/completions")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", github_token))
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
    let response_text = result["choices"][0]["message"]["content"]
        .as_str()
        .ok_or("No content in response".to_string())?;

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

/// Try GitHub alignment for line count mismatch
pub async fn try_github_alignment(prompt: &str, github_token: &str) -> Result<Vec<String>, String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;

    let payload = json!({
        "messages": [
            {"role": "user", "content": prompt}
        ],
        "temperature": 0.1,
        "max_tokens": 4096,
        "model": "openai/gpt-4o-mini"
    });

    let response = client
        .post("https://models.github.ai/inference/chat/completions")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", github_token))
        .json(&payload)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        return Err(format!(
            "GitHub alignment request failed: {}",
            response.status()
        ));
    }

    let result: Value = response.json().await.map_err(|e| e.to_string())?;
    let response_text = result["choices"][0]["message"]["content"]
        .as_str()
        .ok_or("No content in GitHub alignment response")?;

    Ok(response_text
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect())
}
