//! Regex-based track name cleaning
//! This is the fallback when LLM cleaning is unavailable
//!
//! Key design decision (from commit de7d91f):
//! - Default: keep FIRST part before separator (handles "Japanese - Romaji", "Title - Subtitle")
//! - Exception: "Artist feat. X - Track" pattern → keep second part

use log::debug;
use regex::Regex;

/// Split text by common separators (-, ·, ×). Returns (first_part, second_part) or None.
fn split_by_separator(text: &str) -> Option<(&str, &str)> {
    // Try each separator pattern
    for sep in &[" - ", " – ", " — ", " · ", " × "] {
        if let Some(pos) = text.find(sep) {
            let first = text[..pos].trim();
            let second = text[pos + sep.len()..].trim();
            if !first.is_empty() && !second.is_empty() {
                return Some((first, second));
            }
        }
    }
    None
}

/// Synchronous regex-based track name cleaning
pub fn clean_track_name_sync(track_name: &str) -> String {
    let mut cleaned = track_name.to_string();
    let original_cleaned = track_name.to_string();
    debug!("Cleaning track name: '{}'", track_name);

    // First try to extract song title from quotes or brackets
    if let Some(extracted) = try_extract_from_quotes(&cleaned) {
        return extracted;
    }

    // Apply cleanup patterns
    cleaned = apply_cleanup_patterns(&cleaned);

    // Handle "Artist feat. X - Track" patterns
    if let Ok(re) = Regex::new(r"^[^-]*\s+feat\.?\s+[^-]*\s*-\s*(.+)$") {
        if let Some(captures) = re.captures(&original_cleaned) {
            if let Some(track_part) = captures.get(1) {
                let track_candidate = track_part.as_str().trim();
                if !track_candidate.is_empty() {
                    debug!("Extracted track from feat pattern: '{}'", track_candidate);
                    cleaned = track_candidate.to_string();
                }
            }
        }
    }

    // Handle separator patterns
    cleaned = handle_separator_patterns(&cleaned);

    // Handle special cases
    cleaned = handle_special_cases(&cleaned, &original_cleaned);

    // Final cleanup
    cleaned = final_cleanup(&cleaned, &original_cleaned);

    debug!("Cleaned result: '{}'", cleaned);
    cleaned
}

fn try_extract_from_quotes(text: &str) -> Option<String> {
    // Try Japanese quotes 「」
    if let Ok(quote_re) = Regex::new(r"「([^」]+)」") {
        if let Some(captures) = quote_re.captures(text) {
            if let Some(quoted_title) = captures.get(1) {
                let extracted = quoted_title.as_str().trim();
                if !extracted.is_empty() && extracted.len() > 2 {
                    debug!("Extracted from 「」quotes: '{}'", extracted);
                    return Some(extracted.to_string());
                }
            }
        }
    }

    // Try Japanese brackets with internal content extraction patterns
    if let Ok(bracket_re) = Regex::new(r"【[^】]*】([^【】]+)【[^】]*】") {
        if let Some(captures) = bracket_re.captures(text) {
            if let Some(middle_content) = captures.get(1) {
                let mut extracted = middle_content.as_str().trim().to_string();
                if !extracted.is_empty() && extracted.len() > 1 {
                    if let Ok(separator_re) = Regex::new(r"[\-/｜／|:：‐‑‒–—―]+.*$")
                    {
                        let cleaned_extracted =
                            separator_re.replace_all(&extracted, "").trim().to_string();
                        if !cleaned_extracted.is_empty() && cleaned_extracted.len() > 1 {
                            extracted = cleaned_extracted;
                        }
                    }
                    debug!("Extracted from 【】 middle content: '{}'", extracted);
                    return Some(extracted);
                }
            }
        }
    }

    // Try single Japanese brackets 【content】
    if let Ok(bracket_re) = Regex::new(r"【([^】]+)】") {
        if let Some(captures) = bracket_re.captures(text) {
            if let Some(bracketed_title) = captures.get(1) {
                let mut extracted = bracketed_title.as_str().trim().to_string();
                if !extracted.is_empty()
                    && extracted.len() > 2
                    && !extracted.to_lowercase().contains("cover")
                    && !extracted.to_lowercase().contains("mv")
                    && !extracted.to_lowercase().contains("video")
                    && !extracted.to_lowercase().contains("official")
                {
                    if let Ok(separator_re) = Regex::new(r"[\-/｜／|:：‐‑‒–—―]+.*$")
                    {
                        let cleaned_extracted =
                            separator_re.replace_all(&extracted, "").trim().to_string();
                        if !cleaned_extracted.is_empty() && cleaned_extracted.len() > 1 {
                            extracted = cleaned_extracted;
                        }
                    }
                    debug!("Extracted from 【】brackets: '{}'", extracted);
                    return Some(extracted);
                }
            }
        }
    }

    // Try Japanese corner brackets 『』
    if let Ok(bracket_re) = Regex::new(r"『([^』]+)』") {
        if let Some(captures) = bracket_re.captures(text) {
            if let Some(bracketed_title) = captures.get(1) {
                let extracted = bracketed_title.as_str().trim();
                if !extracted.is_empty() && extracted.len() > 2 {
                    debug!("Extracted from 『』brackets: '{}'", extracted);
                    return Some(extracted.to_string());
                }
            }
        }
    }

    None
}

