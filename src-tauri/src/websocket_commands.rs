use crate::types::{TrackInfo, TrackState, WebSocketState};
use crate::websocket::{create_websocket_server, TrackUpdate};
use log::{error, info, warn};
use std::sync::{Arc, Mutex as StdMutex};
use tauri::{Emitter, State};

#[tauri::command]
pub async fn init_extension_connection(
    ws_state: State<'_, WebSocketState>,
    track_state: State<'_, TrackState>,
    app_handle: tauri::AppHandle,
) -> Result<String, String> {
    // Ensure idempotent initialization under a single lock
    let server_arc = {
        let mut server_guard = ws_state.lock().await;
        if server_guard.is_some() {
            info!("WebSocket server already initialized; skipping re-bind");
            return Ok("WebSocket server already running".to_string());
        }

        // Create server and register callbacks
        let mut ws_server = create_websocket_server();
        let app_handle_clone = app_handle.clone();
        let track_state_clone = track_state.inner().clone();
        // (title, artist) we last announced. Checked synchronously so messages are
        // judged in arrival order. Any message may introduce the track, so opening
        // the app mid-song, or the extension restarting, picks it up at once.
        let last_track: Arc<StdMutex<Option<(String, String)>>> = Arc::default();
        ws_server.set_track_callback(move |track_update: TrackUpdate| {
            let key = (track_update.title.clone(), track_update.artist.clone());
            let changed = !key.0.trim().is_empty() && {
                let mut last = last_track.lock().unwrap();
                let changed = last.as_ref() != Some(&key);
                *last = Some(key);
                changed
            };

            if changed {
                let track_info = TrackInfo {
                    title: track_update.title.clone(),
                    artist: track_update.artist.clone(),
                    album: None,
                    duration: track_update.duration.filter(|d| *d > 0.0), // 0 = unknown
                    thumbnail: track_update.thumbnail.clone(),
                    video_id: crate::ytmusic::video_id_from_url(&track_update.url),
                };
                info!("Track changed: '{}' by '{}'", track_info.title, track_info.artist);
                if let Err(e) = app_handle_clone.emit("track-updated", &track_info) {
                    error!("Failed to emit track-updated event: {}", e);
                }
                let track_state = track_state_clone.clone();
                tokio::spawn(async move {
                    *track_state.lock().await = Some(track_info);
                });
            }

            // Always emit playback state and time updates (they change frequently)
            if let Err(e) = app_handle_clone.emit("playback-state", &track_update.is_playing) {
                error!("Failed to emit playback-state event: {}", e);
            }

            if let Some(current_time) = track_update.current_time {
                if let Err(e) = app_handle_clone.emit(
                    "track-time-update",
                    &serde_json::json!({
                        "currentTime": current_time,
                        "duration": track_update.duration.unwrap_or(0.0),
                        "isPlaying": track_update.is_playing
                    }),
                ) {
                    error!("Failed to emit track-time-update event: {}", e);
                }
            }
        });

        let server_arc = Arc::new(ws_server);
        *server_guard = Some(server_arc.clone());
        server_arc
    };

    // Clone state handle so we can clear it on bind failure
    let ws_state_for_spawn = ws_state.inner().clone();
    let server_for_spawn = server_arc.clone();
    tokio::spawn(async move {
        if let Err(e) = server_for_spawn.start().await {
            error!("WebSocket server failed to start: {}", e);
            // Clear stored server to allow retry on next init call
            let mut guard = ws_state_for_spawn.lock().await;
            *guard = None;
        }
    });

    Ok("WebSocket server started on port 8765".to_string())
}

#[tauri::command]
pub async fn get_websocket_status(ws_state: State<'_, WebSocketState>) -> Result<bool, String> {
    let server_guard = ws_state.lock().await;
    Ok(server_guard.is_some())
}

#[tauri::command]
pub async fn get_websocket_clients_count(
    ws_state: State<'_, WebSocketState>,
) -> Result<usize, String> {
    let server_guard = ws_state.lock().await;
    if let Some(ref server) = *server_guard {
        let clients = server.clients.lock().await;
        let count = clients.len();
        info!("Current WebSocket clients count: {}", count);

        // Debug: print client IDs
        for (client_id, _) in clients.iter() {
            info!("Connected client: {}", client_id);
        }

        Ok(count)
    } else {
        warn!("WebSocket server not available");
        Ok(0)
    }
}

#[tauri::command]
pub async fn debug_websocket_server(ws_state: State<'_, WebSocketState>) -> Result<String, String> {
    let server_guard = ws_state.lock().await;
    if let Some(ref server) = *server_guard {
        let clients = server.clients.lock().await;
        let mut debug_info = format!("WebSocket Server Debug Info:\n");
        debug_info.push_str(&format!("- Server exists: Yes\n"));
        debug_info.push_str(&format!("- Port: 8765\n"));
        debug_info.push_str(&format!("- Connected clients: {}\n", clients.len()));

        for (client_id, _) in clients.iter() {
            debug_info.push_str(&format!("  - Client ID: {}\n", client_id));
        }

        info!("Debug info requested: {}", debug_info);
        Ok(debug_info)
    } else {
        let error_msg = "WebSocket server instance not found - server failed to initialize";
        warn!("{}", error_msg);
        Ok(error_msg.to_string())
    }
}

#[tauri::command]
pub async fn control_playback(action: String, seek_time: Option<f64>) -> Result<String, String> {
    info!("Playback control: {} {:?}", action, seek_time);

    // For now, return success - we'll implement browser control later
    // This would need to send messages to the extension to control the browser
    Ok(format!("Playback control '{}' executed", action))
}

#[tauri::command]
pub async fn send_playback_command(
    command: String,
    seek_time: Option<f64>,
    ws_state: State<'_, WebSocketState>,
) -> Result<String, String> {
    info!("Sending playback command: {} {:?}", command, seek_time);

    let server_guard = ws_state.lock().await;
    if let Some(ref server) = *server_guard {
        server.send_playback_command(command, seek_time).await?;
        Ok("Command sent to extension".to_string())
    } else {
        Err("WebSocket server not available".to_string())
    }
}
