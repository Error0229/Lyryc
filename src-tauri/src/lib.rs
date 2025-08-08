use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{Emitter, State};
use tokio::sync::Mutex;

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
    info!("Fetching lyrics for: {} by {}", track_name, artist_name);

    // Try multiple search strategies with both exact and wildcard searches
    enum SearchStrategy {
        Exact(String, String), // track_name, artist_name
        Wildcard(String),      // q parameter
    }

    let cleaned_track = clean_track_name(&track_name);
    let track_without_artist = remove_artist_from_track(&track_name, &artist_name);
    let cleaned_track_without_artist = clean_track_name(&track_without_artist);

    let search_strategies = vec![
        // Wildcard searches (often most effective)
        SearchStrategy::Wildcard(format!("{} {}", track_name, artist_name)),
        SearchStrategy::Wildcard(format!("{} {}", cleaned_track, artist_name)),
        SearchStrategy::Wildcard(track_name.clone()),
        SearchStrategy::Wildcard(cleaned_track.clone()),
        // Exact searches
        SearchStrategy::Exact(track_name.clone(), artist_name.clone()),
        SearchStrategy::Exact(cleaned_track.clone(), artist_name.clone()),
        SearchStrategy::Exact(track_without_artist.clone(), artist_name.clone()),
        SearchStrategy::Exact(cleaned_track_without_artist.clone(), artist_name.clone()),
    ];

    for strategy in search_strategies {
        match &strategy {
            SearchStrategy::Exact(track, artist) => {
                if track.trim().is_empty() {
                    continue;
                }
                debug!("Trying exact strategy: '{}' by '{}'", track, artist);
                if let Ok(result) = try_fetch_lyrics_exact(track, artist).await {
                    if !result.is_empty() {
                        info!("Success with exact strategy: '{}' by '{}'", track, artist);
                        return Ok(result);
                    }
                }
            }
            SearchStrategy::Wildcard(query) => {
                if query.trim().is_empty() {
                    continue;
                }
                debug!("Trying wildcard strategy: '{}'", query);
                if let Ok(result) = try_fetch_lyrics_wildcard(query).await {
                    if !result.is_empty() {
                        info!("Success with wildcard strategy: '{}'", query);
                        return Ok(result);
                    }
                }
            }
        }
    }

    warn!("No lyrics found after trying all strategies for '{}' by '{}'", track_name, artist_name);
    Err("No lyrics found after all strategies".to_string())
}

async fn try_fetch_lyrics_exact(
    track_name: &str,
    artist_name: &str,
) -> Result<Vec<LyricLine>, String> {
    // Build request URL
    let mut url = format!(
        "https://lrclib.net/api/search?track_name={}",
        urlencoding::encode(track_name)
    );

    // Only add artist name if it's not empty
    if !artist_name.trim().is_empty() {
        url.push_str(&format!(
            "&artist_name={}",
            urlencoding::encode(artist_name)
        ));
    }

    debug!("Request URL: {}", url);

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

    debug!("Found {} search results", json.len());

    if json.is_empty() {
        return Err("No lyrics found".to_string());
    }

    // Try to find result with synced lyrics first, fallback to any lyrics
    let track_data = json
        .iter()
        .find(|item| {
            item["syncedLyrics"].as_str().is_some()
                && !item["syncedLyrics"].as_str().unwrap().trim().is_empty()
        })
        .or_else(|| {
            json.iter().find(|item| {
                item["plainLyrics"].as_str().is_some()
                    && !item["plainLyrics"].as_str().unwrap().trim().is_empty()
            })
        })
        .or(json.first())
        .ok_or("No valid track data found")?;

    // Check for synced lyrics first
    if let Some(synced_lyrics) = track_data["syncedLyrics"].as_str() {
        if !synced_lyrics.trim().is_empty() {
            info!("Found synced lyrics, parsing LRC format");
            let lyrics = parse_lrc_format(synced_lyrics);
            if !lyrics.is_empty() {
                return Ok(lyrics);
            }
        }
    }

    // Fallback to plain lyrics
    if let Some(plain_lyrics) = track_data["plainLyrics"].as_str() {
        if !plain_lyrics.trim().is_empty() {
            info!("Found plain lyrics, converting to unsynced format");
            let lyrics = convert_plain_lyrics_to_lines(plain_lyrics);
            return Ok(lyrics);
        }
    }

    Err("No usable lyrics found".to_string())
}

