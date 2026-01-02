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
    static ref RE_CJK_ANGLE: Regex = Regex::new(r"《([^》]+)》").unwrap();
    static ref RE_SQUARE_BRACKETS: Regex = Regex::new(r"\[([^\]]+)\]").unwrap();
    static ref RE_SEPARATOR_CLEANUP: Regex = Regex::new(r"[\-/｜／|:：‐‑‒–—―]+.*$").unwrap();

    // Feat pattern
    static ref RE_FEAT_PATTERN: Regex = Regex::new(r"(?i)^[^-]*\s+feat\.?\s+[^-]*\s*-\s*(.+)$").unwrap();

    // Special case patterns
    static ref RE_ARTIST_TRACK_BRACKET: Regex = Regex::new(r"^([^-]+)-([^【]+)【.*?】").unwrap();
    static ref RE_VIDEO_PARENS: Regex = Regex::new(r"(?i)（.*?(video|mv).*?）").unwrap();
    static ref RE_JAPANESE_BRACKETS: Regex = Regex::new(r"【.*?】").unwrap();
    static ref RE_WHITESPACE: Regex = Regex::new(r"\s+").unwrap();
    static ref RE_FULLWIDTH_PARENS: Regex = Regex::new(r"（.*?）").unwrap();
    static ref RE_FT_SUFFIX: Regex = Regex::new(r"(?i)\s+ft\.?\s*[^(),-]*$").unwrap();
    static ref RE_FEAT_SUFFIX: Regex = Regex::new(r"(?i)\s+feat\.?\s*[^(),-]*$").unwrap();
    static ref RE_FT_FEAT_BEFORE_PARENS: Regex =
        Regex::new(r"(?i)\s+(?:ft|feat)\.?\s+[^()]*\s*\(").unwrap();
    static ref RE_LYRICS_SUFFIX: Regex = Regex::new(r"(?i)\s*[|∣]\s*lyrics.*$").unwrap();
    static ref RE_BRACKET_TAGS: Regex =
        Regex::new(r"(?i)\s*\[[^\]]*(?:4k|hd|remaster)[^\]]*\]").unwrap();
    static ref RE_ASCII_PARENS: Regex = Regex::new(r"\([^)]*\)").unwrap();
    static ref RE_OFFICIAL_VIDEO: Regex = Regex::new(r"(?i)\s+Official\s+Video\s*$").unwrap();
    static ref RE_FULL_ALBUM: Regex = Regex::new(r"^([^-]+)\(Full Album\)\s*-.*$").unwrap();
    static ref RE_LOOSE_DASH_SEPARATOR: Regex = Regex::new(r"^(.+?)\s*-\s*(.+)$").unwrap();
    static ref RE_FEAT_PARENS: Regex =
        Regex::new(r"(?i)\s*\((?:feat|ft|featuring)\.?\s+[^)]*\)").unwrap();
    static ref RE_FEAT_PARENS_CAPTURE: Regex =
        Regex::new(r"(?i)\s*\((?:feat|ft|featuring)\.?\s+([^)]*)\)").unwrap();
    static ref RE_QUOTED_TITLE_WITH_SUFFIX: Regex =
        Regex::new(r#"^'([^']+)'\s+(.+)$"#).unwrap();
    static ref RE_DOUBLE_QUOTED_TITLE_WITH_SUFFIX: Regex =
        Regex::new(r#"^"([^"]+)"\s+(.+)$"#).unwrap();

    // Cleanup patterns (case-insensitive)
    static ref CLEANUP_PATTERNS: Vec<Regex> = vec![
        Regex::new(r"(?i)\s*-\s*YouTube\s*Music\s*$").unwrap(),
        Regex::new(r"(?i)\s*-\s*YouTube\s*$").unwrap(),
        Regex::new(r"(?i)\s*\([^)]*(?:official|music|lyric)[^)]*(?:video|audio)[^)]*\)").unwrap(),
        Regex::new(r"(?i)\s*【[^】]*(?:official|music|lyric)[^】]*(?:video|audio)[^】]*】").unwrap(),
        Regex::new(r"(?i)\s*\[[^\]]*(?:official|music|lyric)[^\]]*(?:video|audio)[^\]]*\]").unwrap(),
        Regex::new(r"(?i)\s*\(.*?mv.*?\)").unwrap(),
        Regex::new(r"(?i)\s*【.*?mv.*?】").unwrap(),
        Regex::new(r"(?i)\s*\[.*?mv.*?\]").unwrap(),
        Regex::new(r"(?i)\.(?:flv|mp4|avi|mov|wmv|mkv)$").unwrap(),
        Regex::new(r"(?i)\s*\[.*?playlist.*?\]").unwrap(),
        Regex::new(r"(?i)\s*\(.*?(?:中文|日本語|한국어|english\s+sub|lyrics?).*?\)").unwrap(),
        Regex::new(r"(?i)\s*【.*?(?:中文|日本語|한국어|english\s+sub|lyrics?).*?】").unwrap(),
        Regex::new(r"(?i)\s*\[.*?(?:4k|hd|remaster).*?\]").unwrap(),
        Regex::new(r"(?i)\s*\(.*?4k.*?\)").unwrap(),
        Regex::new(r"(?i)\s+ft\.?\s+@?[^()]*$").unwrap(),
        Regex::new(r"(?i)\s+feat\.?\s+[^()]*$").unwrap(),
        Regex::new(r"(?i)\s*\(prod\.\s*[^)]*\)").unwrap(),
        Regex::new(r"(?i)\s*\((?:visualizer|video|audio)\)").unwrap(),
        Regex::new(r"(?i)\s*\((?:acoustic\s+video)\)").unwrap(),
        Regex::new(r"(?i)\s*\(lyrics\)").unwrap(),
        Regex::new(r"(?i)\s*\([^)]*(?:中文|chinese|中字版|华纳官方|華納官方)[^)]*\)").unwrap(),
        Regex::new(r"(?i)\s*\([^)]*(?:english\s+sub|eng\s+sub|subtitle)[^)]*\)").unwrap(),
        Regex::new(r"(?i)\s*\(from the series[^)]*\)").unwrap(),
        Regex::new(r"(?i)\s*\([^)]*BGM[^)]*\)").unwrap(),
        Regex::new(r"\s*\([^)]*(?:插曲|片尾曲|片頭曲)[^)]*\)").unwrap(),
        Regex::new(r"(?i)\s*\(where darkness ends\)").unwrap(),
        Regex::new(r"を\s*歌ってみた.*$").unwrap(),
        Regex::new(r"\s*歌ってみた.*$").unwrap(),
        Regex::new(r"(?i)\s*-\s*arrange.*$").unwrap(),
        Regex::new(r"(?i)\s+music\s+video\s*$").unwrap(),
        Regex::new(r"(?i)\s*\|+\s*official\s+video\s*$").unwrap(),
        Regex::new(r"(?i)\s*[|∣]\s*lyrics.*$").unwrap(),
        Regex::new(r"(?i)\s*\((?:original)\)").unwrap(),
    ];
}

/// Split text by common separators (-, ·, ×). Returns (first_part, second_part) or None.
fn split_by_separator(text: &str) -> Option<(&str, &str)> {
    for sep in &[" - ", " – ", " — ", " · ", " × ", " • "] {
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

fn is_cjk_char(c: char) -> bool {
    matches!(
        c as u32,
        0x3040..=0x30ff // Hiragana + Katakana
            | 0x4e00..=0x9fff // CJK Unified Ideographs
            | 0x3400..=0x4dbf // CJK Unified Ideographs Extension A
            | 0xac00..=0xd7af // Hangul Syllables
    )
}

fn contains_cjk(s: &str) -> bool {
    s.chars().any(is_cjk_char)
}

fn is_mostly_ascii(s: &str) -> bool {
    let mut total = 0usize;
    let mut ascii = 0usize;
    for c in s.chars() {
        if c.is_whitespace() {
            continue;
        }
        total += 1;
        if c.is_ascii() {
            ascii += 1;
        }
    }
    total > 0 && ascii * 100 / total >= 90
}

/// Synchronous regex-based track name cleaning
pub fn clean_track_name_sync(track_name: &str) -> String {
    let mut cleaned = track_name.to_string();
    let original = track_name;
    debug!("Cleaning track name: '{}'", track_name);

    // First try to extract song title from quotes or brackets
    if let Some(extracted) = try_extract_from_quotes(&cleaned) {
        let original_trim = track_name.trim_start();
        if contains_cjk(&extracted)
            && !extracted.contains(" / ")
            && !extracted.contains('／')
            && !extracted.contains(" - ")
            && !original_trim.starts_with('【')
        {
            return extracted;
        }
        cleaned = extracted;
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
            let extracted_len = extracted.chars().count();
            let match_end = captures.get(0).map(|m| m.end()).unwrap_or(0);
            let match_start = captures.get(0).map(|m| m.start()).unwrap_or(0);
            let has_trailing_text = text[match_end..].trim().len() > 0;
            let has_prefix_text = text[..match_start].trim().len() > 0;
            if !extracted.is_empty()
                && extracted_len > 2
                && !(has_trailing_text && !has_prefix_text && extracted_len <= 6)
                && !(has_prefix_text && !has_trailing_text && extracted_len <= 6)
                && !extracted.contains(':')
                && !extracted.contains('×')
                && !extracted.contains('&')
                && !lower.contains("cover")
                && !lower.contains("mv")
                && !lower.contains("video")
                && !lower.contains("official")
                && !lower.contains("歌ってみた")
                && !lower.contains("原创")
                && !lower.contains("原創")
                && !lower.contains("歌詞")
                && !lower.contains("lyrics")
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

    // Try CJK angle brackets 《》
    if let Some(captures) = RE_CJK_ANGLE.captures(text) {
        if let Some(bracketed) = captures.get(1) {
            let match_start = captures.get(0).map(|m| m.start()).unwrap_or(0);
            let match_end = captures.get(0).map(|m| m.end()).unwrap_or(0);
            let before_paren = text[..match_start].rfind('(');
            let after_paren = text[match_end..].find(')');
            if before_paren.is_none() || after_paren.is_none() {
                let extracted = bracketed.as_str().trim();
                if !extracted.is_empty() && extracted.len() > 2 {
                    debug!("Extracted from 《》brackets: '{}'", extracted);
                    return Some(extracted.to_string());
                }
            }
        }
    }

    // Try square brackets []
    if let Some(captures) = RE_SQUARE_BRACKETS.captures(text) {
        if let Some(bracketed) = captures.get(1) {
            let extracted = bracketed.as_str().trim();
            let lower = extracted.to_lowercase();
            let first_dash = text.find('-');
            let bracket_end = captures.get(0).map(|m| m.end()).unwrap_or(0);
            let before_dash = first_dash.map_or(false, |pos| bracket_end <= pos);
            if !extracted.is_empty()
                && extracted.len() > 2
                && !lower.contains("official")
                && !lower.contains("mv")
                && !lower.contains("video")
                && !lower.contains("playlist")
                && !lower.contains("remaster")
                && !lower.contains("4k")
                && !before_dash
            {
                if contains_cjk(extracted) && extracted.chars().any(|c| c.is_ascii_alphabetic()) {
                    let cjk_only: String = extracted
                        .chars()
                        .filter(|c| is_cjk_char(*c) || c.is_whitespace())
                        .collect();
                    let cjk_only = cjk_only.trim();
                    if !cjk_only.is_empty() {
                        debug!("Extracted CJK from []: '{}'", cjk_only);
                        return Some(cjk_only.to_string());
                    }
                }
                debug!("Extracted from [] brackets: '{}'", extracted);
                return Some(extracted.to_string());
            }
        }
    }

    None
}

fn apply_cleanup_patterns(text: &str) -> String {
    let original = text.trim();
    let mut best = original.to_string();
    let mut best_reduction = 0usize;

    for re in CLEANUP_PATTERNS.iter() {
        let candidate = re.replace_all(original, "").trim().to_string();
        if candidate.len() <= 1 || !candidate.chars().any(|c| c.is_alphanumeric()) {
            continue;
        }

        let reduction = original.len().saturating_sub(candidate.len());
        if reduction > best_reduction {
            best_reduction = reduction;
            best = candidate;
        }
    }

    best
}

fn handle_separator_patterns(text: &str) -> String {
    let mut cleaned = text.to_string();
    let lower = cleaned.to_lowercase();

    // For "/" separators - keep first part only for known tag-like suffixes
    if cleaned.contains(" / ") && !lower.contains("feat") && !lower.contains("ft.") {
        if let Some(slash_pos) = cleaned.find(" / ") {
            let open_paren = cleaned[..slash_pos].rfind('(');
            let close_paren = cleaned[slash_pos + 3..].find(')');
            if open_paren.is_none() || close_paren.is_none() {
                let parts: Vec<&str> = cleaned.split(" / ").collect();
                if parts.len() == 2 {
                    let first = parts[0].trim();
                    let second = parts[1].trim();
                    let second_lower = second.to_lowercase();
                    let is_year_suffix =
                        second.len() == 4 && second.chars().all(|c| c.is_ascii_digit());
                    let is_tag_suffix = second_lower.contains("first take");
                    let is_feat_suffix = second_lower.contains("feat") || second_lower.contains("ft.");
                    let is_compact_suffix = !second.contains(' ') && second.chars().count() <= 6;
                    let is_official_suffix =
                        second_lower.contains("official") && contains_cjk(first) && contains_cjk(second);
                    let is_cover_suffix = second_lower.contains("歌ってみた");
                    let is_cjk_pair = contains_cjk(first) && contains_cjk(second);
                    if (is_year_suffix
                        || is_tag_suffix
                        || is_feat_suffix
                        || is_compact_suffix
                        || is_official_suffix
                        || is_cover_suffix
                        || is_cjk_pair)
                        && !first.is_empty()
                        && first.len() > 2
                    {
                        debug!("Extracted track before /: '{}'", first);
                        cleaned = first.to_string();
                    }
                }
            }
        }
    }

    // Handle separators: "-", "·", "×"
    let split = split_by_separator(&cleaned).or_else(|| {
        RE_LOOSE_DASH_SEPARATOR.captures(&cleaned).map(|captures| {
            let first = captures.get(1).map(|m| m.as_str().trim()).unwrap_or("");
            let second = captures.get(2).map(|m| m.as_str().trim()).unwrap_or("");
            (first, second)
        })
    });

    if let Some((first, second)) = split {
        let first_lower = first.to_lowercase();
        let second_lower = second.to_lowercase();

        let is_featuring = first_lower.contains("feat") || first_lower.contains("ft.");
        let second_is_lyrics = second_lower.contains("lyrics");
        let second_is_junk = second_lower.contains("youtube") || second_lower.contains("cover by");
        let prefer_first_for_lyrics = second_is_lyrics && first.len() <= 20;
        let prefer_first = contains_cjk(first)
            && !contains_cjk(second)
            && is_mostly_ascii(second)
            && second.len() >= 12
            && !second.contains(' ');

        cleaned = if is_featuring && !second.is_empty() && !second_is_junk {
            debug!("Feat pattern: keeping '{}'", second);
            second.to_string()
        } else if prefer_first {
            debug!("Separator: keeping first part '{}'", first);
            first.to_string()
        } else if prefer_first_for_lyrics {
            debug!("Separator: keeping first part '{}'", first);
            first.to_string()
        } else if !second.is_empty() && !second_is_junk {
            debug!("Separator: keeping second part '{}'", second);
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
    cleaned = cleaned.replace('（', "(").replace('）', ")");
    cleaned = RE_WHITESPACE.replace_all(&cleaned, " ").trim().to_string();

    cleaned
}

fn final_cleanup(text: &str, original: &str) -> String {
    let mut cleaned = text.to_string();

    // Handle Japanese full-width slash
    if cleaned.contains("／") {
        let parts: Vec<&str> = cleaned.split("／").collect();
        if parts.len() == 2 {
            let first = parts[0].trim();
            let second = parts[1].trim();
            let second_lower = second.to_lowercase();
            let is_tag_suffix = second_lower.contains("first take");
            let is_feat_suffix = second_lower.contains("feat") || second_lower.contains("ft.");
            let is_compact_suffix = !second.contains(' ') && second.chars().count() <= 6;
            let is_cover_suffix = second_lower.contains("歌ってみた") || second_lower.contains("cover");
            let has_spaced_slash = cleaned.contains(" ／ ");
            let both_cjk = contains_cjk(first) && contains_cjk(second);
            if !(both_cjk && !has_spaced_slash && !is_cover_suffix) {
                if (has_spaced_slash
                    || is_tag_suffix
                    || is_feat_suffix
                    || is_cover_suffix
                    || (is_compact_suffix && !both_cjk))
                    && !first.is_empty()
                    && first.len() > 2
                {
                    cleaned = first.to_string();
                }
            }
        }
    }

    if let Some((first, second)) = cleaned.split_once('／') {
        let second_lower = second.to_lowercase();
        let has_digits = second.chars().any(|c| c.is_ascii_digit());
        if (second_lower.contains("feat") || second_lower.contains("ft."))
            && (has_digits || !contains_cjk(second))
            && !first.trim().is_empty()
        {
            cleaned = first.trim().to_string();
        }
    }

    if let Some((first, second)) = cleaned.split_once(" / ") {
        if contains_cjk(first) && contains_cjk(second) {
            cleaned = first.trim().to_string();
        }
    }

    if original.contains("歌ってみた") {
        if let Some((first, _second)) = cleaned.split_once('／') {
            if !first.trim().is_empty() {
                cleaned = first.trim().to_string();
            }
        }
    }

    // Handle remaining full-width parentheses for obvious video tags
    if cleaned.contains('（') {
        let lower = cleaned.to_lowercase();
        if lower.contains("video") || lower.contains("mv") || lower.contains("official") {
            let result = RE_FULLWIDTH_PARENS.replace_all(&cleaned, "").trim().to_string();
            if !result.is_empty() && result.len() > 3 {
                cleaned = result;
            }
        }
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

    // Handle feat. suffix
    let result = RE_FEAT_SUFFIX.replace_all(&cleaned, "").trim().to_string();
    if !result.is_empty() && result.len() > 1 {
        cleaned = result;
    }

    // Remove feat/ft when it appears before parentheses
    let result = RE_FT_FEAT_BEFORE_PARENS
        .replace_all(&cleaned, " (")
        .trim()
        .to_string();
    if !result.is_empty() && result.len() > 1 {
        cleaned = result;
    }

    // Remove Official Video suffix
    let result = RE_OFFICIAL_VIDEO.replace_all(&cleaned, "").trim().to_string();
    if !result.is_empty() && result.len() > 1 {
        cleaned = result;
    }

    // Remove feat/ft parentheses unless it looks intentional in the title
    let mut keep_feat = false;
    if let Some(captures) = RE_FEAT_PARENS_CAPTURE.captures(&cleaned) {
        if let Some(feat_text) = captures.get(1) {
            let feat_text = feat_text.as_str();
            if feat_text.contains('.') {
                keep_feat = true;
            }
            if feat_text.contains('&') && contains_cjk(&cleaned) {
                keep_feat = true;
            }
        }
    }
    if original.to_lowercase().contains("[official video]") {
        keep_feat = true;
    }
    if !keep_feat {
        let result = RE_FEAT_PARENS.replace_all(&cleaned, "").trim().to_string();
        if !result.is_empty() && result.len() > 1 {
            cleaned = result;
        }
    }

    // Remove trailing lyrics tags after separators
    let result = RE_LYRICS_SUFFIX.replace_all(&cleaned, "").trim().to_string();
    if !result.is_empty() && result.len() > 1 {
        cleaned = result;
    }

    let lower = cleaned.to_lowercase();
    if lower.ends_with(" cover") && !lower.ends_with("japanese cover") {
        cleaned = cleaned[..cleaned.len() - 6].trim().to_string();
    }

    let result = RE_BRACKET_TAGS.replace_all(&cleaned, "").trim().to_string();
    if !result.is_empty() && result.len() > 1 {
        cleaned = result;
    }

    if original.to_lowercase().contains("lyrics") {
        if let Some((first, _second)) = original.split_once(" - ") {
            if first.chars().count() <= 20 {
                cleaned = first.trim().to_string();
            }
        }
    }

    let original_lower = original.to_lowercase();

    if original_lower.contains("english sub") && contains_cjk(&cleaned) {
        let cjk_only: String = cleaned
            .chars()
            .filter(|c| is_cjk_char(*c) || c.is_whitespace())
            .collect();
        if !cjk_only.trim().is_empty() {
            cleaned = cjk_only.trim().to_string();
        }
    }

    // Remove trailing ASCII translation when official tags were present
    if original_lower.contains("official") && contains_cjk(&cleaned) {
        if let Some((left, right)) = cleaned.rsplit_once(' ') {
            if right.chars().all(|c| c.is_ascii_alphabetic()) {
                cleaned = left.trim().to_string();
            }
        }
    }

    if original_lower.contains("歌ってみた") && cleaned.ends_with('を') {
        cleaned = cleaned.trim_end_matches('を').trim().to_string();
    }

    if original_lower.contains("arrange") {
        if let Some((first, _second)) = original.split_once('-') {
            cleaned = first.trim().to_string();
        }
        cleaned = RE_FULLWIDTH_PARENS.replace_all(&cleaned, "").trim().to_string();
        cleaned = RE_ASCII_PARENS.replace_all(&cleaned, "").trim().to_string();
    }

    // Normalize quoted title with suffix into title + (suffix)
    let normalize_quoted = |title: &str, suffix: &str| -> Option<String> {
        let suffix_text = suffix.trim();
        if suffix_text.is_empty() {
            return None;
        }
        if suffix_text.starts_with('(') || suffix_text.starts_with('[') {
            return Some(format!("{} {}", title.trim(), suffix_text));
        }
        Some(format!("{} ({})", title.trim(), suffix_text))
    };

    if let Some(captures) = RE_QUOTED_TITLE_WITH_SUFFIX.captures(&cleaned) {
        if let (Some(title), Some(suffix)) = (captures.get(1), captures.get(2)) {
            if let Some(normalized) = normalize_quoted(title.as_str(), suffix.as_str()) {
                cleaned = normalized;
            }
        }
    }

    if let Some(captures) = RE_DOUBLE_QUOTED_TITLE_WITH_SUFFIX.captures(&cleaned) {
        if let (Some(title), Some(suffix)) = (captures.get(1), captures.get(2)) {
            if let Some(normalized) = normalize_quoted(title.as_str(), suffix.as_str()) {
                cleaned = normalized;
            }
        }
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

    cleaned = cleaned.trim_start_matches('「').trim_end_matches('」').trim().to_string();
    cleaned = cleaned.replace('（', "(").replace('）', ")");
    if let Some(pos) = cleaned.find('(') {
        if pos > 0 && !cleaned[..pos].ends_with(' ') {
            cleaned = format!("{} {}", &cleaned[..pos], &cleaned[pos..]);
        }
    }
    cleaned = RE_WHITESPACE.replace_all(&cleaned, " ").trim().to_string();

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
