use crate::track_cleaning::{clean_track_name, clean_track_name_github_with_key, clean_track_name_gemini_with_key, clean_track_name_sync};
use crate::translation::{translate_lyrics, TranslationConfig, LyricLine, TranslationResult};
use crate::types::{TrackInfo, TrackState};
use tauri::State;

#[tauri::command]
pub async fn get_current_track(state: State<'_, TrackState>) -> Result<Option<TrackInfo>, String> {
    let track = state.lock().await;
    Ok(track.clone())
}

#[tauri::command]
pub async fn set_current_track(
    track: TrackInfo,
    state: State<'_, TrackState>,
) -> Result<(), String> {
    let mut current_track = state.lock().await;
    *current_track = Some(track);
    Ok(())
}

#[tauri::command]
pub async fn clean_track_name_command(track_name: String) -> Result<String, String> {
    Ok(clean_track_name(&track_name).await)
}

/// Enhanced track cleaning command that can use translation API keys
#[tauri::command]
pub async fn clean_track_name_with_config(
    track_name: String,
    config: Option<TranslationConfig>
) -> Result<String, String> {
    if let Some(config) = config {
        // Try GitHub AI first with provided API key
        if let Some(github_key) = &config.github_api_key {
            if !github_key.trim().is_empty() {
                log::debug!("Using provided GitHub API key for track cleaning");
                match clean_track_name_github_with_key(&track_name, Some(github_key.clone())).await {
                    Ok(cleaned) => return Ok(cleaned),
                    Err(e) => {
                        log::debug!("GitHub track cleaning with provided key failed: {}", e);
                        // Continue to Gemini fallback
                    }
                }
            } else {
                log::debug!("GitHub API key provided but empty, skipping GitHub AI");
            }
        } else {
            log::debug!("No GitHub API key provided in config");
        }

        // Try Gemini fallback with provided API key
        if let Some(gemini_key) = &config.gemini_api_key {
            if !gemini_key.trim().is_empty() {
                log::debug!("Using provided Gemini API key for track cleaning");
                match clean_track_name_gemini_with_key(&track_name, Some(gemini_key.clone())).await {
                    Ok(cleaned) => return Ok(cleaned),
                    Err(e) => {
                        log::debug!("Gemini track cleaning with provided key failed: {}", e);
                        // Continue to standard fallback
                    }
                }
            } else {
                log::debug!("Gemini API key provided but empty, skipping Gemini AI");
            }
        } else {
            log::debug!("No Gemini API key provided in config");
        }
    }

    // Fallback to standard flow (check env vars then regex)
    Ok(clean_track_name(&track_name).await)
}

#[tauri::command]
pub async fn translate_lyrics_command(
    lyrics: Vec<LyricLine>,
    config: TranslationConfig,
) -> Result<TranslationResult, String> {
    translate_lyrics(lyrics, config).await
}

#[tauri::command]
pub async fn get_supported_languages() -> Result<Vec<(String, String)>, String> {
    Ok(vec![
        ("auto".to_string(), "Auto-detect".to_string()),
        ("en".to_string(), "English".to_string()),
        ("es".to_string(), "Spanish".to_string()),
        ("fr".to_string(), "French".to_string()),
        ("de".to_string(), "German".to_string()),
        ("it".to_string(), "Italian".to_string()),
        ("pt".to_string(), "Portuguese".to_string()),
        ("ru".to_string(), "Russian".to_string()),
        ("ja".to_string(), "Japanese".to_string()),
        ("ko".to_string(), "Korean".to_string()),
        ("zh".to_string(), "Chinese (Simplified)".to_string()),
        ("zh-tw".to_string(), "Chinese (Traditional)".to_string()),
        ("ar".to_string(), "Arabic".to_string()),
        ("hi".to_string(), "Hindi".to_string()),
        ("th".to_string(), "Thai".to_string()),
        ("vi".to_string(), "Vietnamese".to_string()),
        ("tr".to_string(), "Turkish".to_string()),
        ("pl".to_string(), "Polish".to_string()),
        ("nl".to_string(), "Dutch".to_string()),
        ("sv".to_string(), "Swedish".to_string()),
        ("no".to_string(), "Norwegian".to_string()),
        ("da".to_string(), "Danish".to_string()),
        ("fi".to_string(), "Finnish".to_string()),
        ("he".to_string(), "Hebrew".to_string()),
        ("el".to_string(), "Greek".to_string()),
    ])
}
