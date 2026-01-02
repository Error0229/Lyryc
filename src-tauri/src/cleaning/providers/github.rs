//! GitHub AI provider for track cleaning

use log::{debug, info, warn};
use serde_json::{json, Value};
use std::time::Duration;

use crate::cleaning::cache::get_shared_http_client;
use crate::cleaning::prompt::{
    build_artist_track_cleaning_prompt, build_track_cleaning_prompt, clean_markdown_formatting,
    ARTIST_TRACK_CLEANING_SYSTEM_PROMPT, TRACK_CLEANING_SYSTEM_PROMPT,
};
use crate::cleaning::regex::clean_track_name_sync;
use crate::cleaning::types::{ArtistTrackCleaningResult, CleaningResult};
use crate::translation::{blacklist_model, is_model_blacklisted};

/// High-performance GitHub AI models ranked by accuracy
const GITHUB_MODELS: &[&str] = &[
    "meta/Llama-4-Maverick-17B-128E-Instruct-FP8",
    "openai/gpt-4.1-nano",
    "openai/gpt-4o-mini",
    "mistral-ai/Mistral-Large-2411",
    "mistral-ai/mistral-medium-2505",
    "mistral-ai/mistral-small-2503",
    "meta/Meta-Llama-3.1-8B-Instruct",
    "mistral-ai/Codestral-2501",
    "deepseek/DeepSeek-V3-0324",
    "meta/Llama-3.3-70B-Instruct",
    "xai/grok-3",
    "meta/Llama-4-Scout-17B-16E-Instruct",
];

const RATE_LIMIT_BLACKLIST_DURATION: u64 = 86400; // 24 hours

fn normalize_model_id(model_id: &str) -> String {
    let trimmed = model_id.trim();
    if let Some((provider, rest)) = trimmed.split_once('/') {
        let provider_lower = provider.to_lowercase();
        let normalized_provider = match provider_lower.as_str() {
            "mistral ai" | "mistralai" => "mistral-ai",
            _ => provider,
        };
        return format!("{}/{}", normalized_provider, rest);
    }
    trimmed.to_string()
}

fn is_rate_limited_error(error: &str) -> bool {
    error.starts_with("HTTP 429")
}

/// Cleans track name using GitHub AI models when token is available
pub async fn clean_track_name_github(track_name: &str) -> Result<String, String> {
    clean_track_name_github_with_key(track_name, None).await
}

