use crate::track_cleaning::{clean_track_name, remove_artist_from_track, clean_artist_and_track_names};
use crate::types::LyricLine;
use anyhow::{anyhow, Result};
use log::{debug, info, warn};
use reqwest::Client;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::{task::JoinSet, time::timeout};
use tokio_util::sync::CancellationToken;

// Priority constants for clear hierarchy - CLEANED names should have HIGHER priority
const PRIORITY_EXACT_CLEANED: u8 = 10;          // Highest - cleaned track + cleaned artist
const PRIORITY_EXACT_ORIGINAL: u8 = 9;          // Original track + cleaned artist
const PRIORITY_WILDCARD_CLEANED_WITH_ARTIST: u8 = 8;   // Wildcard cleaned + artist
const PRIORITY_WILDCARD_WITH_ARTIST: u8 = 7;           // Wildcard original + artist
const PRIORITY_EXACT_CLEANED_WITHOUT_ARTIST: u8 = 6;   // Cleaned without artist prefix
const PRIORITY_EXACT_WITHOUT_ARTIST: u8 = 5;           // Original without artist prefix
const PRIORITY_WILDCARD_CLEANED_ONLY: u8 = 4;          // Wildcard cleaned only
const PRIORITY_WILDCARD_ORIGINAL_ONLY: u8 = 3;         // Wildcard original only

// Configuration constants
const MAX_CANDIDATES_PER_STAGE: usize = 4;
const PER_REQUEST_TIMEOUT_SECS: u64 = 10;
const STAGE_TIMEOUT_SECS: u64 = 15;

// Shared HTTP client instance for lyrics fetching
static LYRICS_HTTP_CLIENT: OnceLock<Arc<Client>> = OnceLock::new();

/// Get shared HTTP client with connection pooling for lyrics
fn get_lyrics_http_client() -> &'static Arc<Client> {
    LYRICS_HTTP_CLIENT.get_or_init(|| {
        Arc::new(
            Client::builder()
                .timeout(Duration::from_secs(30))
                .pool_max_idle_per_host(15)
                .pool_idle_timeout(Duration::from_secs(90))
                .build()
                .expect("Failed to create lyrics HTTP client"),
        )
    })
}

#[derive(Clone, Debug)]
struct LyricsCandidate {
    priority: u8, // Higher number = higher priority
    url: String,
    description: String,
}

#[derive(Clone, Debug)]
struct SearchParameters {
    track_name: String,
    artist_name: String,
    cleaned_track: String,
    cleaned_artist: String,
    cleaned_artist_primary: String, // First artist from cleaned_artists array
    track_without_artist: String,
    cleaned_track_without_artist: String,
}

impl SearchParameters {
    fn new(track_name: String, artist_name: String) -> Self {
        Self {
            track_name: track_name.clone(),
            artist_name: artist_name.clone(),
            cleaned_track: String::new(), // Will be filled async
            cleaned_artist: String::new(), // Will be filled async
            cleaned_artist_primary: String::new(), // Will be filled async
            track_without_artist: String::new(), // Will be filled async
            cleaned_track_without_artist: String::new(), // Will be filled async
        }
    }

    async fn populate_cleaned_variants(&mut self) {
        // Clean both artist and track simultaneously
        match clean_artist_and_track_names(&self.artist_name, &self.track_name).await {
            Ok(result) => {
                self.cleaned_artist = result.artist.cleaned_title.clone();
                // Use the primary artist (first in array) for search
                self.cleaned_artist_primary = result.artist.cleaned_artists
                    .first()
                    .cloned()
                    .unwrap_or_else(|| result.artist.cleaned_title);

                // Check if track cleaning actually worked in combined cleaning
                let combined_cleaned_track = result.track.cleaned_title.clone();
                if combined_cleaned_track != self.track_name {
                    // Combined cleaning worked for track
                    self.cleaned_track = combined_cleaned_track;
                    debug!(
                        "Combined artist/track cleaning successful: '{}' -> '{}' (primary: '{}'), '{}' -> '{}'",
                        self.artist_name, self.cleaned_artist, self.cleaned_artist_primary,
                        self.track_name, self.cleaned_track
                    );
                } else {
                    // Combined cleaning didn't clean the track, fall back to track-only cleaning
                    warn!(
                        "Combined cleaning didn't clean track '{}', falling back to track-only cleaning",
                        self.track_name
                    );
                    self.cleaned_track = clean_track_name(&self.track_name).await;
                    debug!(
                        "Track-only fallback: '{}' -> '{}', artist: '{}' -> '{}' (primary: '{}')",
                        self.track_name, self.cleaned_track,
                        self.artist_name, self.cleaned_artist, self.cleaned_artist_primary
                    );
                }
            }
            Err(e) => {
                warn!("Failed to clean artist/track with LLM: {}, falling back to separate cleaning", e);
                // Fallback to separate cleaning
                self.cleaned_track = clean_track_name(&self.track_name).await;
                self.cleaned_artist = self.artist_name.clone();
                self.cleaned_artist_primary = self.artist_name.clone();

                debug!(
                    "Using separate cleaning fallback: artist '{}' (unchanged), track '{}' -> '{}'",
                    self.artist_name, self.track_name, self.cleaned_track
                );
            }
        }

        self.track_without_artist = remove_artist_from_track(&self.track_name, &self.artist_name);
        self.cleaned_track_without_artist = clean_track_name(&self.track_without_artist).await;
    }
}

