use crate::cleaning::{
    clean_artist_and_track_names, clean_track_name, clean_track_name_sync, remove_artist_from_track,
};
use crate::lyrics_cache::{cache_lyrics, cache_no_lyrics, get_cached_lyrics, CacheResult};
use crate::types::LyricLine;
use anyhow::{anyhow, Result};
use log::{debug, info, warn};
use reqwest::Client;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::{task::JoinSet, time::timeout};
use tokio_util::sync::CancellationToken;

// Priority constants for clear hierarchy - CLEANED names should have HIGHER priority
const PRIORITY_EXACT_CLEANED: u8 = 10; // Highest - cleaned track + cleaned artist
const PRIORITY_EXACT_ORIGINAL: u8 = 9; // Original track + cleaned artist
const PRIORITY_WILDCARD_CLEANED_WITH_ARTIST: u8 = 8; // Wildcard cleaned + artist
const PRIORITY_WILDCARD_WITH_ARTIST: u8 = 7; // Wildcard original + artist
const PRIORITY_EXACT_CLEANED_WITHOUT_ARTIST: u8 = 6; // Cleaned without artist prefix
const PRIORITY_EXACT_WITHOUT_ARTIST: u8 = 5; // Original without artist prefix
const PRIORITY_WILDCARD_CLEANED_ONLY: u8 = 4; // Wildcard cleaned only
const PRIORITY_WILDCARD_ORIGINAL_ONLY: u8 = 3; // Wildcard original only

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
struct MatchContext {
    expected_tracks: Vec<String>,
    expected_artists: Vec<String>,
}

#[derive(Clone, Debug)]
struct LyricsCandidate {
    priority: u8, // Higher number = higher priority
    url: String,
    description: String,
    match_context: MatchContext,
}

#[derive(Clone, Debug)]
struct LyricsFetchResult {
    candidate: LyricsCandidate,
    lyrics: Vec<LyricLine>,
    score: f32,
}

#[derive(Clone, Debug)]
struct SearchParameters {
    track_name: String,
    artist_name: String,
    artist_variants: Vec<String>, // All artist name variants from LLM cleaning
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
            artist_variants: Vec::new(),           // Will be filled async
            cleaned_track: String::new(),          // Will be filled async
            cleaned_artist: String::new(),         // Will be filled async
            cleaned_artist_primary: String::new(), // Will be filled async
            track_without_artist: String::new(),   // Will be filled async
            cleaned_track_without_artist: String::new(), // Will be filled async
        }
    }

    /// Fast path: populate with regex cleaning only (instant, no LLM calls)
    fn populate_regex_only(&mut self) {
        // Use regex cleaning for track name
        self.cleaned_track = clean_track_name_sync(&self.track_name);

        // Use original artist name (no LLM cleaning)
        self.cleaned_artist = self.artist_name.clone();
        self.cleaned_artist_primary = self.artist_name.clone();
        self.artist_variants = vec![self.artist_name.clone()];

        // Track without artist prefix
        self.track_without_artist = remove_artist_from_track(&self.track_name, &self.artist_name);
        self.cleaned_track_without_artist = clean_track_name_sync(&self.track_without_artist);

        debug!(
            "Regex-only cleaning: '{}' -> '{}', artist: '{}'",
            self.track_name, self.cleaned_track, self.artist_name
        );
    }

    async fn populate_cleaned_variants(&mut self) {
        // Use LLM cleaning for artist and track names
        match clean_artist_and_track_names(&self.artist_name, &self.track_name).await {
            Ok(result) => {
                self.cleaned_artist = result.artist.cleaned_title.clone();
                // Use the primary artist (first in array) for search
                self.cleaned_artist_primary = result
                    .artist
                    .cleaned_artists
                    .first()
                    .cloned()
                    .unwrap_or_else(|| result.artist.cleaned_title.clone());

                // Build artist variants list from LLM cleaning
                self.artist_variants = Vec::new();

                // 1. Add LLM cleaned artists
                for artist in &result.artist.cleaned_artists {
                    if !self.artist_variants.contains(artist) {
                        self.artist_variants.push(artist.clone());
                    }
                }

                // 2. Add primary cleaned artist
                if !self.artist_variants.contains(&self.cleaned_artist_primary) {
                    self.artist_variants
                        .push(self.cleaned_artist_primary.clone());
                }

                // 3. Add original artist name as fallback
                if !self.artist_variants.contains(&self.artist_name) {
                    self.artist_variants.push(self.artist_name.clone());
                }

                // Check if track cleaning actually worked in combined cleaning
                let combined_cleaned_track = result.track.cleaned_title.clone();
                if combined_cleaned_track != self.track_name {
                    self.cleaned_track = combined_cleaned_track;
                    debug!(
                        "Artist/track cleaning: '{}' -> {} variants {:?}, '{}' -> '{}'",
                        self.artist_name,
                        self.artist_variants.len(),
                        self.artist_variants,
                        self.track_name,
                        self.cleaned_track
                    );
                } else {
                    // Combined cleaning didn't clean the track, fall back to track-only cleaning
                    warn!(
                        "Combined cleaning didn't clean track '{}', falling back to track-only cleaning",
                        self.track_name
                    );
                    self.cleaned_track = clean_track_name(&self.track_name).await;
                    debug!(
                        "Track-only fallback: '{}' -> '{}', artist: '{}' -> {} variants {:?}",
                        self.track_name,
                        self.cleaned_track,
                        self.artist_name,
                        self.artist_variants.len(),
                        self.artist_variants
                    );
                }
            }
            Err(e) => {
                warn!(
                    "Failed to clean artist/track with LLM: {}, using original names",
                    e
                );

                // Use original name only
                self.artist_variants = vec![self.artist_name.clone()];

                // Fallback to separate cleaning for track
                self.cleaned_track = clean_track_name(&self.track_name).await;
                self.cleaned_artist = self.artist_name.clone();
                self.cleaned_artist_primary = self.artist_name.clone();

                debug!(
                    "Using original fallback: artist '{}', track '{}' -> '{}'",
                    self.artist_name, self.track_name, self.cleaned_track
                );
            }
        }

        self.track_without_artist = remove_artist_from_track(&self.track_name, &self.artist_name);
        self.cleaned_track_without_artist = clean_track_name_sync(&self.track_without_artist);
    }
}