fn apply_cleanup_patterns(text: &str) -> String {
    let mut cleaned = text.to_string();

    let cleanup_patterns = vec![
        r"\s*-\s*YouTube\s*Music\s*$",
        r"\s*-\s*YouTube\s*$",
        r"\s*\(.*?(?i:official|music|lyric).*?(?i:video|audio).*?\)",
        r"\s*【.*?(?i:official|music|lyric).*?(?i:video|audio).*?】",
        r"\s*\[.*?(?i:official|music|lyric).*?(?i:video|audio).*?\]",
        r"\s*\(.*?(?i:mv).*?\)",
        r"\s*【.*?(?i:mv).*?】",
        r"\s*\[.*?(?i:mv).*?\]",
        r"\.(?i:flv|mp4|avi|mov|wmv|mkv)$",
        r"\s*\[.*?(?i:playlist).*?\]",
        r"\s*\(.*?(?i:中文|日本語|한국어|english\s+sub|lyrics?).*?\)",
        r"\s*【.*?(?i:中文|日本語|한국어|english\s+sub|lyrics?).*?】",
        r"\s*\[.*?(?i:4k|hd|remaster).*?\]",
        r"\s*\(.*?(?i:4k).*?\)",
        r"\s*\(feat\.\s+[^)]*\)",
        r"\s*\(ft\.\s+[^)]*\)",
        r"\s*\(featuring\s+[^)]*\)",
        r"\s+ft\.?\s+@?[^()]*$",
        r"\s+feat\.?\s+[^()]*$",
        r"\s*\(prod\.\s*[^)]*\)",
        r"\s*\((?i:visualizer|video|audio)\)",
        r"\s*\((?i:acoustic\s+video)\)",
        r"\s*\([^)]*(?i:中文|chinese|中字版|华纳官方|華納官方)[^)]*\)",
        r"\s*\([^)]*(?i:english\s+sub|eng\s+sub|subtitle)[^)]*\)",
        r"\s*\((?i:twin\s+ver\.?|version)\)",
        r"\s*\((?i:magic\s+shop\s+demo|demo)\)",
        r"\s*\((?i:ryan\s+exley\s+remix|remix)\)",
        r"\s*\((?i:piano\s*&\s*cello|piano\s+version)\)",
        r"\s*\((?i:acoustic\s+session|session)\)",
        r"\s*\((?i:stripped)\)",
        r"\s*\(行ってらっしゃい\)",
        r"\s*\((?i:original)\)",
        r"\s*\(Live\s+At\s+[^)]+\)",
        r"\s*/\s*\d{4}\s*$",
    ];

    for pattern in cleanup_patterns {
        if let Ok(re) = Regex::new(&format!("(?i){}", pattern)) {
            let potential_result = re.replace_all(&cleaned, "").trim().to_string();
            if !potential_result.is_empty() && potential_result.len() > 1 {
                cleaned = potential_result;
            }
        }
    }

    cleaned
}

