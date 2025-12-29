use log::{debug, info, warn};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::{RwLock, Semaphore};
use tokio::time::sleep;

// MusicBrainz API rate limit: 1 request per second
static RATE_LIMITER: OnceLock<Arc<Semaphore>> = OnceLock::new();

// Cache for artist aliases to avoid repeated API calls
static ARTIST_CACHE: OnceLock<Arc<RwLock<HashMap<String, Vec<String>>>>> = OnceLock::new();

// Shared HTTP client for MusicBrainz
static MB_CLIENT: OnceLock<Arc<Client>> = OnceLock::new();

/// Get rate limiter for MusicBrainz API (1 request per second)
fn get_rate_limiter() -> &'static Arc<Semaphore> {
    RATE_LIMITER.get_or_init(|| Arc::new(Semaphore::new(1)))
}

/// Get shared cache for artist aliases
fn get_artist_cache() -> &'static Arc<RwLock<HashMap<String, Vec<String>>>> {
    ARTIST_CACHE.get_or_init(|| Arc::new(RwLock::new(HashMap::new())))
}

/// Get shared HTTP client with proper User-Agent for MusicBrainz
fn get_mb_client() -> &'static Arc<Client> {
    MB_CLIENT.get_or_init(|| {
        Arc::new(
            Client::builder()
                .timeout(Duration::from_secs(10))
                .user_agent("Lyryc/0.1.0 ( https://github.com/Error0229/Lyryc )")
                .build()
                .expect("Failed to create MusicBrainz HTTP client"),
        )
    })
}

#[derive(Debug, Deserialize, Serialize)]
struct MusicBrainzArtistSearchResponse {
    artists: Vec<MusicBrainzArtist>,
}

#[derive(Debug, Deserialize, Serialize)]
struct MusicBrainzArtist {
    id: String,
    name: String,
    #[serde(rename = "sort-name")]
    sort_name: String,
    #[serde(default)]
    aliases: Vec<MusicBrainzAlias>,
}

#[derive(Debug, Deserialize, Serialize)]
struct MusicBrainzAlias {
    name: String,
    #[serde(rename = "sort-name")]
    sort_name: String,
    #[serde(rename = "type")]
    alias_type: Option<String>,
    locale: Option<String>,
    primary: Option<bool>,
}

/// Lookup artist name variants from MusicBrainz
/// Returns a list of all known name variants (aliases) for the artist
pub async fn lookup_artist_variants(artist_name: &str) -> Vec<String> {
    // Check cache first
    let cache = get_artist_cache();
    {
        let cache_read = cache.read().await;
        if let Some(cached_variants) = cache_read.get(artist_name) {
            debug!(
                "Using cached MusicBrainz variants for '{}': {:?}",
                artist_name, cached_variants
            );
            return cached_variants.clone();
        }
    }

    // Query MusicBrainz API with rate limiting
    match query_musicbrainz_artist(artist_name).await {
        Ok(variants) => {
            if !variants.is_empty() {
                info!(
                    "MusicBrainz lookup for '{}' returned {} variants: {:?}",
                    artist_name,
                    variants.len(),
                    variants
                );
                // Cache the results
                let mut cache_write = cache.write().await;
                cache_write.insert(artist_name.to_string(), variants.clone());
                variants
            } else {
                debug!(
                    "MusicBrainz lookup for '{}' found no variants, using original name",
                    artist_name
                );
                // Cache empty result to avoid repeated lookups
                let fallback = vec![artist_name.to_string()];
                let mut cache_write = cache.write().await;
                cache_write.insert(artist_name.to_string(), fallback.clone());
                fallback
            }
        }
        Err(e) => {
            warn!(
                "MusicBrainz lookup failed for '{}': {}, using original name",
                artist_name, e
            );
            // Return original name as fallback
            vec![artist_name.to_string()]
        }
    }
}

async fn query_musicbrainz_artist(artist_name: &str) -> Result<Vec<String>, String> {
    let client = get_mb_client();
    let rate_limiter = get_rate_limiter();

    // Acquire rate limit permit (1 request per second)
    let _permit = rate_limiter
        .acquire()
        .await
        .map_err(|e| format!("Rate limiter error: {}", e))?;

    // Build search query
    let query = format!("artist:{}", artist_name);
    let url = format!(
        "https://musicbrainz.org/ws/2/artist/?query={}&fmt=json&limit=3",
        urlencoding::encode(&query)
    );

    debug!("Querying MusicBrainz: {}", url);

    // Make request
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());

        // Release rate limiter after 1 second
        sleep(Duration::from_secs(1)).await;
        drop(_permit);

        return Err(format!("HTTP {}: {}", status, error_text));
    }

    let result: MusicBrainzArtistSearchResponse = response
        .json()
        .await
        .map_err(|e| format!("JSON parse error: {}", e))?;

    // Release rate limiter after 1 second
    sleep(Duration::from_secs(1)).await;
    drop(_permit);

    // Extract all unique name variants from the first matching artist
    let mut variants = Vec::new();

    if let Some(artist) = result.artists.first() {
        // Add primary name
        variants.push(artist.name.clone());

        // Add all aliases
        for alias in &artist.aliases {
            // Add the alias name if not already present
            if !variants.contains(&alias.name) {
                variants.push(alias.name.clone());
            }
        }

        debug!(
            "Found {} name variants for '{}': {:?}",
            variants.len(),
            artist_name,
            variants
        );
    }

    Ok(variants)
}