fn build_match_context(params: &SearchParameters) -> MatchContext {
    let mut expected_tracks = Vec::new();
    let track_candidates = vec![
        params.cleaned_track.as_str(),
        params.track_name.as_str(),
        params.cleaned_track_without_artist.as_str(),
        params.track_without_artist.as_str(),
    ];
    for track in track_candidates {
        if !track.trim().is_empty() && !expected_tracks.contains(&track.to_string()) {
            expected_tracks.push(track.to_string());
        }
    }

    let mut expected_artists = Vec::new();
    for artist in &params.artist_variants {
        if !artist.trim().is_empty() && !expected_artists.contains(artist) {
            expected_artists.push(artist.clone());
        }
    }

    MatchContext {
        expected_tracks,
        expected_artists,
    }
}

fn normalize_for_match(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last_space = false;
    for ch in text.to_lowercase().chars() {
        if ch.is_alphanumeric() {
            out.push(ch);
            last_space = false;
        } else if ch.is_whitespace() {
            if !last_space {
                out.push(' ');
                last_space = true;
            }
        } else if !last_space {
            out.push(' ');
            last_space = true;
        }
    }
    out.trim().to_string()
}

fn token_overlap_score(expected: &str, candidate: &str) -> f32 {
    let expected_tokens: Vec<&str> = expected.split_whitespace().collect();
    if expected_tokens.is_empty() {
        return 0.0;
    }
    let candidate_tokens: std::collections::HashSet<&str> = candidate.split_whitespace().collect();
    let mut matches = 0;
    for token in &expected_tokens {
        if candidate_tokens.contains(token) {
            matches += 1;
        }
    }
    matches as f32 / expected_tokens.len() as f32
}

fn string_match_score(expected: &str, candidate: &str, contains_score: f32) -> f32 {
    let expected_norm = normalize_for_match(expected);
    let candidate_norm = normalize_for_match(candidate);
    if expected_norm.is_empty() || candidate_norm.is_empty() {
        return 0.0;
    }
    if expected_norm == candidate_norm {
        return 1.0;
    }
    if candidate_norm.contains(&expected_norm) || expected_norm.contains(&candidate_norm) {
        return contains_score;
    }
    let overlap = token_overlap_score(&expected_norm, &candidate_norm);
    if overlap > 0.0 {
        return 0.4 + 0.5 * overlap;
    }
    0.0
}

