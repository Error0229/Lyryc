//! NetEase Cloud Music: `/api/search/get` search, `/api/song/lyric` lyric.

use super::{str_of, Api, Candidate, Query};
use anyhow::{anyhow, Result};
use serde_json::Value;

pub const API: Api = Api {
    referer: None,
    search_url,
    candidates,
    lyric_url,
    lyric,
};

fn search_url(q: &Query) -> String {
    format!(
        "https://music.163.com/api/search/get?s={}&type=1&limit=10",
        urlencoding::encode(&format!("{} {}", q.track, q.artist))
    )
}

/// NetEase answers anti-bot refusals with HTTP 200 and its own code.
fn check(v: &Value) -> Result<()> {
    match v["code"].as_i64() {
        Some(200) => Ok(()),
        code => Err(anyhow!("code {:?}: {}", code, str_of(&v["message"]))),
    }
}

fn candidates(v: &Value) -> Result<Vec<Candidate>> {
    check(v)?;
    let songs = v["result"]["songs"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    Ok(songs
        .iter()
        .map(|s| Candidate {
            id: str_of(&s["id"]),
            title: str_of(&s["name"]),
            artist: names(&s["artists"]),
            duration: s["duration"].as_f64().unwrap_or(0.0) / 1000.0,
            key: String::new(),
        })
        .collect())
}

fn names(v: &Value) -> String {
    let all = v.as_array().map(Vec::as_slice).unwrap_or_default();
    all.iter()
        .map(|a| str_of(&a["name"]))
        .collect::<Vec<_>>()
        .join(" ")
}

fn lyric_url(c: &Candidate) -> String {
    format!(
        "https://music.163.com/api/song/lyric?id={}&lv=1&kv=1&tv=-1",
        urlencoding::encode(&c.id)
    )
}

fn lyric(v: &Value) -> Result<Option<String>> {
    check(v)?;
    Ok(v["lrc"]["lyric"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .map(String::from))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::tests::{fixture, koi};
    use serde_json::json;

    #[test]
    fn parses_search_results() {
        let c = candidates(&fixture("netease_search.json")).unwrap();
        assert_eq!(c.len(), 4);
        assert_eq!(
            (c[0].id.as_str(), c[0].title.as_str(), c[0].artist.as_str()),
            ("409430736", "恋", "茅原実里")
        );
        assert_eq!(c[0].duration, 265.773);
        assert_eq!(crate::sources::best(c, &koi()).unwrap().id, "409430736");
        // No results: no `songs` key at all.
        assert!(
            candidates(&json!({"result": {"songCount": 0}, "code": 200}))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn parses_lyric_and_refusals() {
        let lrc = lyric(&fixture("netease_lyric.json")).unwrap().unwrap();
        assert!(lrc.starts_with("[00:00.00] 作词 : 茅原実里"));
        assert_eq!(lyric(&json!({"code": 200, "nolyric": true})).unwrap(), None);
        assert!(candidates(&json!({"code": -460, "message": "Cheating"})).is_err());
    }
}
