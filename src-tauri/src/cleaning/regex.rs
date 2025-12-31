//! Regex-based track name cleaning
//! This is the fallback when LLM cleaning is unavailable
//!
//! Key design decision (from commit de7d91f):
//! - Default: keep FIRST part before separator (handles "Japanese - Romaji", "Title - Subtitle")
//! - Exception: "Artist feat. X - Track" pattern → keep second part

use log::debug;
use regex::Regex;

// Pre-compiled regex patterns (compiled once at first use)
lazy_static::lazy_static! {
    // Quote/bracket extraction patterns
    static ref RE_JAPANESE_QUOTES: Regex = Regex::new(r"「([^」]+)」").unwrap();
    static ref RE_BRACKET_MIDDLE: Regex = Regex::new(r"【[^】]*】([^【】]+)【[^】]*】").unwrap();
    static ref RE_BRACKET_SINGLE: Regex = Regex::new(r"【([^】]+)】").unwrap();
    static ref RE_CORNER_BRACKETS: Regex = Regex::new(r"『([^』]+)』").unwrap();
    static ref RE_SEPARATOR_CLEANUP: Regex = Regex::new(r"[\-/｜／|:：‐‑‒–—―]+.*$").unwrap();

    // Feat pattern
    static ref RE_FEAT_PATTERN: Regex = Regex::new(r"(?i)^[^-]*\s+feat\.?\s+[^-]*\s*-\s*(.+)$").unwrap();

    // Special case patterns
    static ref RE_ARTIST_TRACK_BRACKET: Regex = Regex::new(r"^([^-]+)-([^【]+)【.*?】").unwrap();
    static ref RE_VIDEO_PARENS: Regex = Regex::new(r"（.*?(VIDEO|video|MV|mv).*?）").unwrap();
    static ref RE_JAPANESE_BRACKETS: Regex = Regex::new(r"【.*?】").unwrap();
    static ref RE_WHITESPACE: Regex = Regex::new(r"\s+").unwrap();
    static ref RE_FULLWIDTH_PARENS: Regex = Regex::new(r"（.*?）").unwrap();
    static ref RE_FT_SUFFIX: Regex = Regex::new(r"(?i)\s+ft\.?\s+[^(),-]*$").unwrap();
    static ref RE_OFFICIAL_VIDEO: Regex = Regex::new(r"(?i)\s+Official\s+Video\s*$").unwrap();
    static ref RE_FULL_ALBUM: Regex = Regex::new(r"^([^-]+)\(Full Album\)\s*-.*$").unwrap();

    // Cleanup patterns (case-insensitive)
    static ref CLEANUP_PATTERNS: Vec<Regex> = vec![
        Regex::new(r"(?i)\s*-\s*YouTube\s*Music\s*$").unwrap(),
        Regex::new(r"(?i)\s*-\s*YouTube\s*$").unwrap(),
        Regex::new(r"(?i)\s*\(.*?(?:official|music|lyric).*?(?:video|audio).*?\)").unwrap(),
        Regex::new(r"(?i)\s*【.*?(?:official|music|lyric).*?(?:video|audio).*?】").unwrap(),
        Regex::new(r"(?i)\s*\[.*?(?:official|music|lyric).*?(?:video|audio).*?\]").unwrap(),
        Regex::new(r"(?i)\s*\(.*?mv.*?\)").unwrap(),
        Regex::new(r"(?i)\s*【.*?mv.*?】").unwrap(),
        Regex::new(r"(?i)\s*\[.*?mv.*?\]").unwrap(),
        Regex::new(r"(?i)\.(?:flv|mp4|avi|mov|wmv|mkv)$").unwrap(),
        Regex::new(r"(?i)\s*\[.*?playlist.*?\]").unwrap(),
        Regex::new(r"(?i)\s*\(.*?(?:中文|日本語|한국어|english\s+sub|lyrics?).*?\)").unwrap(),
        Regex::new(r"(?i)\s*【.*?(?:中文|日本語|한국어|english\s+sub|lyrics?).*?】").unwrap(),
        Regex::new(r"(?i)\s*\[.*?(?:4k|hd|remaster).*?\]").unwrap(),
        Regex::new(r"(?i)\s*\(.*?4k.*?\)").unwrap(),
        Regex::new(r"(?i)\s*\(feat\.\s+[^)]*\)").unwrap(),
        Regex::new(r"(?i)\s*\(ft\.\s+[^)]*\)").unwrap(),
        Regex::new(r"(?i)\s*\(featuring\s+[^)]*\)").unwrap(),
        Regex::new(r"(?i)\s+ft\.?\s+@?[^()]*$").unwrap(),
        Regex::new(r"(?i)\s+feat\.?\s+[^()]*$").unwrap(),
        Regex::new(r"(?i)\s*\(prod\.\s*[^)]*\)").unwrap(),
        Regex::new(r"(?i)\s*\((?:visualizer|video|audio)\)").unwrap(),
        Regex::new(r"(?i)\s*\((?:acoustic\s+video)\)").unwrap(),
        Regex::new(r"(?i)\s*\([^)]*(?:中文|chinese|中字版|华纳官方|華納官方)[^)]*\)").unwrap(),
        Regex::new(r"(?i)\s*\([^)]*(?:english\s+sub|eng\s+sub|subtitle)[^)]*\)").unwrap(),
        Regex::new(r"(?i)\s*\((?:twin\s+ver\.?|version)\)").unwrap(),
        Regex::new(r"(?i)\s*\((?:magic\s+shop\s+demo|demo)\)").unwrap(),
        Regex::new(r"(?i)\s*\((?:ryan\s+exley\s+remix|remix)\)").unwrap(),
        Regex::new(r"(?i)\s*\((?:piano\s*&\s*cello|piano\s+version)\)").unwrap(),
        Regex::new(r"(?i)\s*\((?:acoustic\s+session|session)\)").unwrap(),
        Regex::new(r"(?i)\s*\((?:stripped)\)").unwrap(),
        Regex::new(r"\s*\(行ってらっしゃい\)").unwrap(),
        Regex::new(r"(?i)\s*\((?:original)\)").unwrap(),
        Regex::new(r"(?i)\s*\(Live\s+At\s+[^)]+\)").unwrap(),
        Regex::new(r"\s*/\s*\d{4}\s*$").unwrap(),
    ];
}

