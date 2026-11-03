//! Track and artist name cleaning: a regex pass for video-style titles, and
//! one LLM call (any BYOK provider via `crate::llm`) for names regex can't fix.

mod regex;

use log::{debug, info};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

pub use regex::{clean_title, clean_track_name_sync, normalize_name};

use crate::llm::{self, store::AdhocKey, ChatRequest, Task, NOT_CONFIGURED};

/// One spelling per character. Players send some titles decomposed (バ as ハ
/// plus a combining mark) and others composed; they look the same but would
/// be two songs to the cache, LRCLIB and every lookup. Applied where names enter.
pub fn canonical(s: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    s.nfc().collect()
}

/// The names a lyrics database files a song under, as the LLM sees them.
#[derive(Clone, Debug, PartialEq)]
pub struct Names {
    pub artist: String,
    pub title: String,
}

const SYSTEM_PROMPT: &str = r#"You turn a music player's track title and artist into the names a lyrics database (LRCLIB, filled from Spotify and Apple Music metadata) files the song under.

- title: the song title only. Drop video packaging ("Official Video", "MV", "Lyrics", "【...】" tags, "| ..." descriptions), the artist name, and featuring credits. Keep version markers that name the recording, such as "Acoustic", "Live", "Remix", "Japanese Version". Keep a bilingual title like "雪国 - Snow Country" whole.
- artist: the main artist only, in the script they publish under (e.g. "Yorushika" -> "ヨルシカ"). Drop "Official", "VEVO", "- Topic" and channel suffixes.
- Never translate. If unsure, return the input unchanged.

Reply with JSON only: {"artist": "...", "title": "..."}"#;

/// LLM answers by (artist, title). Only successes are kept, so setting up a
/// provider later is not shadowed by a fallback.
static CACHE: LazyLock<Mutex<HashMap<(String, String), Names>>> = LazyLock::new(Default::default);

async fn llm_names_with_keys(artist: &str, title: &str, call_keys: &[AdhocKey]) -> Result<Names, String> {
    let key = (artist.to_string(), title.to_string());
    if let Some(names) = CACHE.lock().unwrap().get(&key) {
        return Ok(names.clone());
    }
    let target = llm::resolve(Task::Cleaning, call_keys)
        .ok_or_else(|| format!("{NOT_CONFIGURED} no provider is set up for cleaning"))?;
    let user = format!("artist: {artist}\ntitle: {title}");
    let request = ChatRequest {
        system: Some(SYSTEM_PROMPT),
        user: &user,
        temperature: Some(0.1),
        max_tokens: 2048,
        json: true,
    };
    let reply = llm::complete(&target, &request).await?;
    let names = parse_names(&reply, artist)
        .ok_or_else(|| format!("{} ({}): unparseable cleaning reply: {}", target.label, target.model, reply.chars().take(200).collect::<String>()))?;
    info!("LLM cleaning: '{}' by '{}' -> '{}' by '{}'", title, artist, names.title, names.artist);
    CACHE.lock().unwrap().insert(key, names.clone());
    Ok(names)
}

/// `{"artist": .., "title": ..}` from a reply, tolerating fences and chatter.
/// A missing artist keeps `artist`; a missing title is a failure.
fn parse_names(reply: &str, artist: &str) -> Option<Names> {
    let object = &reply[reply.find('{')?..=reply.rfind('}')?];
    let json: serde_json::Value = serde_json::from_str(object).ok()?;
    let field = |k: &str| json[k].as_str().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    Some(Names {
        title: field("title")?,
        artist: field("artist").unwrap_or_else(|| artist.to_string()),
    })
}

/// Title cleaning for the `clean_track_name_command`: LLM first, regex fallback.
pub async fn clean_track_name(track_name: &str) -> String {
    clean_track_name_with_keys(track_name, &[]).await
}