async fn fetch_lyrics_from_url(
    client: Arc<Client>,
    candidate: LyricsCandidate,
    token: CancellationToken,
    per_req_timeout: Duration,
) -> Result<(LyricsCandidate, Vec<LyricLine>)> {
    let req_fut = async {
        let resp = client
            .get(&candidate.url)
            .header("User-Agent", "Lyryc/0.1.0")
            .timeout(per_req_timeout)
            .send()
            .await?;
        let resp = resp.error_for_status()?;
        let json: Vec<serde_json::Value> = resp.json().await?;

        debug!(
            "Found {} search results for {}",
            json.len(),
            candidate.description
        );

        let lyrics = if json.is_empty() {
            Vec::new()
        } else {
            // Find the best result (prefer synced lyrics)
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
                .or(json.first());

            match track_data {
                Some(data) => {
                    // Check for synced lyrics first
                    if let Some(synced_lyrics) = data["syncedLyrics"].as_str() {
                        if !synced_lyrics.trim().is_empty() {
                            info!(
                                "Found synced lyrics for {}, parsing LRC format",
                                candidate.description
                            );
                            parse_lrc_format(synced_lyrics)
                        } else if let Some(plain_lyrics) = data["plainLyrics"].as_str() {
                            if !plain_lyrics.trim().is_empty() {
                                info!(
                                    "Found plain lyrics for {}, converting to unsynced format",
                                    candidate.description
                                );
                                convert_plain_lyrics_to_lines(plain_lyrics)
                            } else {
                                Vec::new()
                            }
                        } else {
                            Vec::new()
                        }
                    } else if let Some(plain_lyrics) = data["plainLyrics"].as_str() {
                        if !plain_lyrics.trim().is_empty() {
                            info!(
                                "Found plain lyrics for {}, converting to unsynced format",
                                candidate.description
                            );
                            convert_plain_lyrics_to_lines(plain_lyrics)
                        } else {
                            Vec::new()
                        }
                    } else {
                        Vec::new()
                    }
                }
                None => Vec::new(),
            }
        };

        Ok::<_, anyhow::Error>((candidate, lyrics))
    };

    tokio::select! {
        _ = token.cancelled() => Err(anyhow!("Request cancelled")),
        res = req_fut => res,
    }
}

#[tauri::command]
pub async fn fetch_lyrics(
    track_name: String,
    artist_name: String,
    _request_id: Option<String>, // Keep for compatibility
) -> Result<Vec<LyricLine>, String> {
    // Convert single artist name to comma-separated format for search
    let search_artist = if artist_name.contains(',') {
        artist_name.clone() // Already comma-separated
    } else {
        artist_name.clone() // Single artist
    };

    info!("Fetching lyrics for: {} by {}", track_name, search_artist);

    // Prepare search parameters with all variants
    let mut params = SearchParameters::new(track_name.clone(), search_artist.clone());
    params.populate_cleaned_variants().await;

    // Build all search candidates with proper priority
    let candidates = build_search_candidates(&params)?;

    if candidates.is_empty() {
        return Err("No valid search candidates".to_string());
    }

    // Execute staged priority search
    match execute_staged_search(candidates).await {
        Ok((winner, lyrics)) => {
            info!("Success with strategy: {}", winner.description);
            Ok(lyrics)
        }
        Err(e) => {
            warn!(
                "No lyrics found after trying all strategies for '{}' by '{}': {}",
                track_name, artist_name, e
            );
            Err(format!("No lyrics found: {}", e))
        }
    }
}

