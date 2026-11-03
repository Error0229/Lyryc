use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::websocket::WebSocketServer;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TrackInfo {
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub duration: Option<f64>,
    pub thumbnail: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct LyricLine {
    pub time: f64, // time in seconds
    pub text: String,
    pub duration: Option<f64>,
    /// Word onsets inside the line (pre-aligned timing only). Absent from the
    /// wire when None, so `fetch_lyrics` payloads and cached rows are unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub words: Option<Vec<LyricWord>>,
}

/// A word (Latin word, or one CJK character / hangul syllable) inside a line.
/// `time` is its onset in seconds; without `duration` it runs until the next word.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct LyricWord {
    pub time: f64,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<f64>,
}

// Global state types
pub type TrackState = Arc<Mutex<Option<TrackInfo>>>;
pub type WebSocketState = Arc<Mutex<Option<Arc<WebSocketServer>>>>;
pub type ClickThroughState = Arc<Mutex<bool>>; // true = click-through enabled, false = disabled
