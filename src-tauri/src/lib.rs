use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
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
type WebSocketState = Arc<Mutex<Option<Arc<WebSocketServer>>>>;

#[tauri::command]
async fn get_current_track(state: State<'_, TrackState>) -> Result<Option<TrackInfo>, String> {
    let track = state.lock().await;
    Ok(track.clone())
}

#[tauri::command]
async fn set_current_track(track: TrackInfo, state: State<'_, TrackState>) -> Result<(), String> {
    let mut current_track = state.lock().await;
    *current_track = Some(track);
    Ok(())
}

#[tauri::command]
async fn fetch_lyrics(track_name: String, artist_name: String) -> Result<Vec<LyricLine>, String> {
    println!("Fetching lyrics for: {} by {}", track_name, artist_name);
    
    // Build request URL
    let url = format!(
        "https://lrclib.net/api/search?track_name={}&artist_name={}",
        urlencoding::encode(&track_name),
        urlencoding::encode(&artist_name)
    );
    
    println!("Request URL: {}", url);

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

    println!("Found {} search results", json.len());

    if json.is_empty() {
        return Err("No lyrics found".to_string());
    }

    // Try to find result with synced lyrics first, fallback to any lyrics
    let track_data = json
        .iter()
        .find(|item| {
            item["syncedLyrics"].as_str().is_some() && 
            !item["syncedLyrics"].as_str().unwrap().trim().is_empty()
        })
        .or_else(|| {
            json.iter().find(|item| {
                item["plainLyrics"].as_str().is_some() && 
                !item["plainLyrics"].as_str().unwrap().trim().is_empty()
            })
        })
        .or(json.first())
        .ok_or("No valid track data found")?;

    // Check for synced lyrics first
    if let Some(synced_lyrics) = track_data["syncedLyrics"].as_str() {
        if !synced_lyrics.trim().is_empty() {
            println!("Found synced lyrics, parsing LRC format");
            let lyrics = parse_lrc_format(synced_lyrics);
            if !lyrics.is_empty() {
                return Ok(lyrics);
            }
        }
    }

    // Fallback to plain lyrics
    if let Some(plain_lyrics) = track_data["plainLyrics"].as_str() {
        if !plain_lyrics.trim().is_empty() {
            println!("Found plain lyrics, converting to unsynced format");
            let lyrics = convert_plain_lyrics_to_lines(plain_lyrics);
            return Ok(lyrics);
        }
    }

    Err("No usable lyrics found".to_string())
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

fn convert_plain_lyrics_to_lines(plain_lyrics: &str) -> Vec<LyricLine> {
    let mut lyrics = Vec::new();
    let lines: Vec<&str> = plain_lyrics.lines().collect();
    
    // Create unsynced lyrics with placeholder timing (5 seconds per line)
    for (index, line) in lines.iter().enumerate() {
        let text = line.trim();
        if !text.is_empty() {
            lyrics.push(LyricLine {
                time: (index as f64) * 5.0, // 5 seconds per line
                text: text.to_string(),
                duration: Some(5.0),
            });
        }
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
            title: track_update.title.clone(),
            artist: track_update.artist.clone(),
            album: None,
            duration: track_update.duration,
            thumbnail: track_update.thumbnail.clone(),
        };
        
        // Always emit track update for new songs
        if track_update.current_time.is_none() || track_update.current_time == Some(0.0) {
            // This is a track change, emit track updated
            if let Err(e) = app_handle_clone.emit("track-updated", &track_info) {
                eprintln!("Failed to emit track-updated event: {}", e);
            }
        }
        
        // Always emit playback state
        if let Err(e) = app_handle_clone.emit("playback-state", &track_update.is_playing) {
            eprintln!("Failed to emit playback-state event: {}", e);
        }
        
        // Emit time updates if we have timing info
        if let Some(current_time) = track_update.current_time {
            if let Err(e) = app_handle_clone.emit("track-time-update", &serde_json::json!({
                "currentTime": current_time,
                "duration": track_update.duration.unwrap_or(0.0),
                "isPlaying": track_update.is_playing
            })) {
                eprintln!("Failed to emit track-time-update event: {}", e);
            }
        }
    });

    // Store the server instance before starting it
    let server_arc = Arc::new(ws_server);
    {
        let mut server_guard = ws_state.lock().await;
        *server_guard = Some(server_arc.clone());
    }

    // Start WebSocket server in background
    let server_for_spawn = server_arc.clone();
    tokio::spawn(async move {
        if let Err(e) = server_for_spawn.start().await {
            eprintln!("WebSocket server error: {}", e);
        }
    });

    Ok("WebSocket server started on port 8765".to_string())
}

#[tauri::command]
async fn get_websocket_status(ws_state: State<'_, WebSocketState>) -> Result<bool, String> {
    let server_guard = ws_state.lock().await;
    Ok(server_guard.is_some())
}

#[tauri::command]
async fn control_playback(action: String, seek_time: Option<f64>) -> Result<String, String> {
    println!("Playback control: {} {:?}", action, seek_time);
    
    // For now, return success - we'll implement browser control later
    // This would need to send messages to the extension to control the browser
    Ok(format!("Playback control '{}' executed", action))
}

#[tauri::command]
async fn send_playback_command(
    command: String, 
    seek_time: Option<f64>,
    ws_state: State<'_, WebSocketState>
) -> Result<String, String> {
    println!("Sending playback command: {} {:?}", command, seek_time);
    
    let server_guard = ws_state.lock().await;
    if let Some(ref server) = *server_guard {
        server.send_playback_command(command, seek_time).await?;
        Ok("Command sent to extension".to_string())
    } else {
        Err("WebSocket server not available".to_string())
    }
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
            get_websocket_status,
            control_playback,
            send_playback_command
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}