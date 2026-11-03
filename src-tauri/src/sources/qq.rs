//! QQ Music: `client_search_cp` search, `fcg_query_lyric_new` lyric (plain LRC
//! with `nobase64=1`). Both want a y.qq.com Referer.

use super::{str_of, Api, Candidate, Query};
use anyhow::{anyhow, Result};
use serde_json::Value;

pub const API: Api = Api {
    referer: Some("https://y.qq.com/"),
    search_url,
    candidates,
    lyric_url,
    lyric,
};

fn search_url(q: &Query) -> String {
    format!(
        "https://c.y.qq.com/soso/fcgi-bin/client_search_cp?w={}&format=json&n=10",
        urlencoding::encode(&format!("{} {}", q.track, q.artist))
    )
}

fn candidates(v: &Value) -> Result<Vec<Candidate>> {
    let list = v["data"]["song"]["list"]
        .as_array()
        .ok_or_else(|| anyhow!("unexpected search response"))?;
    Ok(list
        .iter()
        .map(|s| Candidate {
            id: str_of(&s["songmid"]),
            title: str_of(&s["songname"]),
            artist: names(&s["singer"]),
            duration: s["interval"].as_f64().unwrap_or(0.0),
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
        "https://c.y.qq.com/lyric/fcgi-bin/fcg_query_lyric_new.fcg?songmid={}&format=json&nobase64=1",
        urlencoding::encode(&c.id)
    )
}

fn lyric(v: &Value) -> Result<Option<String>> {
    if !v.is_object() {
        return Err(anyhow!("unexpected lyric response"));
    }
    Ok(v["lyric"]
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
        let c = candidates(&fixture("qq_search.json")).unwrap();
        assert_eq!(c.len(), 5);
        assert_eq!(c[0].id, "004b0Z2O3rHFH1");
        assert_eq!(c[0].title, "恋 (Love)");
        assert_eq!(c[0].artist, "茅原实里 (ちはらみのり)");
        assert_eq!(c[0].duration, 265.0);
        assert_eq!(
            crate::sources::best(c, &koi()).unwrap().id,
            "004b0Z2O3rHFH1"
        );
        assert!(candidates(&json!({"code": 0, "data": {}})).is_err());
    }

    #[test]
    fn parses_lyric() {
        let lrc = lyric(&fixture("qq_lyric.json")).unwrap().unwrap();
        assert!(lrc.contains("[00:24.45]眠れない夜 脳裏をよぎる"));
        assert_eq!(
            lyric(&json!({"retcode": -1901, "code": -1901})).unwrap(),
            None
        );
        assert!(lyric(&json!("<html>")).is_err());
    }

    #[test]
    fn search_url_encodes_query() {
        assert_eq!(
            search_url(&koi()),
            "https://c.y.qq.com/soso/fcgi-bin/client_search_cp?w=%E6%81%8B%20%E8%8C%85%E5%8E%9F%E5%AE%9F%E9%87%8C&format=json&n=10"
        );
    }
}