fn has_cover_marker(text: &str) -> bool {
    let lowered = text.to_lowercase();
    lowered.contains("cover")
        || lowered.contains("karaoke")
        || lowered.contains("instrumental")
        || lowered.contains("remix")
}

fn score_result_match(context: &MatchContext, track: &str, artist: &str, has_synced: bool) -> f32 {
    let cleaned_track = crate::cleaning::clean_track_name_sync(track);

    let mut best_track: f32 = 0.0;
    for expected in &context.expected_tracks {
        let score_raw = string_match_score(expected, track, 0.85);
        let score_cleaned = string_match_score(expected, &cleaned_track, 0.85);
        best_track = best_track.max(score_raw.max(score_cleaned));
    }

    let mut best_artist: f32 = 0.0;
    for expected in &context.expected_artists {
        let score = string_match_score(expected, artist, 0.7);
        best_artist = best_artist.max(score);
    }

    let artist_weight = if context.expected_artists.is_empty() {
        0.0
    } else {
        0.25
    };
    let track_weight = 0.95 - artist_weight;
    let mut score: f32 = best_track * track_weight + best_artist * artist_weight;
    if has_synced {
        score += 0.05;
    }

    let expected_has_cover = context.expected_tracks.iter().any(|t| has_cover_marker(t));
    if !expected_has_cover {
        if has_cover_marker(track) {
            score -= 0.15;
        }
        if has_cover_marker(artist) {
            score -= 0.1;
        }
    }

    score.clamp(0.0, 1.0)
}

