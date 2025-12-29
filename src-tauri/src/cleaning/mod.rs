//! Track name cleaning module
//!
//! This module provides comprehensive track name cleaning functionality with:
//! - Multiple LLM provider support (GitHub AI, Google Gemini)
//! - Regex-based fallback cleaning
//! - Caching for duplicate requests
//! - Combined artist and track name cleaning

mod cache;
mod prompt;
pub mod providers;
mod regex;
mod types;

use log::debug;

// Re-export public types and functions
pub use regex::{clean_track_name_sync, remove_artist_from_track};
pub use types::ArtistTrackCleaningResult;

use cache::get_track_cleaning_cache;
use providers::{
    clean_artist_and_track_names_gemini, clean_artist_and_track_names_github,
    clean_track_name_gemini, clean_track_name_github,
};

/// Main track cleaning function - tries GitHub AI first, then Gemini, then regex fallback
pub async fn clean_track_name(track_name: &str) -> String {
    // Check cache first to avoid duplicate operations
    let cache = get_track_cleaning_cache();
    {
        let cache_read = cache.read().await;
        if let Some(cached_result) = cache_read.get(track_name) {
            debug!("Using cached cleaning result for: {}", track_name);
            return cached_result.clone();
        }
    }

    dotenv::dotenv().ok();

    let cleaned_result = {
        if let Ok(github_token) = std::env::var("GITHUB_TOKEN") {
            if !github_token.trim().is_empty() {
                debug!("GitHub token available, trying GitHub AI models");
                match clean_track_name_github(track_name).await {
                    Ok(cleaned) => cleaned,
                    Err(e) => {
                        debug!("GitHub AI cleaning failed, trying Gemini fallback: {}", e);
                        match clean_track_name_gemini(track_name).await {
                            Ok(cleaned) => cleaned,
                            Err(e) => {
                                debug!("Gemini cleaning also failed, using regex fallback: {}", e);
                                clean_track_name_sync(track_name)
                            }
                        }
                    }
                }
            } else {
                debug!("GitHub token is empty, skipping GitHub AI models");
                match clean_track_name_gemini(track_name).await {
                    Ok(cleaned) => cleaned,
                    Err(e) => {
                        debug!("Gemini cleaning also failed, using regex fallback: {}", e);
                        clean_track_name_sync(track_name)
                    }
                }
            }
        } else {
            debug!("No GitHub token found, skipping GitHub AI models");
            match clean_track_name_gemini(track_name).await {
                Ok(cleaned) => cleaned,
                Err(e) => {
                    debug!("Gemini cleaning also failed, using regex fallback: {}", e);
                    clean_track_name_sync(track_name)
                }
            }
        }
    };

    // Cache the result for future calls
    {
        let mut cache_write = cache.write().await;
        cache_write.insert(track_name.to_string(), cleaned_result.clone());
    }

    cleaned_result
}

/// Clean both artist and track names using LLM
pub async fn clean_artist_and_track_names(
    artist_name: &str,
    track_name: &str,
) -> Result<ArtistTrackCleaningResult, String> {
    // Try GitHub AI first, fallback to Gemini
    match clean_artist_and_track_names_github(artist_name, track_name).await {
        Ok(result) => Ok(result),
        Err(github_error) => {
            log::warn!("GitHub cleaning failed, trying Gemini: {}", github_error);
            clean_artist_and_track_names_gemini(artist_name, track_name).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_japanese_title_with_romaji_suffix_after_hyphen() {
        let input = "私じゃなかったんだね。 - Watashijyanakattandane.";
        let cleaned = clean_track_name_sync(input);
        assert_eq!(cleaned, "私じゃなかったんだね。");
    }

    #[test]
    fn cleans_youtube_suffixes_and_brackets() {
        let input = "【MV】私じゃなかったんだね。(Official Video) - YouTube Music";
        let cleaned = clean_track_name_sync(input);
        assert!(cleaned.contains("私じゃなかったんだね"));
        assert!(!cleaned.contains("YouTube"));
    }

    #[test]
    fn test_individual_cleaning_patterns() {
        let test_cases = vec![
            ("【MV】春に揺られど君想う【Official】", "春に揺られど君想う"),
            ("アーティスト「タイトル」/Album", "タイトル"),
            ("Song Title - YouTube Music", "Song Title"),
            ("Song Title (Official Music Video)", "Song Title"),
            ("Song Title (feat. Artist)", "Song Title"),
        ];

        for (input, _expected) in test_cases {
            let cleaned = clean_track_name_sync(input);
            println!("'{}' -> '{}'", input, cleaned);
        }
    }
}