/// Split text by common separators (-, ·, ×). Returns (first_part, second_part) or None.
fn split_by_separator(text: &str) -> Option<(&str, &str)> {
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
    let original = track_name;
    debug!("Cleaning track name: '{}'", track_name);

    // First try to extract song title from quotes or brackets
    if let Some(extracted) = try_extract_from_quotes(&cleaned) {
        return extracted;
    }

    // Apply cleanup patterns
    cleaned = apply_cleanup_patterns(&cleaned);

    // Handle "Artist feat. X - Track" patterns
    if let Some(captures) = RE_FEAT_PATTERN.captures(original) {
        if let Some(track_part) = captures.get(1) {
            let candidate = track_part.as_str().trim();
            if !candidate.is_empty() {
                debug!("Extracted track from feat pattern: '{}'", candidate);
                cleaned = candidate.to_string();
            }
        }
    }

    // Handle separator patterns
    cleaned = handle_separator_patterns(&cleaned);

    // Handle special cases
    cleaned = handle_special_cases(&cleaned, original);

    // Final cleanup
    cleaned = final_cleanup(&cleaned, original);

    debug!("Cleaned result: '{}'", cleaned);
    cleaned
}

fn try_extract_from_quotes(text: &str) -> Option<String> {
    // Try Japanese quotes 「」
    if let Some(captures) = RE_JAPANESE_QUOTES.captures(text) {
        if let Some(quoted) = captures.get(1) {
            let extracted = quoted.as_str().trim();
            if !extracted.is_empty() && extracted.len() > 2 {
                debug!("Extracted from 「」quotes: '{}'", extracted);
                return Some(extracted.to_string());
            }
        }
    }

    // Try Japanese brackets with internal content extraction
    if let Some(captures) = RE_BRACKET_MIDDLE.captures(text) {
        if let Some(middle) = captures.get(1) {
            let mut extracted = middle.as_str().trim().to_string();
            if !extracted.is_empty() && extracted.len() > 1 {
                let cleaned = RE_SEPARATOR_CLEANUP.replace_all(&extracted, "").trim().to_string();
                if !cleaned.is_empty() && cleaned.len() > 1 {
                    extracted = cleaned;
                }
                debug!("Extracted from 【】 middle content: '{}'", extracted);
                return Some(extracted);
            }
        }
    }

    // Try single Japanese brackets 【content】
    if let Some(captures) = RE_BRACKET_SINGLE.captures(text) {
        if let Some(bracketed) = captures.get(1) {
            let mut extracted = bracketed.as_str().trim().to_string();
            let lower = extracted.to_lowercase();
            if !extracted.is_empty()
                && extracted.len() > 2
                && !lower.contains("cover")
                && !lower.contains("mv")
                && !lower.contains("video")
                && !lower.contains("official")
            {
                let cleaned = RE_SEPARATOR_CLEANUP.replace_all(&extracted, "").trim().to_string();
                if !cleaned.is_empty() && cleaned.len() > 1 {
                    extracted = cleaned;
                }
                debug!("Extracted from 【】brackets: '{}'", extracted);
                return Some(extracted);
            }
        }
    }

    // Try Japanese corner brackets 『』
    if let Some(captures) = RE_CORNER_BRACKETS.captures(text) {
        if let Some(bracketed) = captures.get(1) {
            let extracted = bracketed.as_str().trim();
            if !extracted.is_empty() && extracted.len() > 2 {
                debug!("Extracted from 『』brackets: '{}'", extracted);
                return Some(extracted.to_string());
            }
        }
    }

    None
}

