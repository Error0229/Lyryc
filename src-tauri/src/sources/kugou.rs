//! Kugou: `lyrics.kugou.com/search` (it filters by duration itself), then
//! `/download` of a KRC file: base64 of "krc1" + XOR-scrambled zlib. KRC has
//! word timing; LyricLine has no room for it yet, so it is dropped.

use super::{str_of, Api, Candidate, Name};
use anyhow::{anyhow, Result};
use base64::Engine;
use regex::Regex;
use serde_json::Value;
use std::io::Read;
use std::sync::OnceLock;

pub const API: Api = Api {
    referer: None,
    search_url,
    candidates,
    lyric_url,
    lyric,
};

const KRC_KEY: [u8; 16] = [
    64, 71, 97, 119, 94, 50, 116, 71, 81, 54, 49, 45, 206, 210, 110, 105,
];

fn search_url(name: &Name, duration: f64) -> String {
    format!(
        "https://lyrics.kugou.com/search?ver=1&man=yes&client=pc&keyword={}&duration={}",
        urlencoding::encode(&format!("{} - {}", name.artist, name.title)),
        (duration * 1000.0).round() as u64
    )
}

fn candidates(v: &Value) -> Result<Vec<Candidate>> {
    let list = v["candidates"]
        .as_array()
        .ok_or_else(|| anyhow!("unexpected search response"))?;
    Ok(list
        .iter()
        .map(|c| Candidate {
            id: str_of(&c["id"]),
            title: str_of(&c["song"]),
            artist: str_of(&c["singer"]),
            duration: c["duration"].as_f64().unwrap_or(0.0) / 1000.0,
            key: str_of(&c["accesskey"]),
        })
        .collect())
}

fn lyric_url(c: &Candidate) -> String {
    format!(
        "https://lyrics.kugou.com/download?ver=1&client=pc&id={}&accesskey={}&fmt=krc&charset=utf8",
        urlencoding::encode(&c.id),
        urlencoding::encode(&c.key)
    )
}

fn lyric(v: &Value) -> Result<Option<String>> {
    match v["content"].as_str().filter(|s| !s.is_empty()) {
        Some(b64) => Ok(Some(krc_to_lrc(&decode_krc(b64)?))),
        None => Ok(None),
    }
}

fn decode_krc(b64: &str) -> Result<String> {
    let raw = base64::engine::general_purpose::STANDARD.decode(b64)?;
    let body = raw
        .strip_prefix(b"krc1")
        .ok_or_else(|| anyhow!("not a KRC file"))?;
    let plain: Vec<u8> = body
        .iter()
        .enumerate()
        .map(|(i, b)| b ^ KRC_KEY[i % KRC_KEY.len()])
        .collect();
    let mut out = Vec::new();
    flate2::read::ZlibDecoder::new(plain.as_slice()).read_to_end(&mut out)?;
    Ok(String::from_utf8_lossy(&out).into_owned())
}

/// `[start_ms,dur_ms]<off,dur,0>word...` lines to `[mm:ss.xx]text`.
fn krc_to_lrc(krc: &str) -> String {
    static LINE: OnceLock<Regex> = OnceLock::new();
    static WORD: OnceLock<Regex> = OnceLock::new();
    let line = LINE.get_or_init(|| Regex::new(r"^\[(\d+),(\d+)\](.*)$").unwrap());
    let word = WORD.get_or_init(|| Regex::new(r"<\d+,\d+,\d+>").unwrap());
    krc.lines()
        .filter_map(|l| line.captures(l.trim_end_matches('\r')))
        .map(|c| {
            let ms: u64 = c[1].parse().unwrap_or(0);
            let text = word.replace_all(&c[3], "");
            format!(
                "[{:02}:{:02}.{:02}]{}",
                ms / 60_000,
                ms / 1000 % 60,
                ms % 1000 / 10,
                text
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::tests::{fixture, koi};
    use serde_json::json;

    #[test]
    fn parses_search_results() {
        let c = candidates(&fixture("kugou_search.json")).unwrap();
        assert_eq!(c.len(), 4);
        assert_eq!(c[0].id, "49985390");
        assert_eq!(c[0].key, "86167D5677C979CDB68A3417271047CC");
        assert_eq!(
            (c[0].title.as_str(), c[0].artist.as_str(), c[0].duration),
            ("恋", "茅原実里", 265.0)
        );
        assert_eq!(
            crate::sources::best(c, &koi(), true).unwrap().id,
            "49985390"
        );
    }

    #[test]
    fn decodes_krc_download() {
        let lrc = lyric(&fixture("kugou_download.json")).unwrap().unwrap();
        assert!(lrc.lines().all(|l| l.starts_with('[')));
        assert!(!lrc.contains('<'), "word timing is stripped");
        assert!(lrc.contains("眠れない夜"), "{}", lrc);
        assert_eq!(lyric(&json!({"status": 200, "content": ""})).unwrap(), None);
        assert!(lyric(&json!({"content": "bm90IGtyYw=="})).is_err());
    }

    #[test]
    fn krc_lines_become_lrc() {
        let krc = "[ti:x]\n[24450,5000]<0,300,0>眠れ<300,400,0>ない夜\r\n[61234,10]<0,10,0>a";
        assert_eq!(krc_to_lrc(krc), "[00:24.45]眠れない夜\n[01:01.23]a");
    }
}
