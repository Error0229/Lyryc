//! Cache for track cleaning results

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use tokio::sync::RwLock;

// Cache for track cleaning results to avoid duplicate API calls
static TRACK_CLEANING_CACHE: OnceLock<Arc<RwLock<HashMap<String, String>>>> = OnceLock::new();

/// Get shared cache for track cleaning results
pub fn get_track_cleaning_cache() -> &'static Arc<RwLock<HashMap<String, String>>> {
    TRACK_CLEANING_CACHE.get_or_init(|| Arc::new(RwLock::new(HashMap::new())))
}
