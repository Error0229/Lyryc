//! Google Gemini provider for track cleaning

use log::{debug, info};
use serde_json::{json, Value};

use crate::cleaning::cache::get_shared_http_client;
use crate::cleaning::providers::github::parse_artist_track_cleaning_response;
use crate::cleaning::types::ArtistTrackCleaningResult;

/// Cleans track name using Gemini 2.5 Flash-Lite when API key is available
pub async fn clean_track_name_gemini(track_name: &str) -> Result<String, String> {
    clean_track_name_gemini_with_key(track_name, None).await
}

/// Cleans track name using Gemini with optional API key override
pub async fn clean_track_name_gemini_with_key(
    track_name: &str,
    api_key_override: Option<String>,
) -> Result<String, String> {
    dotenv::dotenv().ok();

    let gemini_api_key = api_key_override
        .filter(|key| !key.trim().is_empty())
        .or_else(|| std::env::var("GEMINI_API_KEY").ok())
        .ok_or_else(|| "No Gemini API key found".to_string())?;

    let model_id = "gemini-2.5-flash-lite";

    match try_gemini_cleaning(track_name, model_id, &gemini_api_key).await {
        Ok(cleaned) => {
            info!(
                "Gemini cleaning successful with {}: '{}' -> '{}'",
                model_id, track_name, cleaned
            );
            Ok(cleaned)
        }
        Err(e) => {
            debug!("Gemini cleaning failed with {}: {}", model_id, e);
            Err(e)
        }
    }
}

async fn try_gemini_cleaning(
    track_name: &str,
    model_id: &str,
    gemini_api_key: &str,
) -> Result<String, String> {
    let client = get_shared_http_client().clone();

    let user_content = format!(
        r#"Clean this track title by removing platform/marketing fluff but keeping the actual song title:

Original: "{}"

Rules:
- Remove: "Official Video", "MV", "YouTube Music", platform info, file extensions
- Keep: song title, version markers (Acoustic, Remix, Live), artist collaborations if part of title
- If "Artist - Title" format, return just the title part
- Remove brackets/parentheses containing promotional text
- Keep Japanese/other language titles as-is. When both original and translation are present, prefer the original language (e.g., "火星人 - Martian" becomes "火星人")

Return only the cleaned title, nothing else:"#,
        track_name
    );

    let system_instruction = r#"You are a music metadata cleaner. Clean track titles by removing promotional/platform content while preserving the actual song title. IMPORTANT: When both original language and English translation are present, always prefer the original language. Be concise and accurate. Return only the cleaned title with no explanations or formatting."#;

    let payload = json!({
        "contents": [{
            "parts": [{
                "text": format!("{}\n\n{}", system_instruction, user_content)
            }]
        }],
        "generationConfig": {
            "temperature": 0.1,
            "maxOutputTokens": 512,
            "topP": 0.8,
            "topK": 10
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

    if let Some(finish_reason) = result["candidates"][0]["finishReason"].as_str() {
        if finish_reason == "MAX_TOKENS" {
            return Err("Gemini response truncated due to token limit".to_string());
        }
        if finish_reason != "STOP" {
            return Err(format!(
                "Gemini response incomplete. Finish reason: {}",
                finish_reason
            ));
        }
    }

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

    let cleaned_title = response_text
        .trim()
        .lines()
        .next()
        .unwrap_or(response_text.trim())
        .trim_matches('"')
        .trim();

    if cleaned_title.is_empty() {
        return Err("Empty response from Gemini".to_string());
    }

    if cleaned_title.len() > track_name.len() * 2 {
        return Err("Gemini response unreasonably long".to_string());
    }

    Ok(cleaned_title.to_string())
}

/// Clean artist and track names using Gemini
pub async fn clean_artist_and_track_names_gemini(
    artist_name: &str,
    track_name: &str,
) -> Result<ArtistTrackCleaningResult, String> {
    let gemini_api_key = std::env::var("GEMINI_API_KEY").map_err(|_| {
        "Gemini API key required. Please set GEMINI_API_KEY environment variable.".to_string()
    })?;

    try_gemini_artist_track_cleaning(
        artist_name,
        track_name,
        "gemini-flash-lite-latest",
        &gemini_api_key,
    )
    .await
}

async fn try_gemini_artist_track_cleaning(
    artist_name: &str,
    track_name: &str,
    model_id: &str,
    gemini_api_key: &str,
) -> Result<ArtistTrackCleaningResult, String> {
    let client = get_shared_http_client().clone();

    let prompt = format!(
        r#"Clean this artist name and track title into canonical forms:

ARTIST: {}
TRACK: {}

Return ONE JSON object with this exact structure:
{{
  "artist": {{
    "cleaned_title": "cleaned artist name",
    "cleaned_artists": ["artist1", "artist2"],
    "kept_markers": [],
    "removed_context": [],
    "confidence": 0.95,
    "notes": "brief explanation"
  }},
  "track": {{
    "cleaned_title": "cleaned track title",
    "kept_markers": [],
    "removed_context": [],
    "confidence": 0.95,
    "notes": "brief explanation"
  }}
}}

Remove: platform names, "Official", "MV", record labels, years, marketing text.
Keep: artist identity, meaningful version markers, original language.

CRITICAL - Romanization Conversion:
- If artist name appears romanized (e.g., "Natori", "Yoasobi", "Kenshi Yonezu"), convert to original Japanese/Korean/Chinese script
- Examples: "Natori" → "なとり", "Yoasobi" → "YOASOBI", "Kenshi Yonezu" → "米津玄師"

Multi-artist handling:
- Separate collaborations: "Thomas Reid 和 Rxseboy" → two separate artists
- Recognize all separators: 和, と, 와, &, feat., vs, with, and, y, et
- Preserve original language for each artist name
- Output multiple artists in cleaned_artists array"#,
        artist_name, track_name
    );

    let payload = json!({
        "contents": [{
            "parts": [{"text": prompt}]
        }],
        "generationConfig": {
            "temperature": 0.1,
            "topK": 1,
            "topP": 0.1,
            "maxOutputTokens": 500
        }
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

    let status = response.status();
    if !status.is_success() {
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(format!("HTTP {}: {}", status, error_text));
    }

    let result: Value = response.json().await.map_err(|e| e.to_string())?;
    let response_text = result["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .ok_or("No text in Gemini response")?;

    parse_artist_track_cleaning_response(response_text)
}
