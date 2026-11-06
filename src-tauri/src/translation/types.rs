//! Translation types and data structures

use serde::{Deserialize, Serialize};

use crate::llm::store::{AdhocKey, LegacyKeys};

/// How much of the song one request carries: all of it, then smaller pieces
/// when a reply doesn't fit.
#[derive(Debug, Clone)]
pub enum TranslationStrategy {
    Combined,
    Chunked(usize),
    LineByLine,
}

/// Configuration for translation request.
///
/// `provider` and the two key fields are legacy: the backend settings store
/// picks the provider now. They are still accepted, and a key that arrives here
/// is used for a task that has no route yet, so an un-migrated frontend works.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TranslationConfig {
    pub source_language: String,
    pub target_language: String,
    pub style: String, // "natural", "literal", "poetic", "rhyming", "custom"
    pub custom_prompt: Option<String>,
    pub preserve_rhyme: bool,
    pub preserve_meaning: bool,
    pub maintain_structure: bool,
    #[serde(default)]
    pub provider: String, // legacy: "github" or "gemini"
    pub github_api_key: Option<String>,
    pub gemini_api_key: Option<String>,
}

impl Default for TranslationConfig {
    fn default() -> Self {
        Self {
            source_language: "auto".to_string(),
            target_language: "en".to_string(),
            style: "natural".to_string(),
            custom_prompt: None,
            preserve_rhyme: true,
            preserve_meaning: true,
            maintain_structure: true,
            provider: "github".to_string(), // GitHub models preferred (higher rate limits)
            github_api_key: None,
            gemini_api_key: None,
        }
    }
}

impl TranslationConfig {
    /// Legacy keys from this call, the selected legacy provider first.
    pub fn legacy_keys(&self) -> Vec<AdhocKey> {
        LegacyKeys {
            github_api_key: self.github_api_key.clone(),
            gemini_api_key: self.gemini_api_key.clone(),
        }
        .candidates(&self.provider)
    }
}

/// Input lyric line
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LyricLine {
    pub time: f64,
    pub text: String,
    pub duration: Option<f64>,
}

/// Output translated lyric line
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TranslatedLyricLine {
    pub time: f64,
    pub text: String,
    pub duration: Option<f64>,
    pub original_text: String,
    pub translated_text: String,
    pub translation_confidence: Option<f64>,
    pub detected_language: Option<String>,
}

/// Translation operation result
#[derive(Debug, Serialize, Deserialize)]
pub struct TranslationResult {
    pub success: bool,
    pub translated_lines: Vec<TranslatedLyricLine>,
    pub detected_language: Option<String>,
    pub processing_time_ms: u64,
    pub error: Option<String>,
    pub is_same_language: bool,
}

#[allow(dead_code)]
impl TranslationResult {
    /// Create an error result
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            translated_lines: vec![],
            detected_language: None,
            processing_time_ms: 0,
            error: Some(message.into()),
            is_same_language: false,
        }
    }

    /// Create an empty result (no lyrics provided)
    pub fn empty() -> Self {
        Self::error("No lyrics provided")
    }
}