/// Build all search candidates with proper priority and error handling
fn build_search_candidates(params: &SearchParameters) -> Result<Vec<LyricsCandidate>, String> {
    let mut candidates = Vec::new();

    // Stage 1: Exact searches (Priority 9-10) - Most precise, use cleaned artist names
    add_exact_search_candidate(
        &mut candidates,
        &params.cleaned_track,
        &params.cleaned_artist_primary,
        PRIORITY_EXACT_CLEANED,
        "exact cleaned track + primary artist",
    );

    add_exact_search_candidate(
        &mut candidates,
        &params.track_name,
        &params.cleaned_artist_primary,
        PRIORITY_EXACT_ORIGINAL,
        "exact track + cleaned artist",
    );

    // Stage 2: Wildcard with cleaned artist (Priority 7-8) - Good balance
    add_wildcard_search_candidate(
        &mut candidates,
        &params.cleaned_track,
        &params.cleaned_artist_primary,
        PRIORITY_WILDCARD_CLEANED_WITH_ARTIST,
        "wildcard cleaned track + primary artist",
    );

    add_wildcard_search_candidate(
        &mut candidates,
        &params.track_name,
        &params.cleaned_artist_primary,
        PRIORITY_WILDCARD_WITH_ARTIST,
        "wildcard track + cleaned artist",
    );

    // Stage 3: Exact without artist prefix (Priority 5-6) - Handle artist prefix removal
    add_exact_search_candidate(
        &mut candidates,
        &params.cleaned_track_without_artist,
        &params.cleaned_artist_primary,
        PRIORITY_EXACT_CLEANED_WITHOUT_ARTIST,
        "exact cleaned without artist prefix + primary artist",
    );

    add_exact_search_candidate(
        &mut candidates,
        &params.track_without_artist,
        &params.cleaned_artist_primary,
        PRIORITY_EXACT_WITHOUT_ARTIST,
        "exact without artist prefix + cleaned artist",
    );

    // Stage 4: Wildcard only track (Priority 3-4) - Last resort
    add_wildcard_only_candidate(
        &mut candidates,
        &params.cleaned_track,
        PRIORITY_WILDCARD_CLEANED_ONLY,
        "wildcard cleaned only",
    );

    add_wildcard_only_candidate(
        &mut candidates,
        &params.track_name,
        PRIORITY_WILDCARD_ORIGINAL_ONLY,
        "wildcard original only",
    );

    Ok(candidates)
}

/// Add exact search candidate (track_name + artist_name parameters)
fn add_exact_search_candidate(
    candidates: &mut Vec<LyricsCandidate>,
    track: &str,
    artist: &str,
    priority: u8,
    description_prefix: &str,
) {
    if !track.trim().is_empty() && !artist.trim().is_empty() {
        let url = format!(
            "https://lrclib.net/api/search?track_name={}&artist_name={}",
            urlencoding::encode(track),
            urlencoding::encode(artist)
        );
        candidates.push(LyricsCandidate {
            priority,
            url,
            description: format!("{}: '{}' by '{}'", description_prefix, track, artist),
        });
    }
}

/// Add wildcard search candidate (q parameter with track + artist)
fn add_wildcard_search_candidate(
    candidates: &mut Vec<LyricsCandidate>,
    track: &str,
    artist: &str,
    priority: u8,
    description_prefix: &str,
) {
    if !track.trim().is_empty() && !artist.trim().is_empty() {
        let url = format!(
            "https://lrclib.net/api/search?q={}",
            urlencoding::encode(&format!("{} {}", track, artist))
        );
        candidates.push(LyricsCandidate {
            priority,
            url,
            description: format!("{}: '{}' + '{}'", description_prefix, track, artist),
        });
    }
}

/// Add wildcard search candidate (q parameter with track only)
fn add_wildcard_only_candidate(
    candidates: &mut Vec<LyricsCandidate>,
    track: &str,
    priority: u8,
    description_prefix: &str,
) {
    if !track.trim().is_empty() {
        let url = format!(
            "https://lrclib.net/api/search?q={}",
            urlencoding::encode(track)
        );
        candidates.push(LyricsCandidate {
            priority,
            url,
            description: format!("{}: '{}'", description_prefix, track),
        });
    }
}

