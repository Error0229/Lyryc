use crate::track_cleaning::clean_track_name;
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
