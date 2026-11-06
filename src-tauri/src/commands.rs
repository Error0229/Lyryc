use log::info;
use crate::cleaning::{clean_track_name, clean_track_name_with_keys};
use crate::llm::{self, Task, NOT_CONFIGURED};
use crate::translation::{self, translate_lyrics, LyricLine, TranslationConfig, TranslationResult};
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

/// Track cleaning with an optional legacy config. The config's keys are only
/// used when neither the `cleaning` nor the `translation` route is set.
#[tauri::command]
pub async fn clean_track_name_with_config(
    track_name: String,
    config: Option<TranslationConfig>,
) -> Result<String, String> {
    let call_keys = config.map(|c| c.legacy_keys()).unwrap_or_default();
    Ok(clean_track_name_with_keys(&track_name, &call_keys).await)
}

#[tauri::command]
pub async fn translate_lyrics_command(
    lyrics: Vec<LyricLine>,
    config: TranslationConfig,
) -> Result<TranslationResult, String> {
    let target = llm::resolve(Task::Translation, &config.legacy_keys()).ok_or_else(|| {
        format!("{NOT_CONFIGURED} no provider is set up for translation. Add one in Settings.")
    })?;
    let key = translation::cache::key(&lyrics, &config, &target);
    if let Some(hit) = translation::cache::get(&key, &lyrics) {
        info!("Translation for {} lines from the cache", lyrics.len());
        return Ok(hit);
    }
    let result = translate_lyrics(lyrics, config, &target).await?;
    translation::cache::put(&key, &result);
    Ok(result)
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
        (
            "zh".to_string(),
            "Chinese Simplified (简体中文)".to_string(),
        ),
        (
            "zh-cn".to_string(),
            "Chinese Simplified - Mainland (简体中文)".to_string(),
        ),
        (
            "zh-tw".to_string(),
            "Chinese Traditional - Taiwan (繁體中文)".to_string(),
        ),
        (
            "zh-hk".to_string(),
            "Chinese Traditional - Hong Kong (繁體中文)".to_string(),
        ),
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