async fn try_fetch_lyrics_wildcard(query: &str) -> Result<Vec<LyricLine>, String> {
    // Build request URL using q parameter for wildcard search
    let url = format!(
        "https://lrclib.net/api/search?q={}",
        urlencoding::encode(query)
    );

    debug!("Request URL: {}", url);

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
        .map_err(|e| format!("Failed to parse JSON: {}", e))?;

    debug!("Found {} search results", json.len());

    if json.is_empty() {
        return Err("No results found".to_string());
    }

    // Find the best result (prefer synced lyrics)
    let mut best_result: Option<&serde_json::Value> = None;

    for result in &json {
        let has_synced = result
            .get("syncedLyrics")
            .and_then(|v| v.as_str())
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false);

        if has_synced {
            best_result = Some(result);
            break;
        }
    }

    // If no synced lyrics found, use the first result
    let chosen_result = best_result.unwrap_or(&json[0]);

    let synced_lyrics = chosen_result
        .get("syncedLyrics")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if synced_lyrics.trim().is_empty() {
        return Err("No synced lyrics available".to_string());
    }

    // Parse the LRC format
    Ok(parse_lrc_format(synced_lyrics))
}

fn clean_track_name(track_name: &str) -> String {
    use regex::Regex;

    let mut cleaned = track_name.to_string();
    debug!("Cleaning track name: '{}'", track_name);

    // First try to extract song title from quotes or brackets
    // Try Japanese quotes 「」
    if let Ok(quote_re) = Regex::new(r"「([^」]+)」") {
        if let Some(captures) = quote_re.captures(&cleaned) {
            if let Some(quoted_title) = captures.get(1) {
                let extracted = quoted_title.as_str().trim();
                if !extracted.is_empty() && extracted.len() > 2 {
                    debug!("Extracted from 「」quotes: '{}'", extracted);
                    return extracted.to_string();
                }
            }
        }
    }

    // Try Japanese brackets 【】- look for content between different bracket pairs
    // Pattern: 【something】actual_title【something_else】
    if let Ok(bracket_re) = Regex::new(r"【[^】]*】([^【】]+)【[^】]*】") {
        if let Some(captures) = bracket_re.captures(&cleaned) {
            if let Some(middle_content) = captures.get(1) {
                let mut extracted = middle_content.as_str().trim().to_string();
                if !extracted.is_empty() && extracted.len() > 1 {
                    // Apply "/" cleaning rule to the extracted content
                    if let Ok(separator_re) = Regex::new(r"[/-｜].*$") {
                        let cleaned_extracted = separator_re.replace_all(&extracted, "").to_string().trim().to_string();
                        if !cleaned_extracted.is_empty() && cleaned_extracted.len() > 1 {
                            extracted = cleaned_extracted;
                        }
                    }
                    debug!("Extracted from 【】 middle content: '{}'", extracted);
                    return extracted;
                }
            }
        }
    }
    
    // Try Western brackets []
    if let Ok(bracket_re) = Regex::new(r"\[([^\]]+)\]") {
        if let Some(captures) = bracket_re.captures(&cleaned) {
            if let Some(bracketed_title) = captures.get(1) {
                let mut extracted = bracketed_title.as_str().trim().to_string();
                if !extracted.is_empty()
                    && extracted.len() > 2
                    && !extracted.to_lowercase().contains("cover")
                {
                    // Apply "/" cleaning rule to the extracted content
                    if let Ok(separator_re) = Regex::new(r"[/-｜].*$") {
                        let cleaned_extracted = separator_re.replace_all(&extracted, "").to_string().trim().to_string();
                        if !cleaned_extracted.is_empty() && cleaned_extracted.len() > 1 {
                            extracted = cleaned_extracted;
                        }
                    }
                    debug!("Extracted from []brackets: '{}'", extracted);
                    return extracted;
                }
            }
        }
    }

    // Try to extract the first part before / if it looks like a song title
    if let Ok(slash_re) = Regex::new(r"^([^/]+)") {
        if let Some(captures) = slash_re.captures(&cleaned) {
            if let Some(first_part) = captures.get(1) {
                let extracted = first_part.as_str().trim();
                // Only use if it's not just the start of a longer title
                if !extracted.is_empty() && extracted.len() > 3 && !extracted.ends_with("の") {
                    // Remove common prefixes/suffixes that indicate it's not the main title
                    let cleaned_part = extracted
                        .replace("【", "")
                        .replace("】", "")
                        .trim()
                        .to_string();
                    if !cleaned_part.is_empty() {
                        debug!("Extracted first part before /: '{}'", cleaned_part);
                        return cleaned_part;
                    }
                }
            }
        }
    }

    // Remove common YouTube Music additions and track name cleaners
    // First apply all specific pattern cleaning
    let cleanup_patterns = vec![
        // Remove Japanese/Unicode brackets
        r"【.*?】",
        // Remove YouTube suffixes
        r" - YouTube Music$",
        r" - YouTube$",
        // Remove common video indicators
        r"\s*\(.*?MV.*?\)",
        r"\s*\[.*?MV.*?\]",
        r"\s*\(.*?Official.*?Video.*?\)",
        r"\s*\[.*?Official.*?Video.*?\]",
        r"\s*\(.*?Official.*?Music.*?Video.*?\)",
        r"\s*\[.*?Official.*?Music.*?Video.*?\]",
        r"\s*\(.*?Audio.*?\)",
        r"\s*\[.*?Audio.*?\]",
        r"\s*\(.*?Lyric.*?Video.*?\)",
        r"\s*\[.*?Lyric.*?Video.*?\]",
        // Remove translations and language indicators
        r"\s*\(.*?中文.*?\)",
        r"\s*\[.*?中文.*?\]",
        r"\s*\(.*?日本語.*?\)",
        r"\s*\[.*?日本語.*?\]",
        r"\s*\(.*?한국어.*?\)",
        r"\s*\[.*?한국어.*?\]",
        // Remove featuring
        r"\s*\(.*?feat\..*?\)",
        r"\s*\[.*?feat\..*?\]",
        r"\s*\(.*?ft\..*?\)",
        r"\s*\[.*?ft\..*?\]",
        r"\s*\(.*?featuring.*?\)",
        r"\s*\[.*?featuring.*?\]",
        // Remove remix/version
        r"\s*\(.*?remix.*?\)",
        r"\s*\[.*?remix.*?\]",
        r"\s*\(.*?version.*?\)",
        r"\s*\[.*?version.*?\]",
        r"\s*-\s*remaster.*$",
        // Remove live
        r"\s*\(.*?Live.*?\)",
        r"\s*\[.*?Live.*?\]",
        // Remove cover mentions
        r"\s*\([Cc]over\)\s*$",
        r".*\s*[/-]\s*.*\s*[/-]\s*.*\([Cc]over\)\s*$",
    ];

    // Apply all cleanup patterns first, but preserve non-empty results
    for pattern in cleanup_patterns {
        if let Ok(re) = Regex::new(&format!("(?i){}", pattern)) {
            let potential_result = re.replace_all(&cleaned, "").to_string().trim().to_string();
            // Only apply the cleaning if it doesn't make the string empty or too short
            if !potential_result.is_empty() && potential_result.len() > 1 {
                cleaned = potential_result;
            }
        }
    }

    // Finally, remove everything after first separator (/, -, ｜) - this should be last
    // But only if the result wouldn't be empty
    if let Ok(re) = Regex::new(r"[/-｜].*$") {
        let potential_result = re.replace_all(&cleaned, "").to_string().trim().to_string();
        if !potential_result.is_empty() && potential_result.len() > 2 {
            cleaned = potential_result;
        }
    }

    // Clean up multiple spaces
    if let Ok(re) = Regex::new(r"\s+") {
        cleaned = re.replace_all(&cleaned, " ").trim().to_string();
    }

    debug!("Cleaned result: '{}'", cleaned);
    cleaned
}