/// As `clean_track_name`, possibly with legacy keys from the current call
/// when no route is configured.
pub async fn clean_track_name_with_keys(track_name: &str, call_keys: &[AdhocKey]) -> String {
    match llm_names_with_keys("", track_name, call_keys).await {
        Ok(names) => names.title,
        Err(e) => {
            debug!("LLM cleaning unavailable, using regex: {}", e);
            clean_track_name_sync(track_name)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decomposed_and_composed_kana_are_one_spelling() {
        // The player sent バ as ハ + U+3099 while the queue sent U+30D0.
        let decomposed = "\u{30CF}\u{3099}\u{30A4}";
        let composed = "\u{30D0}\u{30A4}";
        assert_ne!(decomposed, composed);
        assert_eq!(canonical(decomposed), composed);
        assert_eq!(canonical("Snow Country"), "Snow Country");
    }

    #[test]
    fn cases_reported_by_the_llm_search_study() {
        // Each of these defeated the old cleaner.
        assert_eq!(clean_title("Shinunoga E-Wa", "Fujii Kaze"), "Shinunoga E-Wa");
        assert_eq!(clean_title("雪国 - Snow Country", "Yorushika"), "雪国 - Snow Country");
        assert_eq!(clean_title("夜に駆ける［Official Video］", "YOASOBI"), "夜に駆ける");
        assert_eq!(clean_title("IU(아이유) _ Blueming(블루밍)", "IU"), "Blueming (블루밍)");
    }

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
    fn never_splits_on_a_dash_inside_brackets() {
        // Seen in a real LRCLIB result list: this used to become "February 5, 2001)".
        let cleaned = clean_track_name_sync("Hello Love (Buddha Remastered - February 5, 2001)");
        assert!(cleaned.starts_with("Hello Love"), "got '{}'", cleaned);
        // A dash outside brackets still splits.
        assert_eq!(clean_track_name_sync("Song Title - YouTube Music"), "Song Title");
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

        for (input, expected) in test_cases {
            assert_eq!(clean_track_name_sync(input), expected, "input '{}'", input);
        }
    }

    #[test]
    fn clean_titles_pass_through() {
        // YouTube Music titles are already clean; a hyphenated word is not a separator.
        for title in ["Anti-Hero", "Spider-Man", "雪国", "Video Games", "Television / So Far So Good"] {
            assert_eq!(clean_track_name_sync(title), title);
        }
    }

    #[test]
    fn with_an_artist_only_its_part_is_split_off() {
        let cases = [
            // Bilingual titles survive whole (real log: 'Hole In The Heart' lost its native half).
            ("雪国 - Snow Country", "Yorushika", "雪国 - Snow Country"),
            ("心に穴が空いた - Hole In The Heart", "Yorushika", "心に穴が空いた - Hole In The Heart"),
            ("私じゃなかったんだね。 - Watashijyanakattandane.", "Aimyon", "私じゃなかったんだね。 - Watashijyanakattandane."),
            ("Anti-Hero", "Taylor Swift", "Anti-Hero"),
            // A part naming the artist goes, whichever side, across case and width.
            ("Daoko - SYNC (Official Video)", "DAOKO", "SYNC"),
            ("ＤＡＯＫＯ - SYNC", "Daoko", "SYNC"),
            ("coffee - beabadoobee", "beabadoobee", "coffee"),
            ("Tsumiki feat. KAFU - フォニイ", "Tsumiki", "フォニイ"),
            ("ヨルシカ - 左右盲（OFFICIAL VIDEO）", "ヨルシカ / n-buna Official", "左右盲"),
            ("ASA-嫌いになれない【Official Music Video】", "ASA", "嫌いになれない"),
            // Packaging still goes when no part names the artist.
            ("potsu - just friends (Official Video)", "Lofi Girl", "potsu - just friends"),
        ];
        for (title, artist, expected) in cases {
            assert_eq!(clean_title(title, artist), expected, "'{}' by '{}'", title, artist);
        }
    }

    #[test]
    fn normalizes_names_lightly() {
        assert_eq!(normalize_name("ＤＡＯＫＯ"), "daoko");
        assert_eq!(normalize_name("  Hole  In The Heart!"), "hole in the heart");
        assert_eq!(normalize_name("心に穴が空いた"), "心に穴が空いた");
    }

    #[test]
    fn parses_names_reply() {
        let reply = "```json\n{\"artist\": \" なとり \", \"title\": \"Overdose\"}\n```";
        let names = parse_names(reply, "Natori").unwrap();
        assert_eq!((names.artist.as_str(), names.title.as_str()), ("なとり", "Overdose"));

        let pretty = "Sure:\n{\n  \"title\": \"雪国 - Snow Country\"\n}";
        assert_eq!(parse_names(pretty, "Yorushika").unwrap().artist, "Yorushika");

        assert!(parse_names("no json here", "a").is_none());
        assert!(parse_names("{\"title\": \"\", \"artist\": \"a\"}", "a").is_none());
    }
}
