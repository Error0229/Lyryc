//! Cleaning types and data structures

use serde::{Deserialize, Serialize};

/// Result of a cleaning operation
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CleaningResult {
    pub cleaned_title: String,
    pub cleaned_artist: Option<String>,
    pub cleaned_artists: Vec<String>,
    pub kept_markers: Vec<String>,
    pub removed_context: Vec<String>,
    pub confidence: f64,
    pub notes: Option<String>,
}

/// Result of combined artist and track cleaning
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ArtistTrackCleaningResult {
    pub artist: CleaningResult,
    pub track: CleaningResult,
}
