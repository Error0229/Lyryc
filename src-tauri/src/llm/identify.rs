//! Song identification through the provider's web search: the raw title and
//! artist a browser player reports go in, canonical names for the normal
//! LRCLIB / third-party lookup come out. The model never supplies lyrics.
//! Findings and where to call this: `docs/research/llm-search/REPORT.md`.

use log::{debug, warn};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, OnceLock};

use super::{
    complete_with, resolve, ChatRequest, Completion, JsonSchema, LlmError, Target, Task, Tools,
};

/// Below this the model is guessing, and a wrong name means wrong lyrics.
pub const MIN_CONFIDENCE: f64 = 0.6;

const SYSTEM: &str = "You identify songs from what a web music player displays. \
The title and artist are raw: they may carry video tags (【MV】, (Official Video), 「」), \
a channel or label name instead of the artist, a translated or romanized title, or a native \
and a translated title together. Search the web for the released recording, then give its \
title and primary artist exactly as written on the original release, in the original script \
(for example 雪国 / ヨルシカ, 晴天 / 周杰倫, 봄날 / 방탄소년단), plus the common romanized or \
English form when one exists, else null. Use the duration to tell versions apart; duration_s \
is the length of the release you name. Set found=false if the input is not a single song \
(a stream, mix or podcast) or the search gives no evidence. confidence is the probability that \
title_native and artist_native are both right. instrumental: true only when the recording \
has no sung or spoken words at all (a piano piece, a soundtrack cue, a lofi beat); false when \
it has vocals or you are not sure. cover_of: when this recording is a cover of another \
artist's song (a 'covered by' or 歌ってみた upload, a tribute), that original artist in the \
original script, and title_native is the original song's title; null for an original. \
evidence_urls: pages that confirm it. Never include lyrics.";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SongIdentity {
    pub title_native: String,
    pub title_romanized: Option<String>,
    pub artist_native: String,
    pub artist_romanized: Option<String>,
    pub album: Option<String>,
    pub duration_s: Option<f64>,
    pub isrc: Option<String>,
    /// No sung or spoken words: there are no lyrics to find.
    pub instrumental: bool,
    /// When this recording is a cover: the original artist, native script.
    /// The words are theirs; the timing is not (another arrangement).
    pub cover_of: Option<String>,
    pub confidence: f64,
    /// The provider's grounding sources first, then URLs the model named.
    pub evidence_urls: Vec<String>,
}

impl SongIdentity {
    /// (title, artist) pairs to try against LRCLIB `/api/get`, best first.
    pub fn queries(&self) -> Vec<(String, String)> {
        let titles = [Some(&self.title_native), self.title_romanized.as_ref()];
        let artists = [Some(&self.artist_native), self.artist_romanized.as_ref()];
        let mut out: Vec<(String, String)> = Vec::new();
        for title in titles.into_iter().flatten() {
            for artist in artists.into_iter().flatten() {
                let pair = (title.clone(), artist.clone());
                if !out.contains(&pair) {
                    out.push(pair);
                }
            }
        }
        out
    }
}

/// Strict-mode compatible: every property required, nulls for unknowns.
pub fn schema() -> &'static Value {
    static SCHEMA: OnceLock<Value> = OnceLock::new();
    SCHEMA.get_or_init(|| {
        let text = json!({"type": "string"});
        let opt_text = json!({"type": ["string", "null"]});
        json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "found": {"type": "boolean"},
                "title_native": text,
                "title_romanized": opt_text,
                "artist_native": text,
                "artist_romanized": opt_text,
                "album": opt_text,
                "duration_s": {"type": ["number", "null"]},
                "isrc": opt_text,
                "instrumental": {"type": "boolean"},
                "cover_of": opt_text,
                "confidence": {"type": "number"},
                "evidence_urls": {"type": "array", "items": text},
            },
            "required": [
                "found", "title_native", "title_romanized", "artist_native",
                "artist_romanized", "album", "duration_s", "isrc", "instrumental", "cover_of", "confidence",
                "evidence_urls",
            ],
        })
    })
}

pub fn prompt(raw_title: &str, raw_artist: &str, duration: Option<f64>) -> String {
    let duration = duration.map_or("unknown".to_string(), |d| format!("{d:.0} s"));
    format!("Title: {raw_title}\nArtist: {raw_artist}\nDuration: {duration}")
}

pub fn request(user: &str) -> ChatRequest<'_> {
    ChatRequest {
        system: Some(SYSTEM),
        user,
        // Gemini 3 and reasoning models want their default temperature.
        temperature: None,
        max_tokens: 4096,
        json: true,
    }
}

