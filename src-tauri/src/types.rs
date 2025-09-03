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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LyricLine {
    pub time: f64, // time in seconds
    pub text: String,
    pub duration: Option<f64>,
}

// Global state types
pub type TrackState = Arc<Mutex<Option<TrackInfo>>>;
pub type WebSocketState = Arc<Mutex<Option<Arc<WebSocketServer>>>>;
pub type ClickThroughState = Arc<Mutex<bool>>; // true = click-through enabled, false = disabled