/// Cleans track name using GitHub AI models with optional API key override
pub async fn clean_track_name_github_with_key(
    track_name: &str,
    api_key_override: Option<String>,
) -> Result<String, String> {
    let github_token = if let Some(key) = api_key_override {
        if !key.trim().is_empty() {
            key
        } else {
            return Err("Provided GitHub token is empty".to_string());
        }
    } else {
        dotenv::dotenv().ok();
        std::env::var("GITHUB_TOKEN").map_err(|_| "No GitHub token found".to_string())?
    };

    if github_token.trim().is_empty() {
        return Err("GitHub token is empty".to_string());
    }

    let mut available_models = Vec::new();
    for &model in GITHUB_MODELS.iter() {
        let normalized = normalize_model_id(model);
        if !available_models.contains(&normalized) && !is_model_blacklisted(&normalized) {
            available_models.push(normalized);
        }
    }

    if available_models.is_empty() {
        debug!("All GitHub AI models are currently rate-limited, falling back to regex cleaning");
        return Ok(clean_track_name_sync(track_name));
    }

    info!(
        "Available GitHub models for track cleaning: {} out of {} total",
        available_models.len(),
        GITHUB_MODELS.len()
    );

    for (i, model_id) in available_models.iter().enumerate() {
        match try_llm_cleaning(track_name, model_id, &github_token).await {
            Ok(cleaned) => {
                info!(
                    "LLM cleaning successful with {}: '{}' -> '{}'",
                    model_id, track_name, cleaned
                );
                return Ok(cleaned);
            }
            Err(e) => {
                if is_rate_limited_error(&e) {
                    blacklist_model(model_id, RATE_LIMIT_BLACKLIST_DURATION);
                    warn!(
                        "Model {} rate limited for track cleaning, blacklisted for 24 hours",
                        model_id
                    );
                }

                debug!(
                    "LLM cleaning failed with {}: {} (attempt {}/{})",
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

    debug!("All LLM attempts failed, falling back to regex cleaning");
    Ok(clean_track_name_sync(track_name))
}

async fn try_llm_cleaning(
    track_name: &str,
    model_id: &str,
    github_token: &str,
) -> Result<String, String> {
    let client = get_shared_http_client().clone();
    let user_content = build_track_cleaning_prompt(track_name);

    let payload = json!({
        "messages": [
            {"role": "system", "content": TRACK_CLEANING_SYSTEM_PROMPT},
            {"role": "user", "content": user_content}
        ],
        "temperature": 0.1,
        "max_tokens": 1000,
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

    parse_track_cleaning_response(response_text)
}

fn parse_track_cleaning_response(response_text: &str) -> Result<String, String> {
    let cleaned_response = clean_markdown_formatting(response_text);
    let lines: Vec<&str> = cleaned_response.trim().split('\n').collect();

    for line in lines {
        let trimmed = line.trim();
        if trimmed.starts_with('{') && trimmed.ends_with('}') {
            if let Ok(parsed) = serde_json::from_str::<Value>(trimmed) {
                if let Some(cleaned_title) = parsed.get("cleaned_title") {
                    if let Some(title_str) = cleaned_title.as_str() {
                        let title_trimmed = title_str.trim();
                        if !title_trimmed.is_empty() {
                            return Ok(title_trimmed.to_string());
                        }
                    }
                }
            }
        }
    }

    Err("Could not parse valid cleaned title from LLM response".to_string())
}

/// Clean artist and track names using GitHub AI
pub async fn clean_artist_and_track_names_github(
    artist_name: &str,
    track_name: &str,
) -> Result<ArtistTrackCleaningResult, String> {
    let github_token = std::env::var("GITHUB_TOKEN").map_err(|_| {
        "GitHub API key required for cleaning. Please set GITHUB_TOKEN environment variable."
            .to_string()
    })?;

    let mut available_models = Vec::new();
    for &model in GITHUB_MODELS.iter() {
        let normalized = normalize_model_id(model);
        if !available_models.contains(&normalized) && !is_model_blacklisted(&normalized) {
            available_models.push(normalized);
        }
    }

    if available_models.is_empty() {
        return Err("All GitHub AI models are currently rate-limited".to_string());
    }

    for model_id in available_models {
        match try_llm_artist_track_cleaning(
            artist_name,
            track_name,
            model_id.as_str(),
            &github_token,
        )
        .await
        {
            Ok(result) => {
                info!("Artist/track cleaning successful with {}", model_id);
                return Ok(result);
            }
            Err(e) => {
                if is_rate_limited_error(&e) {
                    blacklist_model(&model_id, RATE_LIMIT_BLACKLIST_DURATION);
                    warn!("Model {} rate limited, blacklisted for 24 hours", model_id);
                }
                debug!("Cleaning failed with {}: {}", model_id, e);
            }
        }
    }

    Err("All GitHub AI cleaning attempts failed".to_string())
}

async fn try_llm_artist_track_cleaning(
    artist_name: &str,
    track_name: &str,
    model_id: &str,
    github_token: &str,
) -> Result<ArtistTrackCleaningResult, String> {
    let client = get_shared_http_client().clone();
    let user_content = build_artist_track_cleaning_prompt(artist_name, track_name);

    let payload = json!({
        "model": model_id,
        "messages": [
            {"role": "system", "content": ARTIST_TRACK_CLEANING_SYSTEM_PROMPT},
            {"role": "user", "content": user_content}
        ],
        "max_tokens": 500,
        "temperature": 0.1
    });

    let response = client
        .post("https://models.github.ai/inference/chat/completions")
        .header("Authorization", format!("Bearer {}", github_token))
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = response.status();
    if !status.is_success() {
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

    parse_artist_track_cleaning_response(response_text)
}

pub fn parse_artist_track_cleaning_response(
    response_text: &str,
) -> Result<ArtistTrackCleaningResult, String> {
    let cleaned_text = response_text.trim();

    match serde_json::from_str::<Value>(cleaned_text) {
        Ok(json_value) => {
            let artist_obj = json_value["artist"]
                .as_object()
                .ok_or("Missing 'artist' field in response")?;
            let track_obj = json_value["track"]
                .as_object()
                .ok_or("Missing 'track' field in response")?;

            let artist_result = CleaningResult {
                cleaned_title: artist_obj["cleaned_title"]
                    .as_str()
                    .ok_or("Missing cleaned_title in artist")?
                    .to_string(),
                cleaned_artist: None,
                cleaned_artists: artist_obj["cleaned_artists"]
                    .as_array()
                    .unwrap_or(&vec![])
                    .iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_string())
                    .collect(),
                kept_markers: artist_obj["kept_markers"]
                    .as_array()
                    .unwrap_or(&vec![])
                    .iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_string())
                    .collect(),
                removed_context: artist_obj["removed_context"]
                    .as_array()
                    .unwrap_or(&vec![])
                    .iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_string())
                    .collect(),
                confidence: artist_obj["confidence"].as_f64().unwrap_or(0.8),
                notes: artist_obj["notes"].as_str().map(|s| s.to_string()),
            };

            let track_result = CleaningResult {
                cleaned_title: track_obj["cleaned_title"]
                    .as_str()
                    .ok_or("Missing cleaned_title in track")?
                    .to_string(),
                cleaned_artist: None,
                cleaned_artists: vec![],
                kept_markers: track_obj["kept_markers"]
                    .as_array()
                    .unwrap_or(&vec![])
                    .iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_string())
                    .collect(),
                removed_context: track_obj["removed_context"]
                    .as_array()
                    .unwrap_or(&vec![])
                    .iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_string())
                    .collect(),
                confidence: track_obj["confidence"].as_f64().unwrap_or(0.8),
                notes: track_obj["notes"].as_str().map(|s| s.to_string()),
            };

            Ok(ArtistTrackCleaningResult {
                artist: artist_result,
                track: track_result,
            })
        }
        Err(e) => {
            warn!(
                "Failed to parse artist/track cleaning response as JSON: {}",
                e
            );
            warn!("Response was: {}", cleaned_text);
            Err(format!("Failed to parse cleaning response: {}", e))
        }
    }
}