fn remove_artist_from_track(track_name: &str, artist_name: &str) -> String {
    if artist_name.trim().is_empty() {
        return track_name.to_string();
    }

    use regex::Regex;

    // Create pattern to match artist name at the beginning with separators
    let escaped_artist = regex::escape(artist_name);
    let patterns = vec![
        format!(r"^{}\s*[-–—]\s*", escaped_artist),
        format!(r"\s*[-–—]\s*{}$", escaped_artist),
    ];

    let mut result = track_name.to_string();

    for pattern in patterns {
        if let Ok(re) = Regex::new(&format!("(?i){}", pattern)) {
            let new_result = re.replace_all(&result, "").trim().to_string();
            if new_result.len() < result.len() && !new_result.is_empty() {
                result = new_result;
            }
        }
    }

    result
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
            let centiseconds: f64 = caps
                .get(3)
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
                error!("Failed to emit track-updated event: {}", e);
            }
        }

        // Always emit playback state
        if let Err(e) = app_handle_clone.emit("playback-state", &track_update.is_playing) {
            error!("Failed to emit playback-state event: {}", e);
        }

        // Emit time updates if we have timing info
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
            error!("WebSocket server error: {}", e);
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
async fn get_websocket_clients_count(ws_state: State<'_, WebSocketState>) -> Result<usize, String> {
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
async fn debug_websocket_server(ws_state: State<'_, WebSocketState>) -> Result<String, String> {
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
async fn control_playback(action: String, seek_time: Option<f64>) -> Result<String, String> {
    info!("Playback control: {} {:?}", action, seek_time);

    // For now, return success - we'll implement browser control later
    // This would need to send messages to the extension to control the browser
    Ok(format!("Playback control '{}' executed", action))
}

#[tauri::command]
async fn send_playback_command(
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize logger
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();
    
    info!("Starting Lyryc application...");
    
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
            get_websocket_clients_count,
            debug_websocket_server,
            control_playback,
            send_playback_command
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
