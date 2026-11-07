//! Bring-your-own-key LLM layer.
//!
//! Three wire dialects (`dialect.rs`), a data table of presets on top of them
//! (`presets.rs`), and a backend-owned settings store (`store.rs`) that maps
//! each task to a provider + model. Callers build a prompt and call
//! [`complete`]; nothing outside this module knows about providers.
//! [`complete_with`] adds the provider's own web search and a JSON schema.

pub mod commands;
mod dialect;
// Not called yet: wired into the lyrics lookup after its refactor lands
// (docs/research/llm-search/REPORT.md, "Integration").
#[allow(dead_code)]
pub mod identify;
pub mod presets;
#[cfg(test)]
mod search_experiment;
pub mod store;
mod transport;
mod vault;

use serde_json::Value;
use std::fmt;

pub use presets::Preset;
pub use store::{resolve, translation_backup, LlmState, Task};
pub use transport::{complete, complete_with, list_models, Lane};

/// One chat turn: optional system prompt plus a single user message.
pub struct ChatRequest<'a> {
    pub system: Option<&'a str>,
    pub user: &'a str,
    pub temperature: Option<f32>,
    pub max_tokens: u32,
    /// Ask for a JSON object reply where the dialect supports it.
    pub json: bool,
}

/// Extras for [`complete_with`]. `Tools::default()` is a plain chat turn.
#[derive(Clone, Copy, Default)]
pub struct Tools<'a> {
    /// Let the model run the provider's built-in web search. Providers
    /// without one fail the call instead of answering unsearched.
    pub web_search: bool,
    /// Constrain the reply to this schema. Write it strict-mode compatible
    /// (every property required, `additionalProperties: false`): OpenAI-style
    /// APIs reject anything else. Where a provider can't combine a schema
    /// with search, the schema goes into the system prompt instead.
    pub schema: Option<JsonSchema<'a>>,
}

#[derive(Clone, Copy)]
pub struct JsonSchema<'a> {
    /// `[a-zA-Z0-9_-]{1,64}`, required by OpenAI-style APIs.
    pub name: &'a str,
    pub schema: &'a Value,
}

/// A reply plus what the provider's search tool reported.
#[derive(Debug, Default, PartialEq)]
pub struct Completion {
    pub text: String,
    /// Pages the provider grounded the reply on, deduplicated, in order.
    pub sources: Vec<String>,
    /// Searches the provider ran: the unit most of them bill per.
    pub searches: u32,
}

/// A resolved route: everything needed to talk to one model.
#[derive(Clone)]
pub struct Target {
    pub label: String,
    pub preset: &'static Preset,
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
}

// Hand-written so the key can never end up in a log line.
impl fmt::Debug for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Target")
            .field("label", &self.label)
            .field("preset", &self.preset.id)
            .field("base_url", &self.base_url)
            .field("has_key", &self.api_key.is_some())
            .field("model", &self.model)
            .finish()
    }
}

/// Prefix for errors returned when no provider is routed for a task.
pub const NOT_CONFIGURED: &str = "LLM_NOT_CONFIGURED:";

#[derive(Debug)]
pub struct LlmError {
    pub provider: String,
    pub model: String,
    pub status: Option<u16>,
    pub message: String,
}

impl LlmError {
    pub fn is_auth(&self) -> bool {
        matches!(self.status, Some(401 | 403))
    }
}

impl fmt::Display for LlmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.model.is_empty() {
            write!(f, "{}: {}", self.provider, self.message)
        } else {
            write!(f, "{} ({}): {}", self.provider, self.model, self.message)
        }
    }
}

impl From<LlmError> for String {
    fn from(e: LlmError) -> Self {
        e.to_string()
    }
}