fn handle_separator_patterns(text: &str) -> String {
    let mut cleaned = text.to_string();

    // For "/" separators - keep first part (original behavior)
    if cleaned.contains(" / ")
        && !cleaned.to_lowercase().contains("feat")
        && !cleaned.to_lowercase().contains("ft.")
    {
        let parts: Vec<&str> = cleaned.split(" / ").collect();
        if parts.len() == 2 {
            let first_part = parts[0].trim();
            if !first_part.is_empty() && first_part.len() > 2 {
                debug!("Extracted track name before / separator: '{}'", first_part);
                cleaned = first_part.to_string();
            }
        }
    }

    // Handle separators: "-", "·", "×"
    // Rule: Keep first part, UNLESS first part has "feat/ft." then keep second
    if let Some((first, second)) = split_by_separator(&cleaned) {
        let first_lower = first.to_lowercase();
        let second_lower = second.to_lowercase();

        // "Artist feat. X - Track" → keep Track
        let is_featuring = first_lower.contains("feat") || first_lower.contains("ft.");
        let second_is_junk = second_lower.contains("youtube") || second_lower.contains("music video");

        cleaned = if is_featuring && !second.is_empty() && !second_is_junk {
            debug!("Feat pattern: keeping '{}'", second);
            second.to_string()
        } else if !first.is_empty() && first.len() > 1 {
            debug!("Separator: keeping first part '{}'", first);
            first.to_string()
        } else {
            cleaned
        };
    }

    cleaned
}

fn handle_special_cases(text: &str, original: &str) -> String {
    let mut cleaned = text.to_string();

    // Handle "Artist-Track【...】" pattern
    if let Ok(re) = Regex::new(r"^([^-]+)-([^【]+)【.*?】") {
        if let Some(captures) = re.captures(original) {
            if let Some(track_part) = captures.get(2) {
                let track_candidate = track_part.as_str().trim();
                if !track_candidate.is_empty() && track_candidate.len() > 2 {
                    debug!(
                        "Extracted track from Artist-Track【...】 pattern: '{}'",
                        track_candidate
                    );
                    cleaned = track_candidate.to_string();
                }
            }
        }
    }

    // Handle brackets around track numbers
    if let Ok(re) = Regex::new(r"（.*?(VIDEO|video|MV|mv).*?）") {
        let potential_result = re.replace_all(&cleaned, "").trim().to_string();
        if !potential_result.is_empty() && potential_result.len() > 1 {
            cleaned = potential_result;
        }
    }

    // Remove remaining Japanese brackets
    if let Ok(re) = Regex::new(r"【.*?】") {
        let potential_result = re.replace_all(&cleaned, "").trim().to_string();
        if !potential_result.is_empty() && potential_result.len() > 1 {
            cleaned = potential_result;
        }
    }

    // Handle pipe separators
    if cleaned.contains(" | ") {
        let parts: Vec<&str> = cleaned.split(" | ").collect();
        if parts.len() > 1 {
            let first_part = parts[0].trim();
            if !first_part.is_empty() && first_part.len() > 2 {
                cleaned = first_part.to_string();
            }
        }
    }

    // Remove quotes around the entire string
    if cleaned.starts_with('"') && cleaned.ends_with('"') && cleaned.len() > 2 {
        cleaned = cleaned[1..cleaned.len() - 1].to_string();
    }
    if cleaned.starts_with('\'') && cleaned.ends_with('\'') && cleaned.len() > 2 {
        cleaned = cleaned[1..cleaned.len() - 1].to_string();
    }
    if cleaned.starts_with('「') && cleaned.ends_with('」') && cleaned.len() > 2 {
        cleaned = cleaned[1..cleaned.len() - 1].to_string();
    }

    // Clean up multiple spaces
    if let Ok(re) = Regex::new(r"\s+") {
        cleaned = re.replace_all(&cleaned, " ").trim().to_string();
    }

    cleaned
}

