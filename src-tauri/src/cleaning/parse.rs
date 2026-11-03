//! Parsing of LLM cleaning replies

use log::warn;
use serde_json::Value;

use super::prompt::clean_markdown_formatting;
use super::types::{ArtistTrackCleaningResult, CleaningResult};

/// First `cleaned_title` from the reply: a single (possibly pretty-printed) JSON
/// object, or one object per line.
pub fn parse_track_cleaning_response(response_text: &str) -> Result<String, String> {
    let cleaned_response = clean_markdown_formatting(response_text);
    let text = cleaned_response.trim();

    // A pretty-printed object spans lines; if the outermost {...} isn't one
    // object (one-per-line output), it fails to parse and the lines get a turn.
    let whole = match (text.find('{'), text.rfind('}')) {
        (Some(start), Some(end)) if start < end => Some(&text[start..=end]),
        _ => None,
    };
    let title = whole
        .into_iter()
        .chain(text.lines().map(str::trim))
        .filter_map(|candidate| serde_json::from_str::<Value>(candidate).ok())
        .find_map(|parsed| {
            let title = parsed["cleaned_title"].as_str()?.trim();
            (!title.is_empty()).then(|| title.to_string())
        });

    title.ok_or_else(|| {
        warn!("Unparseable cleaning reply: {}", text.chars().take(300).collect::<String>());
        "Could not parse valid cleaned title from LLM response".to_string()
    })
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|s| s.as_str())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn cleaning_result(obj: &Value, field: &str) -> Result<CleaningResult, String> {
    if !obj.is_object() {
        return Err(format!("Missing '{field}' field in response"));
    }
    Ok(CleaningResult {
        cleaned_title: obj["cleaned_title"]
            .as_str()
            .ok_or_else(|| format!("Missing cleaned_title in {field}"))?
            .to_string(),
        cleaned_artist: None,
        cleaned_artists: strings(&obj["cleaned_artists"]),
        kept_markers: strings(&obj["kept_markers"]),
        removed_context: strings(&obj["removed_context"]),
        confidence: obj["confidence"].as_f64().unwrap_or(0.8),
        notes: obj["notes"].as_str().map(str::to_string),
    })
}

/// Parse the combined `{ "artist": {...}, "track": {...} }` reply. Tolerates
/// code fences or chatter around the object.
pub fn parse_artist_track_cleaning_response(
    response_text: &str,
) -> Result<ArtistTrackCleaningResult, String> {
    let text = response_text.trim();
    let object = match (text.find('{'), text.rfind('}')) {
        (Some(start), Some(end)) if start < end => &text[start..=end],
        _ => text,
    };

    let json: Value = serde_json::from_str(object).map_err(|e| {
        warn!(
            "Failed to parse artist/track cleaning response as JSON: {}",
            e
        );
        warn!("Response was: {}", text);
        format!("Failed to parse cleaning response: {}", e)
    })?;

    let mut track = cleaning_result(&json["track"], "track")?;
    track.cleaned_artists.clear();
    Ok(ArtistTrackCleaningResult {
        artist: cleaning_result(&json["artist"], "artist")?,
        track,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_track_lines() {
        let reply = "```json\n{\"cleaned_title\": \" 火星人 \", \"confidence\": 0.9}\n```";
        assert_eq!(parse_track_cleaning_response(reply).unwrap(), "火星人");
        assert!(parse_track_cleaning_response("no json here").is_err());
        assert!(parse_track_cleaning_response("{\"cleaned_title\": \"\"}").is_err());
    }

    #[test]
    fn parses_pretty_printed_and_per_line_track_replies() {
        let pretty = "{\n  \"cleaned_title\": \"雪国 - Snow Country\",\n  \"confidence\": 0.9\n}";
        assert_eq!(parse_track_cleaning_response(pretty).unwrap(), "雪国 - Snow Country");

        let per_line = "{\"cleaned_title\": \"A\"}\n{\"cleaned_title\": \"B\"}";
        assert_eq!(parse_track_cleaning_response(per_line).unwrap(), "A");
    }

    #[test]
    fn parses_artist_track_object() {
        let reply = r#"Here you go:
```json
{"artist":{"cleaned_title":"なとり","cleaned_artists":["なとり"],"confidence":0.9},
 "track":{"cleaned_title":"Overdose","kept_markers":["Live"],"cleaned_artists":["x"]}}
```"#;
        let r = parse_artist_track_cleaning_response(reply).unwrap();
        assert_eq!(r.artist.cleaned_title, "なとり");
        assert_eq!(r.artist.cleaned_artists, vec!["なとり"]);
        assert_eq!(r.track.cleaned_title, "Overdose");
        assert_eq!(r.track.kept_markers, vec!["Live"]);
        assert!(r.track.cleaned_artists.is_empty());
        assert!((r.track.confidence - 0.8).abs() < 1e-9);
    }

    #[test]
    fn rejects_incomplete_artist_track_object() {
        assert!(
            parse_artist_track_cleaning_response(r#"{"artist":{"cleaned_title":"a"}}"#)
                .unwrap_err()
                .contains("track")
        );
        assert!(parse_artist_track_cleaning_response("nope").is_err());
    }
}
