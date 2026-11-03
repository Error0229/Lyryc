//! Reading InnerTube answers. Shapes follow ytmusicapi (MIT, sigma67):
//! `parsers/watch.py` `get_tab_browse_ids`, `mixins/browsing.py` `get_lyrics`,
//! and the paths in `navigation.py`. Err means the answer no longer has the
//! shape we know; Ok(None) is YouTube Music saying it has no lyrics.

use super::YtLyrics;
use anyhow::{anyhow, Result};
use serde_json::Value;

const LYRICS_PAGE: &str = "MUSIC_PAGE_TYPE_TRACK_LYRICS";

/// `next`: the lyrics tab's `browseId` (`MPLYt...`). The tab is there but
/// `unselectable` when the track has no lyrics.
pub fn browse_id(next: &Value) -> Result<Option<String>> {
    let tabs = next
        .pointer("/contents/singleColumnMusicWatchNextResultsRenderer/tabbedRenderer/watchNextTabbedResultsRenderer/tabs")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("next: no watch tabs"))?;
    let tab = tabs.iter().map(|t| &t["tabRenderer"]).find(|t| {
        t.pointer("/endpoint/browseEndpoint/browseEndpointContextSupportedConfigs/browseEndpointContextMusicConfig/pageType")
            .and_then(Value::as_str)
            == Some(LYRICS_PAGE)
    });
    Ok(tab
        .filter(|t| t.get("unselectable").is_none())
        .and_then(|t| t.pointer("/endpoint/browseEndpoint/browseId")?.as_str())
        .filter(|id| !id.is_empty())
        .map(str::to_string))
}

/// `browse`: the mobile client's timed model, or the web client's text shelf.
pub fn lyrics(browse: &Value) -> Result<Option<YtLyrics>> {
    let contents = &browse["contents"];
    if let Some(data) = contents.pointer("/elementRenderer/newElement/type/componentType/model/timedLyricsModel/lyricsData") {
        return Ok(timed(data));
    }
    if let Some(shelves) = contents.pointer("/sectionListRenderer/contents") {
        // A message renderer instead of the shelf: "Lyrics not available".
        let shelf = &shelves[0]["musicDescriptionShelfRenderer"];
        let source = text_run(&shelf["footer"]);
        let lines = text_run(&shelf["description"]).map(|t| t.lines().map(|l| (None, l.to_string())).collect());
        return Ok(lines.and_then(|lines| finish(lines, false, source)));
    }
    Err(anyhow!("browse: unknown lyrics shape"))
}

/// Timed only if every line has a cue, as ytmusicapi decides; else plain lines.
fn timed(data: &Value) -> Option<YtLyrics> {
    let raw = data["timedLyricsData"].as_array()?;
    let start = |l: &Value| ms(&l["cueRange"]["startTimeMilliseconds"]).map(|ms| ms / 1000.0);
    let all_cued = raw.iter().all(|l| start(l).is_some());
    let lines = raw
        .iter()
        .map(|l| (start(l).filter(|_| all_cued), l["lyricLine"].as_str().unwrap_or("").to_string()))
        .collect();
    finish(lines, all_cued, data["sourceMessage"].as_str().map(str::to_string))
}

/// Drop blank lines; nothing left is no lyrics.
fn finish(lines: Vec<(Option<f64>, String)>, timed: bool, source: Option<String>) -> Option<YtLyrics> {
    let lines: Vec<_> = lines
        .into_iter()
        .map(|(t, s)| (t, s.trim().to_string()))
        .filter(|(_, s)| !s.is_empty())
        .collect();
    (!lines.is_empty()).then_some(YtLyrics { lines, timed, source })
}

fn text_run(v: &Value) -> Option<String> {
    v.pointer("/runs/0/text")?.as_str().map(str::to_string)
}

/// InnerTube sends milliseconds as strings ("1000"); accept numbers too.
fn ms(v: &Value) -> Option<f64> {
    let n = match v {
        Value::String(s) => s.parse().ok()?,
        v => v.as_f64()?,
    };
    (n >= 0.0 && f64::is_finite(n)).then_some(n)
}

/// The 11-character id of a `music.youtube.com/watch?v=...` URL. Only YouTube
/// Music URLs: elsewhere the id is a video, not a track with a lyrics tab.
pub fn video_id_from_url(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"))?;
    let (host, path) = rest.split_once('/')?;
    if !host.eq_ignore_ascii_case("music.youtube.com") {
        return None;
    }
    let query = path.strip_prefix("watch?")?.split('#').next()?;
    let id = query.split('&').find_map(|kv| kv.strip_prefix("v="))?;
    is_video_id(id).then(|| id.to_string())
}

