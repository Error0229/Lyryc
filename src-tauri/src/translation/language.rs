//! Language detection and name utilities

use log::debug;

use super::types::LyricLine;

/// Fallback language detection based on character and pattern analysis
pub fn detect_language_fallback(lyrics: &[LyricLine]) -> String {
    let sample_text = lyrics
        .iter()
        .take(3)
        .map(|line| line.text.clone())
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();

    if sample_text.is_empty() {
        return "en".to_string();
    }

    // Count character types
    let hiragana_count = sample_text
        .chars()
        .filter(|c| matches!(c, '\u{3040}'..='\u{309f}'))
        .count();
    let katakana_count = sample_text
        .chars()
        .filter(|c| matches!(c, '\u{30a0}'..='\u{30ff}'))
        .count();
    let kanji_count = sample_text
        .chars()
        .filter(|c| matches!(c, '\u{4e00}'..='\u{9faf}'))
        .count();
    let total_chars = sample_text.chars().count();

    // Japanese detection: must have kana (hiragana or katakana)
    if hiragana_count > 0 || katakana_count > 0 {
        let kana_ratio = (hiragana_count + katakana_count) as f32 / total_chars as f32;
        if kana_ratio > 0.1 {
            return "ja".to_string();
        }
    }

    // Korean detection
    if sample_text
        .chars()
        .any(|c| matches!(c, '\u{ac00}'..='\u{d7af}'))
    {
        return "ko".to_string();
    }

    // Chinese detection: has CJK characters but very little or no kana
    if kanji_count > 0 {
        let kana_ratio = (hiragana_count + katakana_count) as f32 / total_chars as f32;
        let kanji_ratio = kanji_count as f32 / total_chars as f32;
        if kanji_ratio > 0.3 && kana_ratio < 0.1 {
            // Check for Traditional Chinese specific characters
            let has_traditional = sample_text.chars().any(|c| {
                matches!(
                    c,
                    '這' | '樣'
                        | '時'
                        | '會'
                        | '過'
                        | '應'
                        | '來'
                        | '對'
                        | '於'
                        | '個'
                        | '電'
                        | '學'
                        | '國'
                        | '說'
                        | '話'
                        | '語'
                        | '見'
                        | '間'
                        | '開'
                        | '關'
                )
            });

            if has_traditional {
                return "zh-tw".to_string();
            } else {
                return "zh".to_string();
            }
        }
    }

    // Russian detection
    if sample_text
        .chars()
        .any(|c| matches!(c, '\u{0400}'..='\u{04ff}'))
    {
        return "ru".to_string();
    }

    // Arabic detection
    if sample_text
        .chars()
        .any(|c| matches!(c, '\u{0600}'..='\u{06ff}'))
    {
        return "ar".to_string();
    }

    // European language detection with common word patterns
    let english_patterns = [
        " the ", " and ", " is ", " are ", " to ", " of ", " in ", " that ",
    ];
    let spanish_patterns = [
        " el ", " la ", " de ", " que ", " en ", " un ", " se ", " no ",
    ];
    let french_patterns = [
        " le ", " de ", " et ", " à ", " un ", " ce ", " que ", " du ",
    ];
    let german_patterns = [
        " der ", " die ", " das ", " und ", " ich ", " ist ", " du ", " sie ",
    ];
    let portuguese_patterns = [
        " de ", " que ", " não ", " um ", " se ", " da ", " do ", " em ",
    ];
    let italian_patterns = [
        " il ", " di ", " che ", " non ", " un ", " la ", " del ", " da ",
    ];

    if english_patterns.iter().any(|&p| sample_text.contains(p)) {
        return "en".to_string();
    }
    if spanish_patterns.iter().any(|&p| sample_text.contains(p)) {
        return "es".to_string();
    }
    if french_patterns.iter().any(|&p| sample_text.contains(p)) {
        return "fr".to_string();
    }
    if german_patterns.iter().any(|&p| sample_text.contains(p)) {
        return "de".to_string();
    }
    if portuguese_patterns.iter().any(|&p| sample_text.contains(p)) {
        return "pt".to_string();
    }
    if italian_patterns.iter().any(|&p| sample_text.contains(p)) {
        return "it".to_string();
    }

    // Default to English for undetected Latin scripts
    debug!("Language detection fallback to English");
    "en".to_string()
}

/// Get the English name for a language code
pub fn get_language_name(code: &str) -> &str {
    match code.to_lowercase().as_str() {
        "auto" => "Auto-detect",
        "en" => "English",
        "es" => "Spanish",
        "fr" => "French",
        "de" => "German",
        "it" => "Italian",
        "pt" => "Portuguese",
        "ru" => "Russian",
        "ja" => "Japanese",
        "ko" => "Korean",
        "zh" | "zh-cn" => "Chinese (Simplified)",
        "zh-tw" => "Chinese (Traditional)",
        "zh-hk" => "Chinese (Hong Kong)",
        "ar" => "Arabic",
        "hi" => "Hindi",
        "th" => "Thai",
        "vi" => "Vietnamese",
        "id" => "Indonesian",
        "ms" => "Malay",
        "tl" => "Tagalog",
        "nl" => "Dutch",
        "pl" => "Polish",
        "tr" => "Turkish",
        "sv" => "Swedish",
        "da" => "Danish",
        "no" => "Norwegian",
        "fi" => "Finnish",
        "el" => "Greek",
        "he" => "Hebrew",
        "cs" => "Czech",
        "hu" => "Hungarian",
        "ro" => "Romanian",
        "uk" => "Ukrainian",
        _ => code,
    }
}

/// Get the native name for a language code
#[allow(dead_code)]
pub fn get_native_language_name(code: &str) -> &str {
    match code.to_lowercase().as_str() {
        "en" => "English",
        "es" => "Español",
        "fr" => "Français",
        "de" => "Deutsch",
        "it" => "Italiano",
        "pt" => "Português",
        "ru" => "Русский",
        "ja" => "日本語",
        "ko" => "한국어",
        "zh" | "zh-cn" => "简体中文",
        "zh-tw" => "繁體中文",
        "zh-hk" => "繁體中文",
        "ar" => "العربية",
        "hi" => "हिन्दी",
        "th" => "ไทย",
        "vi" => "Tiếng Việt",
        _ => get_language_name(code),
    }
}
