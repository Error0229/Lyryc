//! Bring-your-own-key LLM layer.
//!
//! Three wire dialects (`dialect.rs`), a data table of presets on top of them
//! (`presets.rs`), and a backend-owned settings store (`store.rs`) that maps
//! each task to a provider + model. Callers build a prompt and call
//! [`complete`]; nothing outside this module knows about providers.

pub mod commands;
mod dialect;
pub mod presets;
pub mod store;
mod transport;

use std::fmt;

pub use presets::Preset;
pub use store::{resolve, LlmState, Task};
pub use transport::{complete, list_models};

/// One chat turn: optional system prompt plus a single user message.
pub struct ChatRequest<'a> {
    pub system: Option<&'a str>,
    pub user: &'a str,
    pub temperature: Option<f32>,
    pub max_tokens: u32,
    /// Ask for a JSON object reply where the dialect supports it.
    pub json: bool,
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