fn apply_cleanup_patterns(text: &str) -> String {
    let mut cleaned = text.to_string();

    for re in CLEANUP_PATTERNS.iter() {
        let result = re.replace_all(&cleaned, "").trim().to_string();
        if !result.is_empty() && result.len() > 1 {
            cleaned = result;
        }
    }

    cleaned
}

fn handle_separator_patterns(text: &str) -> String {
    let mut cleaned = text.to_string();
    let lower = cleaned.to_lowercase();

    // For "/" separators - keep first part
    if cleaned.contains(" / ") && !lower.contains("feat") && !lower.contains("ft.") {
        let parts: Vec<&str> = cleaned.split(" / ").collect();
        if parts.len() == 2 {
            let first = parts[0].trim();
            if !first.is_empty() && first.len() > 2 {
                debug!("Extracted track before /: '{}'", first);
                cleaned = first.to_string();
            }
        }
    }

    // Handle separators: "-", "·", "×"
    if let Some((first, second)) = split_by_separator(&cleaned) {
        let first_lower = first.to_lowercase();
        let second_lower = second.to_lowercase();

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
    if let Some(captures) = RE_ARTIST_TRACK_BRACKET.captures(original) {
        if let Some(track_part) = captures.get(2) {
            let candidate = track_part.as_str().trim();
            if !candidate.is_empty() && candidate.len() > 2 {
                debug!("Extracted from Artist-Track【】: '{}'", candidate);
                cleaned = candidate.to_string();
            }
        }
    }

    // Remove video parentheses
    let result = RE_VIDEO_PARENS.replace_all(&cleaned, "").trim().to_string();
    if !result.is_empty() && result.len() > 1 {
        cleaned = result;
    }

    // Remove Japanese brackets
    let result = RE_JAPANESE_BRACKETS.replace_all(&cleaned, "").trim().to_string();
    if !result.is_empty() && result.len() > 1 {
        cleaned = result;
    }

    // Handle pipe separators
    if cleaned.contains(" | ") {
        let parts: Vec<&str> = cleaned.split(" | ").collect();
        if parts.len() > 1 {
            let first = parts[0].trim();
            if !first.is_empty() && first.len() > 2 {
                cleaned = first.to_string();
            }
        }
    }

    // Remove quotes around entire string
    if cleaned.len() > 2 {
        if (cleaned.starts_with('"') && cleaned.ends_with('"'))
            || (cleaned.starts_with('\'') && cleaned.ends_with('\''))
            || (cleaned.starts_with('「') && cleaned.ends_with('」'))
        {
            cleaned = cleaned[1..cleaned.len() - 1].to_string();
        }
    }

    // Normalize whitespace
    cleaned = RE_WHITESPACE.replace_all(&cleaned, " ").trim().to_string();

    cleaned
}

fn final_cleanup(text: &str, original: &str) -> String {
    let mut cleaned = text.to_string();

    // Handle Japanese full-width slash
    if cleaned.contains("／") {
        let parts: Vec<&str> = cleaned.split("／").collect();
        if !parts.is_empty() {
            let first = parts[0].trim();
            if !first.is_empty() && first.len() > 2 {
                cleaned = first.to_string();
            }
        }
    }

    // Handle remaining full-width parentheses
    let result = RE_FULLWIDTH_PARENS.replace_all(&cleaned, "").trim().to_string();
    if !result.is_empty() && result.len() > 3 {
        cleaned = result;
    }

    // Handle dot separators
    if cleaned.contains(" · ") && !cleaned.contains(" - ") {
        let parts: Vec<&str> = cleaned.split(" · ").collect();
        if parts.len() >= 2 {
            if let Some(last) = parts.last() {
                let last = last.trim();
                if !last.is_empty() && last.len() < 50 {
                    cleaned = last.to_string();
                }
            }
        }
    }

    // Handle ft. suffix
    let result = RE_FT_SUFFIX.replace_all(&cleaned, "").trim().to_string();
    if !result.is_empty() && result.len() > 1 {
        cleaned = result;
    }

    // Remove (Acoustic) suffix
    if cleaned.ends_with(" (Acoustic)") {
        let without = cleaned.replace(" (Acoustic)", "");
        if !without.trim().is_empty() {
            cleaned = without.trim().to_string();
        }
    }

    // Remove Official Video suffix
    let result = RE_OFFICIAL_VIDEO.replace_all(&cleaned, "").trim().to_string();
    if !result.is_empty() && result.len() > 1 {
        cleaned = result;
    }

    // Handle multi-dash cases
    if cleaned.contains(" - ") && cleaned.split(" - ").count() > 2 {
        let parts: Vec<&str> = cleaned.split(" - ").collect();
        if parts.len() >= 3 {
            let first = parts[0].trim();
            if !first.is_empty() && first.len() > 2 && first.len() < 50 {
                cleaned = first.to_string();
            }
        }
    }

    // Handle Japanese cover suffix
    if cleaned.contains("/ Japanese Cover") {
        cleaned = cleaned.replace("/ Japanese Cover", " (Japanese Cover)");
    }

    // Last attempt for Artist - Track
    if cleaned == original && cleaned.contains(" - ") {
        let parts: Vec<&str> = cleaned.split(" - ").collect();
        if parts.len() == 2 {
            let track = parts[1].trim();
            if !track.is_empty() {
                cleaned = track.to_string();
            }
        }
    }

    // Handle (Full Album)
    if original.contains("(Full Album) -") {
        if let Some(captures) = RE_FULL_ALBUM.captures(original) {
            if let Some(artist) = captures.get(1) {
                cleaned = format!("{} (Full Album)", artist.as_str().trim());
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

    let escaped = regex::escape(artist_name);
    let mut result = track_name.to_string();

    // Build patterns with escaped artist name
    let patterns = [
        format!(r"(?i)^{}\s*[-–—]\s*", escaped),
        format!(r"(?i)\s*[-–—]\s*{}$", escaped),
    ];

    for pattern in &patterns {
        if let Ok(re) = Regex::new(pattern) {
            let new_result = re.replace_all(&result, "").trim().to_string();
            if new_result.len() < result.len() && !new_result.is_empty() {
                result = new_result;
                debug!("Removed artist '{}', result: '{}'", artist_name, result);
            }
        }
    }

    result
}