pub fn tools(web_search: bool) -> Tools<'static> {
    Tools {
        web_search,
        schema: Some(JsonSchema {
            name: "song_identity",
            schema: schema(),
        }),
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Reply {
    found: bool,
    title_native: String,
    title_romanized: Option<String>,
    artist_native: String,
    artist_romanized: Option<String>,
    album: Option<String>,
    duration_s: Option<f64>,
    isrc: Option<String>,
    instrumental: bool,
    cover_of: Option<String>,
    confidence: f64,
    evidence_urls: Vec<String>,
}

/// The identity in a reply, `None` when the model found nothing it is sure of.
/// Prompt-enforced schemas (no native support next to search) may come back
/// wrapped in prose or a code fence, so read the outermost object.
pub fn parse(reply: &Completion) -> Result<Option<SongIdentity>, String> {
    let text = &reply.text;
    let object = match (text.find('{'), text.rfind('}')) {
        (Some(start), Some(end)) if start < end => &text[start..=end],
        _ => return Err(format!("no JSON object in reply: {}", preview(text))),
    };
    let r: Reply = serde_json::from_str(object)
        .map_err(|e| format!("unreadable identity ({e}): {}", preview(text)))?;
    let (Some(title), Some(artist)) = (
        non_empty(Some(r.title_native)),
        non_empty(Some(r.artist_native)),
    ) else {
        return Ok(None);
    };
    if !r.found || r.confidence < MIN_CONFIDENCE {
        debug!(
            "Identification unsure ({:.2}): {title} / {artist}",
            r.confidence
        );
        return Ok(None);
    }
    let mut evidence_urls = reply.sources.clone();
    for url in r.evidence_urls {
        if url.starts_with("http") && !evidence_urls.contains(&url) {
            evidence_urls.push(url);
        }
    }
    Ok(Some(SongIdentity {
        title_native: title,
        title_romanized: non_empty(r.title_romanized),
        artist_native: artist,
        artist_romanized: non_empty(r.artist_romanized),
        album: non_empty(r.album),
        duration_s: r.duration_s.filter(|d| (1.0..=3600.0).contains(d)),
        isrc: non_empty(r.isrc),
        instrumental: r.instrumental,
        cover_of: non_empty(r.cover_of),
        confidence: r.confidence.clamp(0.0, 1.0),
        evidence_urls,
    }))
}

fn non_empty(s: Option<String>) -> Option<String> {
    let s = s?.trim().to_string();
    (!s.is_empty() && s != "null").then_some(s)
}

fn preview(text: &str) -> String {
    text.chars().take(120).collect()
}

/// One identification against an explicit target. With `web_search` the
/// provider's search tool is required; a provider without one is an error.
pub async fn identify_with(
    target: &Target,
    raw_title: &str,
    raw_artist: &str,
    duration: Option<f64>,
    web_search: bool,
) -> Result<Option<SongIdentity>, LlmError> {
    let user = prompt(raw_title, raw_artist, duration);
    let reply = complete_with(target, &request(&user), tools(web_search)).await?;
    parse(&reply).map_err(|message| LlmError {
        provider: target.label.clone(),
        model: target.model.clone(),
        status: None,
        message,
    })
}

/// How long identification may take (including its turn at the provider);
/// searching the web takes longer.
const LIMIT: std::time::Duration = std::time::Duration::from_secs(20);
const WEB_SEARCH_LIMIT: std::time::Duration = std::time::Duration::from_secs(60);

/// Answers per raw (title, artist), including "unsure", for this session: a
/// song that missed once will miss again, and each call costs the user.
/// By (raw title, raw artist, web search): a search-off "unsure" must not
/// shadow the search-on attempt.
type Answers = HashMap<(String, String, bool), Option<SongIdentity>>;
static ANSWERS: LazyLock<Mutex<Answers>> = LazyLock::new(Default::default);

/// Drop the remembered answers for a song: the lyrics they led to were wrong.
pub fn forget(raw_title: &str, raw_artist: &str) {
    ANSWERS.lock().unwrap().retain(|(title, artist, _), _| title != raw_title || artist != raw_artist);
}

/// Canonical names for a raw player title, using the cleaning route. `None`
/// when no provider is set up, the call failed, or the model is unsure.
///
/// The lookup calls this with `web_search: false`: the study measured 25/26
/// LRCLIB hits that way, and free-tier keys get 429 on every grounded call.
pub async fn identify_song(
    raw_title: &str,
    raw_artist: &str,
    duration: Option<f64>,
    web_search: bool,
) -> Option<SongIdentity> {
    let key = (raw_title.to_string(), raw_artist.to_string(), web_search);
    if let Some(answer) = ANSWERS.lock().unwrap().get(&key) {
        return answer.clone();
    }
    let target = resolve(Task::Cleaning, &[])?;
    // A song is waiting on this: a provider that takes longer is better skipped.
    let limit = if web_search { WEB_SEARCH_LIMIT } else { LIMIT };
    let call = identify_with(&target, raw_title, raw_artist, duration, web_search);
    let answer = match tokio::time::timeout(limit, call).await {
        Ok(Ok(found)) => found,
        Ok(Err(e)) => {
            // Not remembered: a network or quota failure may pass.
            warn!("Song identification failed: {e}");
            return None;
        }
        Err(_) => {
            warn!("Song identification took over {}s; going on without it", limit.as_secs());
            return None;
        }
    };
    ANSWERS.lock().unwrap().insert(key, answer.clone());
    answer
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(text: &str, sources: &[&str]) -> Completion {
        Completion {
            text: text.to_string(),
            sources: sources.iter().map(|s| s.to_string()).collect(),
            searches: 1,
        }
    }

    const SNOW: &str = r#"{"found":true,"title_native":"雪国","title_romanized":"Yukiguni",
        "artist_native":"ヨルシカ","artist_romanized":"Yorushika","album":"幻燈",
        "duration_s":288,"isrc":null,"confidence":0.93,
        "evidence_urls":["https://example.org/a","not a url"]}"#;

    #[test]
    fn parses_an_identity_and_puts_grounding_first() {
        let id = parse(&reply(SNOW, &["https://g/1"])).unwrap().unwrap();
        assert_eq!(id.title_native, "雪国");
        assert_eq!(id.artist_romanized.as_deref(), Some("Yorushika"));
        assert_eq!(id.duration_s, Some(288.0));
        assert_eq!(id.isrc, None);
        assert!(!id.instrumental, "absent means vocals");
        assert_eq!(id.cover_of, None, "absent means the original");
        assert_eq!(id.evidence_urls, ["https://g/1", "https://example.org/a"]);
    }

    #[test]
    fn reads_json_wrapped_in_prose_or_fences() {
        let fenced = format!("Here you go:\n```json\n{SNOW}\n```");
        assert!(parse(&reply(&fenced, &[])).unwrap().is_some());
        assert!(parse(&reply("I could not find it.", &[])).is_err());
    }

    #[test]
    fn unsure_or_not_found_is_none() {
        let low = SNOW.replace("0.93", "0.4");
        assert!(parse(&reply(&low, &[])).unwrap().is_none());
        let not_found = SNOW.replace("\"found\":true", "\"found\":false");
        assert!(parse(&reply(&not_found, &[])).unwrap().is_none());
        let blank = r#"{"found":true,"title_native":" ","artist_native":"x","confidence":1}"#;
        assert!(parse(&reply(blank, &[])).unwrap().is_none());
    }

    #[test]
    fn a_cover_names_the_original_artist() {
        let cover = SNOW.replace("\"isrc\":null", "\"isrc\":null,\"cover_of\":\"RADWIMPS\"");
        let id = parse(&reply(&cover, &[])).unwrap().unwrap();
        assert_eq!(id.cover_of.as_deref(), Some("RADWIMPS"));
        let blank = SNOW.replace("\"isrc\":null", "\"isrc\":null,\"cover_of\":\" \"");
        assert_eq!(parse(&reply(&blank, &[])).unwrap().unwrap().cover_of, None);
    }

    #[test]
    fn missing_optional_fields_and_bad_duration_are_tolerated() {
        let minimal = r#"{"found":true,"title_native":"SYNC","artist_native":"DAOKO",
            "title_romanized":"","duration_s":0,"confidence":0.8}"#;
        let id = parse(&reply(minimal, &[])).unwrap().unwrap();
        assert_eq!(id.title_romanized, None);
        assert_eq!(id.duration_s, None);
        assert!(id.evidence_urls.is_empty());
    }

    #[test]
    fn queries_try_native_first_without_duplicates() {
        let id = parse(&reply(SNOW, &[])).unwrap().unwrap();
        assert_eq!(
            id.queries(),
            [
                ("雪国".into(), "ヨルシカ".into()),
                ("雪国".into(), "Yorushika".into()),
                ("Yukiguni".into(), "ヨルシカ".into()),
                ("Yukiguni".into(), "Yorushika".into()),
            ]
        );
        let mut same = id.clone();
        same.title_romanized = None;
        same.artist_romanized = Some("ヨルシカ".into());
        assert_eq!(same.queries(), [("雪国".into(), "ヨルシカ".into())]);
    }

    #[test]
    fn schema_is_strict_mode_compatible() {
        let s = schema();
        assert_eq!(s["additionalProperties"], false);
        let props = s["properties"].as_object().unwrap();
        let required: Vec<&str> = s["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(props.len(), required.len());
        assert!(props.keys().all(|k| required.contains(&k.as_str())));
    }

    #[test]
    fn prompt_carries_raw_names_untouched() {
        let p = prompt(
            "【MV】私じゃなかったんだね。(Official Video)",
            "マカロニえんぴつ",
            Some(244.4),
        );
        assert_eq!(
            p,
            "Title: 【MV】私じゃなかったんだね。(Official Video)\nArtist: マカロニえんぴつ\nDuration: 244 s"
        );
        assert!(prompt("a", "b", None).ends_with("Duration: unknown"));
    }
}