/// Execute search in priority stages - higher priority stages complete before lower ones
async fn execute_staged_search(
    mut candidates: Vec<LyricsCandidate>,
) -> Result<(LyricsCandidate, Vec<LyricLine>)> {
    // Sort candidates by priority (highest first)
    candidates.sort_by_key(|c| std::cmp::Reverse(c.priority));

    // Group candidates by priority for staged execution
    let mut stages: Vec<Vec<LyricsCandidate>> = Vec::new();
    let mut current_stage = Vec::new();
    let mut current_priority: Option<u8> = None;

    for candidate in candidates {
        match current_priority {
            Some(priority) if priority == candidate.priority => {
                current_stage.push(candidate);
            }
            _ => {
                if !current_stage.is_empty() {
                    stages.push(current_stage);
                }
                current_stage = vec![candidate.clone()];
                current_priority = Some(candidate.priority);
            }
        }
    }
    if !current_stage.is_empty() {
        stages.push(current_stage);
    }

    // Execute each stage in priority order
    for (stage_num, stage_candidates) in stages.into_iter().enumerate() {
        debug!(
            "Executing stage {} with {} candidates at priority {}",
            stage_num + 1,
            stage_candidates.len(),
            stage_candidates.first().map(|c| c.priority).unwrap_or(0)
        );

        // Limit candidates per stage to avoid excessive API calls
        let limited_candidates: Vec<_> = stage_candidates
            .into_iter()
            .take(MAX_CANDIDATES_PER_STAGE)
            .collect();

        match execute_concurrent_stage(limited_candidates).await {
            Ok(result) => {
                info!(
                    "Stage {} succeeded with: {}",
                    stage_num + 1,
                    result.0.description
                );
                return Ok(result);
            }
            Err(e) => {
                debug!("Stage {} failed: {}", stage_num + 1, e);
                // Continue to next stage
            }
        }
    }

    Err(anyhow!("All search stages failed"))
}

/// Execute all candidates in a stage concurrently, return first successful result
async fn execute_concurrent_stage(
    candidates: Vec<LyricsCandidate>,
) -> Result<(LyricsCandidate, Vec<LyricLine>)> {
    if candidates.is_empty() {
        return Err(anyhow!("No candidates in stage"));
    }

    let client = get_lyrics_http_client().clone();
    let cancel_all = CancellationToken::new();
    let mut set = JoinSet::new();

    // Start all candidates in this stage
    for candidate in candidates {
        let token = cancel_all.child_token();
        let client_clone = client.clone();

        set.spawn(fetch_lyrics_from_url(
            client_clone,
            candidate,
            token,
            Duration::from_secs(PER_REQUEST_TIMEOUT_SECS),
        ));
    }

    // Wait for first successful result within stage timeout
    let stage_timeout = Duration::from_secs(STAGE_TIMEOUT_SECS);
    let result = timeout(stage_timeout, async {
        while let Some(joined) = set.join_next().await {
            match joined {
                Ok(Ok((candidate, lyrics))) => {
                    if !lyrics.is_empty() {
                        // Found successful result - cancel remaining and return
                        cancel_all.cancel();
                        return Ok((candidate, lyrics));
                    }
                }
                Ok(Err(e)) => {
                    debug!("Candidate failed: {}", e);
                    // Continue with other candidates
                }
                Err(e) => {
                    debug!("Task join error: {}", e);
                    // Continue with other candidates
                }
            }
        }
        Err(anyhow!("No successful results in stage"))
    })
    .await;

    // Cleanup
    cancel_all.cancel();
    set.abort_all();

    match result {
        Ok(stage_result) => stage_result,
        Err(_) => Err(anyhow!("Stage timeout")),
    }
}

// Legacy function kept for compatibility - now just delegates to new staged approach
#[allow(dead_code)]
async fn search_strict_priority<F>(
    candidates: Vec<LyricsCandidate>,
    _per_req_timeout: Duration,
    _overall_timeout: Duration,
    _is_non_empty: F,
) -> Result<(LyricsCandidate, Vec<LyricLine>)>
where
    F: Fn(&[LyricLine]) -> bool + Copy + Send + 'static,
{
    // Delegate to new staged search implementation
    execute_staged_search(candidates).await
}

#[tauri::command]
pub async fn fetch_lrclib_raw(
    track_name: String,
    artist_name: String,
) -> Result<serde_json::Value, String> {
    info!("Fetching raw LRCLib for: {} by {}", track_name, artist_name);

    let client = Client::builder().build().map_err(|e| e.to_string())?;
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

    if let Ok(resp) = client
        .get(&url)
        .header("User-Agent", "Lyryc/0.1.0")
        .timeout(Duration::from_secs(10))
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
            .timeout(Duration::from_secs(10))
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
