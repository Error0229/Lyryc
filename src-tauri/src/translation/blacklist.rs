//! Model blacklist management for rate-limited LLM models

use log::{debug, warn};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

lazy_static::lazy_static! {
    static ref MODEL_BLACKLIST: Arc<Mutex<HashMap<String, u64>>> = Arc::new(Mutex::new(HashMap::new()));
}

/// Check if a model is currently blacklisted (rate-limited)
pub fn is_model_blacklisted(model_id: &str) -> bool {
    if let Ok(blacklist) = MODEL_BLACKLIST.lock() {
        if let Some(&blacklist_until) = blacklist.get(model_id) {
            let current_time = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();

            if current_time < blacklist_until {
                debug!(
                    "Model {} is blacklisted until {}",
                    model_id, blacklist_until
                );
                return true;
            } else {
                debug!("Model {} blacklist expired, removing", model_id);
                drop(blacklist);
                if let Ok(mut blacklist) = MODEL_BLACKLIST.lock() {
                    blacklist.remove(model_id);
                }
            }
        }
    }
    false
}

/// Blacklist a model for a specified duration (in seconds)
pub fn blacklist_model(model_id: &str, wait_seconds: u64) {
    let blacklist_until = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + wait_seconds;

    if let Ok(mut blacklist) = MODEL_BLACKLIST.lock() {
        blacklist.insert(model_id.to_string(), blacklist_until);
        warn!(
            "Blacklisted model {} for {} seconds (until {})",
            model_id, wait_seconds, blacklist_until
        );
    }
}

/// Duration for rate limit blacklist (24 hours)
pub const RATE_LIMIT_BLACKLIST_DURATION: u64 = 86400;
