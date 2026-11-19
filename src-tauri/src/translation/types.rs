//! Translation types and data structures

use serde::{Deserialize, Serialize};

use crate::llm::store::{AdhocKey, LegacyKeys};

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
    /// What made it, kept with it in the cache: a song has one translation
    /// per setting and model (`cache::key`), and Saved lyrics names each by
    /// this. None when nothing was translated.
    #[serde(default)]
    pub made_with: Option<MadeWith>,
}

/// The settings a translation was made with, and the model that answered.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct MadeWith {
    pub target_language: String,
    pub style: String,
    pub custom_prompt: Option<String>,
    pub preserve_rhyme: bool,
    pub preserve_meaning: bool,
    pub maintain_structure: bool,
    /// The provider's name in Settings, and the model asked.
    pub provider: String,
    pub model: String,
}

impl MadeWith {
    pub fn new(config: &TranslationConfig, target: &crate::llm::Target) -> Self {
        Self {
            target_language: config.target_language.clone(),
            style: config.style.clone(),
            custom_prompt: config.custom_prompt.clone(),
            preserve_rhyme: config.preserve_rhyme,
            preserve_meaning: config.preserve_meaning,
            maintain_structure: config.maintain_structure,
            provider: target.label.clone(),
            model: target.model.clone(),
        }
    }
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
            made_with: None,
        }
    }

    /// Create an empty result (no lyrics provided)
    pub fn empty() -> Self {
        Self::error("No lyrics provided")
    }
}
