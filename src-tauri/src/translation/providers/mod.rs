//! LLM Provider implementations for translation

mod gemini;
mod github;

pub use gemini::{translate_lyrics_gemini, try_gemini_alignment};
pub use github::{translate_lyrics_github, try_github_alignment};

use super::types::{LyricLine, TranslationConfig, TranslationResult};

/// Translate lyrics using the configured provider
pub async fn translate_with_provider(
    lyrics: &[LyricLine],
    config: &TranslationConfig,
) -> Result<TranslationResult, String> {
    match config.provider.as_str() {
        "gemini" => translate_lyrics_gemini(lyrics, config).await,
        "github" | _ => translate_lyrics_github(lyrics, config).await,
    }
}
