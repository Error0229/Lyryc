//! Track name cleaning module
//!
//! This module provides comprehensive track name cleaning functionality with:
//! - Any BYOK provider via `crate::llm` (the `cleaning` route, else `translation`)
//! - Regex-based fallback cleaning
//! - Caching for duplicate requests
//! - Combined artist and track name cleaning

mod cache;
mod parse;
mod prompt;
mod regex;
mod types;

use log::{debug, info, warn};

// Re-export public types and functions
pub use regex::{clean_track_name_sync, remove_artist_from_track};
pub use types::ArtistTrackCleaningResult;

use crate::llm::{self, store::AdhocKey, ChatRequest, Target, Task, NOT_CONFIGURED};
use cache::get_track_cleaning_cache;
use parse::{parse_artist_track_cleaning_response, parse_track_cleaning_response};
use prompt::{
    build_artist_track_cleaning_prompt, build_track_cleaning_prompt,
    ARTIST_TRACK_CLEANING_SYSTEM_PROMPT, TRACK_CLEANING_SYSTEM_PROMPT,
};

/// Main track cleaning function - routed LLM first, regex fallback
pub async fn clean_track_name(track_name: &str) -> String {
    clean_track_name_with_keys(track_name, &[]).await
}

/// Track cleaning that may use legacy keys from the current call when no
/// route is configured.
pub async fn clean_track_name_with_keys(track_name: &str, call_keys: &[AdhocKey]) -> String {
    // Check cache first to avoid duplicate operations
    let cache = get_track_cleaning_cache();
    if let Some(cached_result) = cache.read().await.get(track_name) {
        debug!("Using cached cleaning result for: {}", track_name);
        return cached_result.clone();
    }

    let Some(target) = llm::resolve(Task::Cleaning, call_keys) else {
        debug!("No LLM configured for cleaning, using regex");
        return clean_track_name_sync(track_name);
    };

    match llm_clean_track(&target, track_name).await {
        Ok(cleaned) => {
            info!("LLM cleaning: '{}' -> '{}'", track_name, cleaned);
            // Only LLM results are cached, so configuring a provider later
            // is not shadowed by stale regex results.
            cache
                .write()
                .await
                .insert(track_name.to_string(), cleaned.clone());
            cleaned
        }
        Err(e) => {
            warn!("LLM cleaning failed, using regex: {}", e);
            clean_track_name_sync(track_name)
        }
    }
}

async fn llm_clean_track(target: &Target, track_name: &str) -> Result<String, String> {
    let user = build_track_cleaning_prompt(track_name);
    let reply = llm::complete(
        target,
        &ChatRequest {
            system: Some(TRACK_CLEANING_SYSTEM_PROMPT),
            user: &user,
            temperature: Some(0.1),
            max_tokens: 2048,
            json: true,
        },
    )
    .await?;
    parse_track_cleaning_response(&reply)
        .map_err(|e| format!("{} ({}): {}", target.label, target.model, e))
}

/// Clean both artist and track names using LLM
pub async fn clean_artist_and_track_names(
    artist_name: &str,
    track_name: &str,
) -> Result<ArtistTrackCleaningResult, String> {
    let target = llm::resolve(Task::Cleaning, &[])
        .ok_or_else(|| format!("{NOT_CONFIGURED} no provider is set up for cleaning"))?;
    let user = build_artist_track_cleaning_prompt(artist_name, track_name);
    let reply = llm::complete(
        &target,
        &ChatRequest {
            system: Some(ARTIST_TRACK_CLEANING_SYSTEM_PROMPT),
            user: &user,
            temperature: Some(0.1),
            max_tokens: 2048,
            json: true,
        },
    )
    .await?;
    parse_artist_track_cleaning_response(&reply)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_cleaning_against_csv_fixture() {
        let base_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let path = base_dir
            .join("tests")
            .join("fixtures")
            .join("LyricCleanData.csv");

        let mut rdr = csv::ReaderBuilder::new()
            .has_headers(true)
            .from_path(&path)
            .expect("Failed to open tests/fixtures/LyricCleanData.csv");
        let mut mismatches = Vec::new();

        for (idx, record) in rdr.records().enumerate() {
            let record = record.expect("Failed to read CSV record");
            let original = record.get(0).unwrap_or("").trim();
            let expected = record.get(1).unwrap_or("").trim();
            if original.is_empty() {
                continue;
            }

            let cleaned = clean_track_name_sync(original);
            if cleaned != expected && !cleaned.eq_ignore_ascii_case(expected) {
                mismatches.push(format!(
                    "Line {}: '{}' -> '{}' (expected '{}')",
                    idx + 2,
                    original,
                    cleaned,
                    expected
                ));
                if mismatches.len() >= 10 {
                    break;
                }
            }
        }

        assert!(
            mismatches.is_empty(),
            "Cleaning mismatches (showing up to 10):\n{}",
            mismatches.join("\n")
        );
    }

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
