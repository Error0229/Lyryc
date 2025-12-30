//! LLM Provider implementations for translation
//!
//! GitHub models are the primary provider (higher free-tier rate limits).
//! Gemini is deprecated but kept for backward compatibility.

mod gemini;
mod github;

pub use gemini::{translate_lyrics_gemini, try_gemini_alignment};
pub use github::{translate_lyrics_github, try_github_alignment};

use super::types::{LyricLine, TranslationConfig, TranslationResult};

/// Translate lyrics using the configured provider
///
/// Default: GitHub models (150 req/day for low-tier, 50 req/day for high-tier)
/// Deprecated: Gemini (~20 req/day free tier limit)
pub async fn translate_with_provider(
    lyrics: &[LyricLine],
    config: &TranslationConfig,
) -> Result<TranslationResult, String> {
    match config.provider.as_str() {
        "gemini" => translate_lyrics_gemini(lyrics, config).await,
        _ => translate_lyrics_github(lyrics, config).await, // Default to GitHub
    }
}
