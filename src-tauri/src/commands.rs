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
