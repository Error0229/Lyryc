//! Provider presets: pure data on top of the three wire dialects.
//!
//! Base URLs and model IDs were checked against each provider's official docs
//! (2026-11). `suggested_models` is only a fallback for when the live models
//! endpoint is unavailable; the first entry is the default for legacy import.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Dialect {
    /// OpenAI-compatible Chat Completions (`POST {base}/chat/completions`)
    Openai,
    /// Anthropic Messages (`POST {base}/messages`)
    Anthropic,
    /// Gemini `POST {base}/models/{model}:generateContent`
    Gemini,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub id: &'static str,
    pub name: &'static str,
    pub dialect: Dialect,
    pub base_url: &'static str,
    pub key_required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_url: Option<&'static str>,
    pub suggested_models: &'static [&'static str],
    /// Models endpoint when it does not live at `{base}/models`.
    #[serde(skip)]
    pub models_url: Option<&'static str>,
}

impl Preset {
    pub fn default_model(&self) -> Option<&'static str> {
        self.suggested_models.first().copied()
    }
}

pub const CUSTOM: &str = "custom";
pub const GITHUB: &str = "github";
pub const GEMINI: &str = "gemini";

pub static PRESETS: &[Preset] = &[
    Preset {
        id: "openai",
        name: "OpenAI",
        dialect: Dialect::Openai,
        base_url: "https://api.openai.com/v1",
        key_required: true,
        key_url: Some("https://platform.openai.com/api-keys"),
        suggested_models: &["gpt-6.1-terra", "gpt-6-luna", "gpt-6.1-sol"],
        models_url: None,
    },
    Preset {
        id: "anthropic",
        name: "Anthropic",
        dialect: Dialect::Anthropic,
        base_url: "https://api.anthropic.com/v1",
        key_required: true,
        key_url: Some("https://console.anthropic.com/settings/keys"),
        suggested_models: &["claude-haiku-5-5", "claude-sonnet-5-5", "claude-opus-5-5"],
        models_url: None,
    },
    Preset {
        id: GEMINI,
        name: "Google Gemini",
        dialect: Dialect::Gemini,
        base_url: "https://generativelanguage.googleapis.com/v1beta",
        key_required: true,
        key_url: Some("https://aistudio.google.com/apikey"),
        suggested_models: &[
            "gemini-3.5-flash-lite",
            "gemini-3.6-flash",
            "gemini-2.5-flash-lite",
        ],
        models_url: None,
    },
    Preset {
        id: "openrouter",
        name: "OpenRouter",
        dialect: Dialect::Openai,
        base_url: "https://openrouter.ai/api/v1",
        key_required: true,
        key_url: Some("https://openrouter.ai/keys"),
        suggested_models: &[],
        models_url: None,
    },
    Preset {
        id: "groq",
        name: "Groq",
        dialect: Dialect::Openai,
        base_url: "https://api.groq.com/openai/v1",
        key_required: true,
        key_url: Some("https://console.groq.com/keys"),
        suggested_models: &[
            "llama-3.3-70b-versatile",
            "openai/gpt-oss-120b",
            "openai/gpt-oss-20b",
            "llama-3.1-8b-instant",
        ],
        models_url: None,
    },
    Preset {
        id: "deepseek",
        name: "DeepSeek",
        dialect: Dialect::Openai,
        base_url: "https://api.deepseek.com",
        key_required: true,
        key_url: Some("https://platform.deepseek.com/api_keys"),
        suggested_models: &["deepseek-flash", "deepseek-v4-pro"],
        models_url: None,
    },
    Preset {
        id: "mistral",
        name: "Mistral",
        dialect: Dialect::Openai,
        base_url: "https://api.mistral.ai/v1",
        key_required: true,
        key_url: Some("https://console.mistral.ai/api-keys"),
        suggested_models: &["mistral-small-2603", "mistral-medium-latest"],
        models_url: None,
    },
    Preset {
        id: "xai",
        name: "xAI",
        dialect: Dialect::Openai,
        base_url: "https://api.x.ai/v1",
        key_required: true,
        key_url: Some("https://console.x.ai"),
        suggested_models: &["grok-4.3", "grok-4.7"],
        models_url: None,
    },
    // GitHub retired GitHub Models on 2026-07-30. Kept so legacy keys still
    // import and existing routes fail with a readable error instead of vanishing.
    Preset {
        id: GITHUB,
        name: "GitHub Models (retired)",
        dialect: Dialect::Openai,
        base_url: "https://models.github.ai/inference",
        key_required: true,
        key_url: Some("https://github.com/settings/tokens"),
        suggested_models: &[
            "openai/gpt-4.1-mini",
            "openai/gpt-4.1-nano",
            "openai/gpt-4o-mini",
        ],
        models_url: Some("https://models.github.ai/catalog/models"),
    },
    Preset {
        id: "ollama",
        name: "Ollama (local)",
        dialect: Dialect::Openai,
        base_url: "http://localhost:11434/v1",
        key_required: false,
        key_url: None,
        suggested_models: &["gpt-oss:20b", "qwen3:8b"],
        models_url: None,
    },
    Preset {
        id: "lmstudio",
        name: "LM Studio (local)",
        dialect: Dialect::Openai,
        base_url: "http://localhost:1234/v1",
        key_required: false,
        key_url: None,
        suggested_models: &[],
        models_url: None,
    },
    Preset {
        id: CUSTOM,
        name: "Custom (OpenAI-compatible)",
        dialect: Dialect::Openai,
        base_url: "",
        key_required: false,
        key_url: None,
        suggested_models: &[],
        models_url: None,
    },
];

pub fn find(id: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ids_are_unique_and_lookup_works() {
        let ids: HashSet<_> = PRESETS.iter().map(|p| p.id).collect();
        assert_eq!(ids.len(), PRESETS.len());
        for p in PRESETS {
            assert_eq!(find(p.id).unwrap().id, p.id);
        }
        assert!(find("nope").is_none());
    }

    #[test]
    fn base_urls_are_well_formed() {
        for p in PRESETS {
            if p.id == CUSTOM {
                assert!(p.base_url.is_empty());
                continue;
            }
            assert!(p.base_url.starts_with("http"), "{}", p.id);
            assert!(!p.base_url.ends_with('/'), "{}", p.id);
            if p.key_required {
                assert!(p.base_url.starts_with("https://"), "{}", p.id);
            }
        }
    }

    #[test]
    fn legacy_presets_have_default_models() {
        assert_eq!(find(GITHUB).unwrap().dialect, Dialect::Openai);
        assert_eq!(find(GEMINI).unwrap().dialect, Dialect::Gemini);
        assert!(find(GITHUB).unwrap().default_model().is_some());
        assert!(find(GEMINI).unwrap().default_model().is_some());
    }

    #[test]
    fn local_presets_need_no_key() {
        assert!(!find("ollama").unwrap().key_required);
        assert!(!find("lmstudio").unwrap().key_required);
    }

    #[test]
    fn serializes_camel_case() {
        let v = serde_json::to_value(find("openai").unwrap()).unwrap();
        assert_eq!(v["dialect"], "openai");
        assert_eq!(v["baseUrl"], "https://api.openai.com/v1");
        assert_eq!(v["keyRequired"], true);
        assert!(v.get("modelsUrl").is_none());
        assert!(v["suggestedModels"].is_array());
    }
}