fn final_cleanup(text: &str, original: &str) -> String {
    let mut cleaned = text.to_string();

    // Handle complex Japanese patterns
    if cleaned.contains("／") || (cleaned.contains("feat.") && cleaned.contains("×")) {
        if cleaned.contains("／") {
            let parts: Vec<&str> = cleaned.split("／").collect();
            if !parts.is_empty() {
                let first_part = parts[0].trim();
                if !first_part.is_empty() && first_part.len() > 2 {
                    cleaned = first_part.to_string();
                }
            }
        }
    }

    // Handle remaining full-width parentheses
    if let Ok(re) = Regex::new(r"（.*?）") {
        let potential_result = re.replace_all(&cleaned, "").trim().to_string();
        if !potential_result.is_empty() && potential_result.len() > 3 {
            cleaned = potential_result;
        }
    }

    // Handle remaining dot separators
    if cleaned.contains(" · ") && !cleaned.contains(" - ") {
        let parts: Vec<&str> = cleaned.split(" · ").collect();
        if parts.len() >= 2 {
            let last_part = parts.last().unwrap().trim();
            if !last_part.is_empty() && last_part.len() >= 1 && last_part.len() < 50 {
                cleaned = last_part.to_string();
            }
        }
    }

    // Handle ft. patterns
    if let Ok(re) = Regex::new(r"\s+ft\.?\s+[^(),-]*$") {
        let potential_result = re.replace_all(&cleaned, "").trim().to_string();
        if !potential_result.is_empty() && potential_result.len() > 1 {
            cleaned = potential_result;
        }
    }

    // Remove standalone (Acoustic) at the end
    if cleaned.ends_with(" (Acoustic)") {
        let without_acoustic = cleaned.replace(" (Acoustic)", "");
        if !without_acoustic.trim().is_empty() {
            cleaned = without_acoustic.trim().to_string();
        }
    }

    // Handle "Official Video" at the end
    if let Ok(re) = Regex::new(r"\s+Official\s+Video\s*$") {
        let potential_result = re.replace_all(&cleaned, "").trim().to_string();
        if !potential_result.is_empty() && potential_result.len() > 1 {
            cleaned = potential_result;
        }
    }

    // Handle multi-dash cases
    if cleaned.contains(" - ") && cleaned.split(" - ").count() > 2 {
        let parts: Vec<&str> = cleaned.split(" - ").collect();
        if parts.len() >= 3 {
            let first_part = parts[0].trim();
            if !first_part.is_empty() && first_part.len() > 2 && first_part.len() < 50 {
                cleaned = first_part.to_string();
            }
        }
    }

    // Special case for Japanese covers
    if cleaned.contains("/ Japanese Cover") {
        cleaned = cleaned.replace("/ Japanese Cover", " (Japanese Cover)");
    }

    // One last attempt for Artist - Track pattern
    if cleaned == original && cleaned.contains(" - ") {
        let parts: Vec<&str> = cleaned.split(" - ").collect();
        if parts.len() == 2 {
            let track_part = parts[1].trim();
            if !track_part.is_empty() {
                cleaned = track_part.to_string();
            }
        }
    }

    // Handle (Full Album) cases
    if original.contains("(Full Album) -") {
        if let Ok(re) = Regex::new(r"^([^-]+)\(Full Album\)\s*-.*$") {
            if let Some(captures) = re.captures(original) {
                if let Some(artist_album) = captures.get(1) {
                    let artist_name = artist_album.as_str().trim();
                    cleaned = format!("{} (Full Album)", artist_name);
                }
            }
        }
    }

    cleaned
}

/// Remove artist name from track title if present
pub fn remove_artist_from_track(track_name: &str, artist_name: &str) -> String {
    if artist_name.trim().is_empty() {
        return track_name.to_string();
    }

    let escaped_artist = regex::escape(artist_name);
    let mut result = track_name.to_string();

    let patterns = vec![
        format!(r"^{}\s*[-–—]\s*", escaped_artist),
        format!(r"\s*[-–—]\s*{}$", escaped_artist),
        format!(r"\s*[-–—]\s*{}\s*$", escaped_artist),
    ];

    for pattern in patterns {
        if let Ok(re) = Regex::new(&format!("(?i){}", pattern)) {
            let new_result = re.replace_all(&result, "").trim().to_string();
            if new_result.len() < result.len() && !new_result.is_empty() {
                result = new_result;
                debug!(
                    "Removed artist '{}' from track, result: '{}'",
                    artist_name, result
                );
            }
        }
    }

    result
}