async fn fetch_lyrics_from_url(
    client: Arc<Client>,
    candidate: LyricsCandidate,
    token: CancellationToken,
    per_req_timeout: Duration,
) -> Result<LyricsFetchResult> {
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

        if json.is_empty() {
            return Ok::<_, anyhow::Error>(LyricsFetchResult {
                candidate,
                lyrics: Vec::new(),
                score: 0.0,
            });
        }

        let mut best_index: Option<usize> = None;
        let mut best_score = 0.0;
        let mut best_has_synced = false;

        for (index, data) in json.iter().enumerate() {
            let synced = data["syncedLyrics"]
                .as_str()
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false);
            let plain = data["plainLyrics"]
                .as_str()
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false);

            if !synced && !plain {
                continue;
            }

            let track = data["trackName"].as_str().unwrap_or("");
            let artist = data["artistName"].as_str().unwrap_or("");
            let score = score_result_match(&candidate.match_context, track, artist, synced);

            if best_index.is_none() || score > best_score {
                best_index = Some(index);
                best_score = score;
                best_has_synced = synced;
            }
        }

        let (lyrics, match_summary) = match best_index {
            Some(index) => {
                let data = &json[index];
                let track = data["trackName"].as_str().unwrap_or("");
                let artist = data["artistName"].as_str().unwrap_or("");
                let synced_lyrics = data["syncedLyrics"].as_str().unwrap_or("");
                let plain_lyrics = data["plainLyrics"].as_str().unwrap_or("");
                let lyrics = if best_has_synced && !synced_lyrics.trim().is_empty() {
                    info!(
                        "Found synced lyrics for {}, parsing LRC format",
                        candidate.description
                    );
                    parse_lrc_format(synced_lyrics)
                } else if !plain_lyrics.trim().is_empty() {
                    info!(
                        "Found plain lyrics for {}, converting to unsynced format",
                        candidate.description
                    );
                    convert_plain_lyrics_to_lines(plain_lyrics)
                } else {
                    Vec::new()
                };
                let summary = format!("'{}' by '{}' (score {:.2})", track, artist, best_score);
                (lyrics, summary)
            }
            None => (Vec::new(), "no lyric-bearing results".to_string()),
        };

        debug!(
            "Best match for {} -> {}",
            candidate.description, match_summary
        );

        Ok::<_, anyhow::Error>(LyricsFetchResult {
            candidate,
            lyrics,
            score: best_score,
        })
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
    // Check cache first - instant return if we've seen this song before
    match get_cached_lyrics(&artist_name, &track_name) {
        CacheResult::Hit(cached) => {
            info!("Using cached lyrics for: {} by {}", track_name, artist_name);
            return Ok(cached);
        }
        CacheResult::NegativeHit => {
            info!("Cached negative result for: {} by {} (no lyrics available)", track_name, artist_name);
            return Err("No lyrics found (cached)".to_string());
        }
        CacheResult::Miss => {
            // Continue to fetch
        }
    }

    // Convert single artist name to comma-separated format for search
    let search_artist = if artist_name.contains(',') {
        artist_name.clone() // Already comma-separated
    } else {
        artist_name.clone() // Single artist
    };

    info!("Fetching lyrics for: {} by {}", track_name, search_artist);

    // === PARALLEL APPROACH ===
    // Start LLM cleaning in background immediately
    let llm_track = track_name.clone();
    let llm_artist = search_artist.clone();
    let llm_handle = tokio::spawn(async move {
        let mut params = SearchParameters::new(llm_track, llm_artist);
        params.populate_cleaned_variants().await;
        params
    });

    // Regex cleaning for fast path (instant)
    let mut fast_params = SearchParameters::new(track_name.clone(), search_artist.clone());
    fast_params.populate_regex_only();

    // Build fast path candidates (only highest priority - stages 1-2)
    let fast_candidates = build_search_candidates(&fast_params)?;
    let fast_candidates: Vec<_> = fast_candidates
        .into_iter()
        .filter(|c| c.priority >= PRIORITY_EXACT_ORIGINAL) // Only priority 9-10
        .collect();

    // Try fast path with limited stages (runs in parallel with LLM cleaning)
    if !fast_candidates.is_empty() {
        debug!(
            "Trying fast path with {} high-priority candidates",
            fast_candidates.len()
        );

        if let Ok((winner, lyrics)) = execute_staged_search(fast_candidates).await {
            info!("Fast path success with: {}", winner.description);
            llm_handle.abort(); // Cancel LLM - we don't need it
            cache_lyrics(&artist_name, &track_name, lyrics.clone());
            return Ok(lyrics);
        }
        debug!("Fast path failed, waiting for LLM results");
    }

    // === LLM PATH ===
    // Fast path failed or was empty, use LLM-cleaned candidates
    // By now LLM should be done (it ran in parallel with fast path HTTP requests)
    let search_params = match llm_handle.await {
        Ok(llm_params) => llm_params,
        Err(e) => {
            // LLM task panicked - fallback to regex-cleaned params
            warn!("LLM cleaning task failed: {}. Falling back to regex params.", e);
            fast_params
        }
    };

    let full_candidates = build_search_candidates(&search_params)?;
    if full_candidates.is_empty() {
        cache_no_lyrics(&artist_name, &track_name);
        return Err("No valid search candidates".to_string());
    }

    match execute_staged_search(full_candidates).await {
        Ok((winner, lyrics)) => {
            info!("Search success with: {}", winner.description);
            cache_lyrics(&artist_name, &track_name, lyrics.clone());
            Ok(lyrics)
        }
        Err(e) => {
            warn!(
                "No lyrics found after trying all strategies for '{}' by '{}': {}",
                track_name, artist_name, e
            );
            cache_no_lyrics(&artist_name, &track_name);
            Err(format!("No lyrics found: {}", e))
        }
    }
}