pub fn is_video_id(id: &str) -> bool {
    id.len() == 11 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn next_with(lyrics_tab: Value) -> Value {
        let page = |t: &str| json!({"browseEndpointContextMusicConfig": {"pageType": t}});
        json!({"contents": {"singleColumnMusicWatchNextResultsRenderer": {"tabbedRenderer": {
            "watchNextTabbedResultsRenderer": {"tabs": [
                {"tabRenderer": {"title": "Up next", "content": {}}},
                {"tabRenderer": lyrics_tab},
                {"tabRenderer": {"title": "Related", "endpoint": {"browseEndpoint": {
                    "browseId": "MPTRt_placeholder", "browseEndpointContextSupportedConfigs": page("MUSIC_PAGE_TYPE_TRACK_RELATED")}}}}
            ]}}}}})
    }

    fn lyrics_tab(unselectable: bool) -> Value {
        let mut tab = json!({"title": "Lyrics", "endpoint": {"browseEndpoint": {
            "browseId": "MPLYt_placeholder",
            "browseEndpointContextSupportedConfigs": {"browseEndpointContextMusicConfig": {"pageType": LYRICS_PAGE}}}}});
        if unselectable {
            tab["unselectable"] = json!(true);
        }
        tab
    }

    #[test]
    fn next_gives_the_lyrics_browse_id() {
        assert_eq!(browse_id(&next_with(lyrics_tab(false))).unwrap().as_deref(), Some("MPLYt_placeholder"));
        assert_eq!(browse_id(&next_with(lyrics_tab(true))).unwrap(), None, "unselectable: no lyrics");
        assert_eq!(browse_id(&next_with(json!({"title": "Lyrics"}))).unwrap(), None);
        assert!(browse_id(&json!({"contents": {}})).is_err(), "a changed shape is a failure");
    }

    fn mobile(lines: Value) -> Value {
        json!({"contents": {"elementRenderer": {"newElement": {"type": {"componentType": {"model": {
            "timedLyricsModel": {"lyricsData": {"timedLyricsData": lines, "sourceMessage": "Source: Test"}}}}}}}}})
    }

    fn cue(text: &str, start: &str, end: &str) -> Value {
        json!({"lyricLine": text, "cueRange": {"startTimeMilliseconds": start, "endTimeMilliseconds": end, "metadata": {"id": "1"}}})
    }

    #[test]
    fn mobile_browse_gives_timed_lines() {
        let got = lyrics(&mobile(json!([cue("line one", "1000", "2000"), cue("", "2000", "2500"), cue("line two", "2500", "4000")])))
            .unwrap()
            .unwrap();
        assert!(got.timed);
        assert_eq!(got.source.as_deref(), Some("Source: Test"));
        assert_eq!(got.lines, vec![(Some(1.0), "line one".to_string()), (Some(2.5), "line two".to_string())]);
    }

    #[test]
    fn a_missing_cue_makes_the_whole_list_plain() {
        let got = lyrics(&mobile(json!([cue("line one", "1000", "2000"), {"lyricLine": "line two"}]))).unwrap().unwrap();
        assert!(!got.timed);
        assert_eq!(got.lines, vec![(None, "line one".to_string()), (None, "line two".to_string())]);
    }

    #[test]
    fn web_browse_gives_plain_text_and_source() {
        let web = json!({"contents": {"sectionListRenderer": {"contents": [{"musicDescriptionShelfRenderer": {
            "description": {"runs": [{"text": "line one\n\nline two\r\nline three"}]},
            "footer": {"runs": [{"text": "Source: Test"}]}}}]}}});
        let got = lyrics(&web).unwrap().unwrap();
        assert!(!got.timed);
        assert_eq!(got.source.as_deref(), Some("Source: Test"));
        assert_eq!(got.lines.len(), 3);
        assert!(got.lines.iter().all(|(t, s)| t.is_none() && !s.ends_with('\r')));
    }

    #[test]
    fn no_lyrics_and_changed_shapes() {
        let message = json!({"contents": {"sectionListRenderer": {"contents": [{"messageRenderer": {}}]}}});
        assert!(lyrics(&message).unwrap().is_none(), "YouTube's own 'not available' message");
        assert!(lyrics(&mobile(json!([]))).unwrap().is_none());
        assert!(lyrics(&mobile(json!([{"lyricLine": "  "}]))).unwrap().is_none());
        assert!(lyrics(&json!({"contents": {"somethingNew": {}}})).is_err());
        assert!(lyrics(&json!({})).is_err());
    }

    #[test]
    fn video_ids_come_only_from_youtube_music_watch_urls() {
        let id = |u: &str| video_id_from_url(u);
        assert_eq!(id("https://music.youtube.com/watch?v=abcDEF12_-x").as_deref(), Some("abcDEF12_-x"));
        assert_eq!(id("https://music.youtube.com/watch?list=RDAMVM1&v=abcDEF12_-x&t=3#x").as_deref(), Some("abcDEF12_-x"));
        assert_eq!(id("https://www.youtube.com/watch?v=abcDEF12_-x"), None, "plain YouTube: not a track");
        assert_eq!(id("https://music.youtube.com/browse/MPREb_x"), None);
        assert_eq!(id("https://music.youtube.com/watch?v=short"), None);
        assert_eq!(id("https://music.youtube.com.evil.example/watch?v=abcDEF12_-x"), None);
        assert_eq!(id(""), None);
    }
}
