use crate::track_cleaning::{clean_track_name, remove_artist_from_track};
use crate::types::LyricLine;
use anyhow::{anyhow, Result};
use log::{debug, info, warn};
use reqwest::Client;
use std::time::Duration;
use tokio::{task::JoinSet, time::timeout};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug)]
struct LyricsCandidate {
    priority: u8, // Higher number = higher priority
    url: String,
    description: String,
}

async fn fetch_lyrics_from_url(
    client: Client,
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

        debug!("Found {} search results for {}", json.len(), candidate.description);

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
                            info!("Found synced lyrics for {}, parsing LRC format", candidate.description);
                            parse_lrc_format(synced_lyrics)
                        } else if let Some(plain_lyrics) = data["plainLyrics"].as_str() {
                            if !plain_lyrics.trim().is_empty() {
                                info!("Found plain lyrics for {}, converting to unsynced format", candidate.description);
                                convert_plain_lyrics_to_lines(plain_lyrics)
                            } else {
                                Vec::new()
                            }
                        } else {
                            Vec::new()
                        }
                    } else if let Some(plain_lyrics) = data["plainLyrics"].as_str() {
                        if !plain_lyrics.trim().is_empty() {
                            info!("Found plain lyrics for {}, converting to unsynced format", candidate.description);
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
    _request_id: Option<String>, // Keep for compatibility but not needed with this approach
) -> Result<Vec<LyricLine>, String> {
    info!("Fetching lyrics for: {} by {}", track_name, artist_name);

    let cleaned_track = clean_track_name(&track_name);
    let track_without_artist = remove_artist_from_track(&track_name, &artist_name);
    let cleaned_track_without_artist = clean_track_name(&track_without_artist);

    // Build candidates with priority (higher = more likely to succeed)
    let mut candidates = Vec::new();

    // Priority 10: Wildcard searches (often most effective)
    if !track_name.trim().is_empty() && !artist_name.trim().is_empty() {
        candidates.push(LyricsCandidate {
            priority: 10,
            url: format!(
                "https://lrclib.net/api/search?q={}",
                urlencoding::encode(&format!("{} {}", track_name, artist_name))
            ),
            description: format!("wildcard: '{}' '{}'", track_name, artist_name),
        });
    }

    if !track_name.trim().is_empty() {
        candidates.push(LyricsCandidate {
            priority: 9,
            url: format!(
                "https://lrclib.net/api/search?q={}",
                urlencoding::encode(&track_name)
            ),
            description: format!("wildcard: '{}'", track_name),
        });
    }

    if !cleaned_track.trim().is_empty() && !artist_name.trim().is_empty() {
        candidates.push(LyricsCandidate {
            priority: 8,
            url: format!(
                "https://lrclib.net/api/search?q={}",
                urlencoding::encode(&format!("{} {}", cleaned_track, artist_name))
            ),
            description: format!("wildcard cleaned: '{}' '{}'", cleaned_track, artist_name),
        });
    }

    if !cleaned_track.trim().is_empty() {
        candidates.push(LyricsCandidate {
            priority: 7,
            url: format!(
                "https://lrclib.net/api/search?q={}",
                urlencoding::encode(&cleaned_track)
            ),
            description: format!("wildcard cleaned: '{}'", cleaned_track),
        });
    }

    // Priority 6-3: Exact searches
    if !track_name.trim().is_empty() && !artist_name.trim().is_empty() {
        let mut url = format!(
            "https://lrclib.net/api/search?track_name={}",
            urlencoding::encode(&track_name)
        );
        url.push_str(&format!(
            "&artist_name={}",
            urlencoding::encode(&artist_name)
        ));
        candidates.push(LyricsCandidate {
            priority: 6,
            url,
            description: format!("exact: '{}' by '{}'", track_name, artist_name),
        });
    }

    if !track_without_artist.trim().is_empty() && !artist_name.trim().is_empty() {
        let mut url = format!(
            "https://lrclib.net/api/search?track_name={}",
            urlencoding::encode(&track_without_artist)
        );
        url.push_str(&format!(
            "&artist_name={}",
            urlencoding::encode(&artist_name)
        ));
        candidates.push(LyricsCandidate {
            priority: 5,
            url,
            description: format!("exact without artist: '{}' by '{}'", track_without_artist, artist_name),
        });
    }

    if !cleaned_track.trim().is_empty() && !artist_name.trim().is_empty() {
        let mut url = format!(
            "https://lrclib.net/api/search?track_name={}",
            urlencoding::encode(&cleaned_track)
        );
        url.push_str(&format!(
            "&artist_name={}",
            urlencoding::encode(&artist_name)
        ));
        candidates.push(LyricsCandidate {
            priority: 4,
            url,
            description: format!("exact cleaned: '{}' by '{}'", cleaned_track, artist_name),
        });
    }

    if !cleaned_track_without_artist.trim().is_empty() && !artist_name.trim().is_empty() {
        let mut url = format!(
            "https://lrclib.net/api/search?track_name={}",
            urlencoding::encode(&cleaned_track_without_artist)
        );
        url.push_str(&format!(
            "&artist_name={}",
            urlencoding::encode(&artist_name)
        ));
        candidates.push(LyricsCandidate {
            priority: 3,
            url,
            description: format!("exact cleaned without artist: '{}' by '{}'", cleaned_track_without_artist, artist_name),
        });
    }

    if candidates.is_empty() {
        return Err("No valid search candidates".to_string());
    }

    match search_strict_priority(
        candidates,
        Duration::from_secs(10), // per-request timeout
        Duration::from_secs(30), // overall timeout
        |lyrics: &[LyricLine]| !lyrics.is_empty(),
    ).await {
        Ok((winner, lyrics)) => {
            info!("Success with strategy: {}", winner.description);
            Ok(lyrics)
        }
        Err(e) => {
            warn!("No lyrics found after trying all strategies for '{}' by '{}': {}", track_name, artist_name, e);
            Err(format!("No lyrics found: {}", e))
        }
    }
}

async fn search_strict_priority<F>(
    mut candidates: Vec<LyricsCandidate>,
    per_req_timeout: Duration,
    overall_timeout: Duration,
    is_non_empty: F,
) -> Result<(LyricsCandidate, Vec<LyricLine>)>
where
    F: Fn(&[LyricLine]) -> bool + Copy + Send + 'static,
{
    // Sort by priority (high to low)
    candidates.sort_by_key(|c| std::cmp::Reverse(c.priority));
    let top_priority = candidates.first().map(|c| c.priority).unwrap_or(0);

    let client = Client::builder().build()?;
    let cancel_all = CancellationToken::new();

    let mut set = JoinSet::new();
    let mut per_task_tokens = Vec::with_capacity(candidates.len());

    for candidate in candidates.clone() {
        let token = cancel_all.child_token();
        per_task_tokens.push((candidate.priority, token.clone()));

        let client_clone = client.clone();
        set.spawn(fetch_lyrics_from_url(client_clone, candidate, token, per_req_timeout));
    }

    let mut best: Option<(LyricsCandidate, Vec<LyricLine>)> = None;
    let mut best_priority: u8 = 0;

    let result = timeout(overall_timeout, async {
        while let Some(joined) = set.join_next().await {
            match joined {
                Ok(Ok((candidate, lyrics))) => {
                    let non_empty = is_non_empty(&lyrics);

                    if non_empty {
                        // Found non-empty result with higher priority
                        if best.is_none() || candidate.priority > best_priority {
                            best_priority = candidate.priority;
                            best = Some((candidate.clone(), lyrics));

                            // If this is the highest priority, we're done
                            if best_priority == top_priority {
                                break;
                            }

                            // Cancel all lower priority tasks
                            for (pri, token) in &per_task_tokens {
                                if *pri < best_priority {
                                    token.cancel();
                                }
                            }
                        }
                    }
                }
                Ok(Err(_e)) => {
                    // HTTP/parsing error - ignore this candidate
                    continue;
                }
                Err(_e) => {
                    // Task join error - ignore this candidate
                    continue;
                }
            }
        }
        Ok::<_, anyhow::Error>(())
    }).await;

    // Cleanup
    cancel_all.cancel();
    set.abort_all();

    match (result, best) {
        (Ok(_), Some(winner)) => Ok(winner),
        (Ok(_), None) => Err(anyhow!("All candidates returned empty")),
        (Err(_), Some(winner)) => Ok(winner), // Timeout but we have a result
        (Err(_), None) => Err(anyhow!("Overall timeout with no results")),
    }
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