/// Build all search candidates with proper priority and error handling
fn build_search_candidates(params: &SearchParameters) -> Result<Vec<LyricsCandidate>, String> {
    let mut candidates = Vec::new();
    let match_context = build_match_context(params);

    // Try all artist variants at each priority level
    // Priority order: LLM cleaned > Original
    for (variant_idx, artist_variant) in params.artist_variants.iter().enumerate() {
        let variant_desc = if variant_idx == 0 {
            "primary"
        } else if variant_idx < params.artist_variants.len() - 1 {
            "variant"
        } else {
            "original"
        };

        // Stage 1: Exact searches (Priority 9-10) - Most precise
        add_exact_search_candidate(
            &mut candidates,
            &params.cleaned_track,
            artist_variant,
            PRIORITY_EXACT_CLEANED,
            &match_context,
            &format!(
                "exact cleaned track + {} artist '{}'",
                variant_desc, artist_variant
            ),
        );

        add_exact_search_candidate(
            &mut candidates,
            &params.track_name,
            artist_variant,
            PRIORITY_EXACT_ORIGINAL,
            &match_context,
            &format!("exact track + {} artist '{}'", variant_desc, artist_variant),
        );

        // Stage 2: Wildcard with artist (Priority 7-8) - Good balance
        add_wildcard_search_candidate(
            &mut candidates,
            &params.cleaned_track,
            artist_variant,
            PRIORITY_WILDCARD_CLEANED_WITH_ARTIST,
            &match_context,
            &format!(
                "wildcard cleaned track + {} artist '{}'",
                variant_desc, artist_variant
            ),
        );

        add_wildcard_search_candidate(
            &mut candidates,
            &params.track_name,
            artist_variant,
            PRIORITY_WILDCARD_WITH_ARTIST,
            &match_context,
            &format!(
                "wildcard track + {} artist '{}'",
                variant_desc, artist_variant
            ),
        );

        // Stage 3: Exact without artist prefix (Priority 5-6)
        add_exact_search_candidate(
            &mut candidates,
            &params.cleaned_track_without_artist,
            artist_variant,
            PRIORITY_EXACT_CLEANED_WITHOUT_ARTIST,
            &match_context,
            &format!(
                "exact cleaned without prefix + {} artist '{}'",
                variant_desc, artist_variant
            ),
        );

        add_exact_search_candidate(
            &mut candidates,
            &params.track_without_artist,
            artist_variant,
            PRIORITY_EXACT_WITHOUT_ARTIST,
            &match_context,
            &format!(
                "exact without prefix + {} artist '{}'",
                variant_desc, artist_variant
            ),
        );
    }

    // Stage 4: Wildcard only track (Priority 3-4) - Last resort, no artist filter
    add_wildcard_only_candidate(
        &mut candidates,
        &params.cleaned_track,
        PRIORITY_WILDCARD_CLEANED_ONLY,
        &match_context,
        "wildcard cleaned only",
    );

    add_wildcard_only_candidate(
        &mut candidates,
        &params.track_name,
        PRIORITY_WILDCARD_ORIGINAL_ONLY,
        &match_context,
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
    match_context: &MatchContext,
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
            match_context: match_context.clone(),
        });
    }
}

/// Add wildcard search candidate (q parameter with track + artist)
fn add_wildcard_search_candidate(
    candidates: &mut Vec<LyricsCandidate>,
    track: &str,
    artist: &str,
    priority: u8,
    match_context: &MatchContext,
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
            match_context: match_context.clone(),
        });
    }
}

/// Add wildcard search candidate (q parameter with track only)
fn add_wildcard_only_candidate(
    candidates: &mut Vec<LyricsCandidate>,
    track: &str,
    priority: u8,
    match_context: &MatchContext,
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
            match_context: match_context.clone(),
        });
    }
}

/// Execute search in priority stages - higher priority stages complete before lower ones
async fn execute_staged_search(
    mut candidates: Vec<LyricsCandidate>,
) -> Result<(LyricsCandidate, Vec<LyricLine>)> {
    const MIN_ACCEPTABLE_SCORE: f32 = 0.6;
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
    let _total_stages = stages.len();
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
                    result.candidate.description
                );
                if result.score >= MIN_ACCEPTABLE_SCORE {
                    return Ok((result.candidate, result.lyrics));
                }
                warn!(
                    "Stage {} best match score {:.2} below threshold {:.2}: {}",
                    stage_num + 1,
                    result.score,
                    MIN_ACCEPTABLE_SCORE,
                    result.candidate.description
                );
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
async fn execute_concurrent_stage(candidates: Vec<LyricsCandidate>) -> Result<LyricsFetchResult> {
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

    // Collect best result within stage timeout
    let stage_timeout = Duration::from_secs(STAGE_TIMEOUT_SECS);
    let deadline = tokio::time::Instant::now() + stage_timeout;
    let mut best_result: Option<LyricsFetchResult> = None;

    loop {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            break;
        }

        let remaining = deadline - now;
        let joined = timeout(remaining, set.join_next()).await;
        match joined {
            Ok(Some(Ok(Ok(result)))) => {
                if !result.lyrics.is_empty() {
                    let is_better = best_result
                        .as_ref()
                        .map(|best| result.score > best.score)
                        .unwrap_or(true);
                    if is_better {
                        best_result = Some(result);
                    }
                }
            }
            Ok(Some(Ok(Err(e)))) => {
                debug!("Candidate failed: {}", e);
            }
            Ok(Some(Err(e))) => {
                debug!("Task join error: {}", e);
            }
            Ok(None) => break,
            Err(_) => break,
        }
    }

    // Cleanup
    cancel_all.cancel();
    set.abort_all();

    best_result.ok_or_else(|| anyhow!("No successful results in stage"))
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
