use crate::track_cleaning::{clean_track_name, remove_artist_from_track};
use crate::types::LyricLine;
use log::{debug, info, warn};

#[tauri::command]
pub async fn fetch_lyrics(
    track_name: String,
    artist_name: String,
) -> Result<Vec<LyricLine>, String> {
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
        // Wildcard searches (often most effective) - try original first, then cleaned
        SearchStrategy::Wildcard(format!("{} {}", track_name, artist_name)),
        SearchStrategy::Wildcard(track_name.clone()),
        SearchStrategy::Wildcard(format!("{} {}", cleaned_track, artist_name)),
        SearchStrategy::Wildcard(cleaned_track.clone()),
        // Exact searches - try original first, then cleaned
        SearchStrategy::Exact(track_name.clone(), artist_name.clone()),
        SearchStrategy::Exact(track_without_artist.clone(), artist_name.clone()),
        SearchStrategy::Exact(cleaned_track.clone(), artist_name.clone()),
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

    warn!(
        "No lyrics found after trying all strategies for '{}' by '{}'",
        track_name, artist_name
    );
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

#[tauri::command]
pub async fn fetch_lrclib_raw(
    track_name: String,
    artist_name: String,
) -> Result<serde_json::Value, String> {
    info!("Fetching raw LRCLib for: {} by {}", track_name, artist_name);

    // Try both exact and wildcard strategies and pick the first with any lyrics
    let mut candidates: Vec<serde_json::Value> = Vec::new();

    // Exact search
    let mut url = format!(
        "https://lrclib.net/api/search?track_name={}",
        urlencoding::encode(&track_name)
    );
    if !artist_name.trim().is_empty() {
        url.push_str(&format!(
            "&artist_name={}",
            urlencoding::encode(&artist_name)
        ));
    }
    let client = reqwest::Client::new();
    if let Ok(resp) = client
        .get(&url)
        .header("User-Agent", "Lyryc/0.1.0")
        .send()
        .await
    {
        if resp.status().is_success() {
            if let Ok(json) = resp.json::<Vec<serde_json::Value>>().await {
                candidates.extend(json);
            }
        }
    }

    // Wildcard search as fallback
    if candidates.is_empty() {
        let url = format!(
            "https://lrclib.net/api/search?q={}",
            urlencoding::encode(&format!("{} {}", track_name, artist_name))
        );
        if let Ok(resp) = client
            .get(&url)
            .header("User-Agent", "Lyryc/0.1.0")
            .send()
            .await
        {
            if resp.status().is_success() {
                if let Ok(json) = resp.json::<Vec<serde_json::Value>>().await {
                    candidates.extend(json);
                }
            }
        }
    }

    if candidates.is_empty() {
        return Err("No LRCLib results".into());
    }

    // Prefer entries with syncedLyrics but return whatever available
    let chosen = candidates
        .iter()
        .find(|item| {
            item.get("syncedLyrics")
                .and_then(|v| v.as_str())
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false)
        })
        .cloned()
        .or_else(|| candidates.get(0).cloned())
        .ok_or_else(|| "No valid result".to_string())?;

    // Shape a compact JSON with relevant fields
    let track = serde_json::json!({
        "trackName": chosen.get("trackName").and_then(|v| v.as_str()).unwrap_or(""),
        "artistName": chosen.get("artistName").and_then(|v| v.as_str()).unwrap_or(""),
        "albumName": chosen.get("albumName").and_then(|v| v.as_str()).unwrap_or(""),
        "duration": chosen.get("duration").and_then(|v| v.as_f64()).unwrap_or(0.0),
        "plainLyrics": chosen.get("plainLyrics").and_then(|v| v.as_str()).unwrap_or("") ,
        "syncedLyrics": chosen.get("syncedLyrics").and_then(|v| v.as_str()).unwrap_or("") ,
    });

    Ok(track)
}

pub fn parse_lrc_format(lrc_content: &str) -> Vec<LyricLine> {
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

pub fn convert_plain_lyrics_to_lines(plain_lyrics: &str) -> Vec<LyricLine> {
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
