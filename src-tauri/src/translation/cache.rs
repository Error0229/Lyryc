//! Translations kept across runs, so a song is translated once per wording,
//! prefs and model: when it plays again, and when the lookahead translated it
//! before it started. Keyed by the lines' text, not their times: retiming
//! (word alignment, a better source) doesn't cost a new translation.

use sha2::{Digest, Sha256};

use super::types::{LyricLine, TranslationConfig, TranslationResult};
use crate::llm::Target;
use crate::lyrics_cache::{cache_translation, get_translation};

/// Everything that changes the answer: the text, the prefs, the model.
pub fn key(lyrics: &[LyricLine], config: &TranslationConfig, target: &Target) -> String {
    let mut h = Sha256::new();
    for part in [
        config.source_language.as_str(),
        config.target_language.as_str(),
        config.style.as_str(),
        config.custom_prompt.as_deref().unwrap_or(""),
        if config.preserve_rhyme { "r" } else { "-" },
        if config.preserve_meaning { "m" } else { "-" },
        if config.maintain_structure { "s" } else { "-" },
        target.base_url.as_str(),
        target.model.as_str(),
    ] {
        h.update(part.as_bytes());
        h.update([0]);
    }
    for line in lyrics {
        h.update(line.text.as_bytes());
        h.update([b'\n']);
    }
    format!("{:x}", h.finalize())
}

/// A stored translation for these lines, with the lines' current times.
pub fn get(key: &str, lyrics: &[LyricLine]) -> Option<TranslationResult> {
    let mut hit: TranslationResult = serde_json::from_str(&get_translation(key)?).ok()?;
    if hit.translated_lines.len() != lyrics.len() {
        return None;
    }
    for (t, l) in hit.translated_lines.iter_mut().zip(lyrics) {
        t.time = l.time;
        t.duration = l.duration;
    }
    hit.processing_time_ms = 0;
    Some(hit)
}

/// Keep a translation only when every line got one: a partial answer is
/// shown this time but asked again next time, never stuck.
pub fn put(key: &str, result: &TranslationResult) {
    let complete = result.success && result.translated_lines.iter().all(|l| l.translation_confidence != Some(0.0));
    if let (true, Ok(json)) = (complete, serde_json::to_string(result)) {
        cache_translation(key, &json);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(model: &str) -> Target {
        Target {
            label: "test".into(),
            preset: &crate::llm::presets::PRESETS[0],
            base_url: "https://example.invalid".into(),
            api_key: None,
            model: model.into(),
        }
    }

    fn lines(times: &[f64]) -> Vec<LyricLine> {
        ["夜明け前の街に", "光が降りてくる"]
            .iter()
            .zip(times)
            .map(|(t, &time)| LyricLine { time, text: t.to_string(), duration: None })
            .collect()
    }

    #[test]
    fn retiming_keeps_the_key_and_prefs_or_model_change_it() {
        let config = TranslationConfig { target_language: "zh-TW".into(), ..TranslationConfig::default() };
        let k = key(&lines(&[1.0, 5.0]), &config, &target("m1"));
        assert_eq!(k, key(&lines(&[1.4, 5.9]), &config, &target("m1")), "word alignment moved the times");
        assert_ne!(k, key(&lines(&[1.0, 5.0]), &config, &target("m2")), "another model");
        let en = TranslationConfig { target_language: "en".into(), ..config.clone() };
        assert_ne!(k, key(&lines(&[1.0, 5.0]), &en, &target("m1")), "another language");
    }
}
