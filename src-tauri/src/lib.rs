use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, State};

mod websocket;
use websocket::{create_websocket_server, TrackUpdate, WebSocketServer};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TrackInfo {
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub duration: Option<f64>,
    pub thumbnail: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LyricLine {
    pub time: f64, // time in seconds
    pub text: String,
    pub duration: Option<f64>,
}

// Global state for current track and WebSocket server
type TrackState = Arc<Mutex<Option<TrackInfo>>>;
type WebSocketState = Arc<Mutex<Option<WebSocketServer>>>;

#[tauri::command]
async fn get_current_track(state: State<'_, TrackState>) -> Result<Option<TrackInfo>, String> {
    let track = state.lock().map_err(|e| e.to_string())?;
    Ok(track.clone())
}

#[tauri::command]
async fn set_current_track(track: TrackInfo, state: State<'_, TrackState>) -> Result<(), String> {
    let mut current_track = state.lock().map_err(|e| e.to_string())?;
    *current_track = Some(track);
    Ok(())
}

#[tauri::command]
async fn fetch_lyrics(track_name: String, artist_name: String) -> Result<Vec<LyricLine>, String> {
    // Build request URL
    let url = format!(
        "https://lrclib.net/api/search?track_name={}&artist_name={}",
        urlencoding::encode(&track_name),
        urlencoding::encode(&artist_name)
    );

    // Make HTTP request
    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("User-Agent", "Lyryc/0.1.0")
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("HTTP error: {}", response.status()));
    }

    let json: Vec<serde_json::Value> = response
        .json()
        .await
        .map_err(|e| format!("JSON parse error: {}", e))?;

    if json.is_empty() {
        return Err("No lyrics found".to_string());
    }

    // Find first result with synced lyrics
    let track_data = json
        .iter()
        .find(|item| item["syncedLyrics"].as_str().is_some())
        .or(json.first())
        .ok_or("No valid track data found")?;

    let synced_lyrics = track_data["syncedLyrics"]
        .as_str()
        .ok_or("No synced lyrics available")?;

    // Parse LRC format
    let lyrics = parse_lrc_format(synced_lyrics);
    Ok(lyrics)
}

fn parse_lrc_format(lrc_content: &str) -> Vec<LyricLine> {
    let mut lyrics = Vec::new();
    
    for line in lrc_content.lines() {
        // Match LRC timestamp format: [mm:ss.xx] or [mm:ss]
        if let Some(caps) = regex::Regex::new(r"^\[(\d{2}):(\d{2})(?:\.(\d{2}))?\](.*)$")
            .unwrap()
            .captures(line)
        {
            let minutes: f64 = caps[1].parse().unwrap_or(0.0);
            let seconds: f64 = caps[2].parse().unwrap_or(0.0);
            let centiseconds: f64 = caps.get(3)
                .map(|m| m.as_str().parse().unwrap_or(0.0))
                .unwrap_or(0.0);
            let text = caps[4].trim().to_string();

            if !text.is_empty() {
                let time_in_seconds = minutes * 60.0 + seconds + centiseconds / 100.0;
                
                lyrics.push(LyricLine {
                    time: time_in_seconds,
                    text,
                    duration: None, // Will be calculated later
                });
            }
        }
    }

    // Sort by time and calculate durations
    lyrics.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());
    
    for i in 0..lyrics.len().saturating_sub(1) {
        lyrics[i].duration = Some(lyrics[i + 1].time - lyrics[i].time);
    }

    // Set duration for last line (default 3 seconds)
    if let Some(last) = lyrics.last_mut() {
        last.duration = Some(3.0);
    }

    lyrics
}

#[tauri::command]
async fn init_extension_connection(
    ws_state: State<'_, WebSocketState>,
    app_handle: tauri::AppHandle,
) -> Result<String, String> {
    let mut ws_server = create_websocket_server();
    
    // Set up callback to handle track updates from extension
    let app_handle_clone = app_handle.clone();
    ws_server.set_track_callback(move |track_update: TrackUpdate| {
        let track_info = TrackInfo {
            title: track_update.title,
            artist: track_update.artist,
            album: None,
            duration: None,
            thumbnail: track_update.thumbnail,
        };
        
        // Emit event to frontend
        if let Err(e) = app_handle_clone.emit("track-updated", &track_info) {
            eprintln!("Failed to emit track-updated event: {}", e);
        }
        
        // Also emit play state
        if let Err(e) = app_handle_clone.emit("playback-state", &track_update.is_playing) {
            eprintln!("Failed to emit playback-state event: {}", e);
        }
    });

    // Start WebSocket server in background
    tokio::spawn(async move {
        if let Err(e) = ws_server.start().await {
            eprintln!("WebSocket server error: {}", e);
        }
    });

    // Store a placeholder in the state to indicate server is running
    let mut server_guard = ws_state.lock().map_err(|e| e.to_string())?;
    *server_guard = Some(WebSocketServer::new(8765)); // Placeholder instance

    Ok("WebSocket server started on port 8765".to_string())
}

#[tauri::command]
async fn get_websocket_status(ws_state: State<'_, WebSocketState>) -> Result<bool, String> {
    let server_guard = ws_state.lock().map_err(|e| e.to_string())?;
    Ok(server_guard.is_some())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(TrackState::new(Mutex::new(None)))
        .manage(WebSocketState::new(Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![
            get_current_track,
            set_current_track,
            fetch_lyrics,
            init_extension_connection,
            get_websocket_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}