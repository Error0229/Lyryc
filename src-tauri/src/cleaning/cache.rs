//! Cache management for track cleaning results

use reqwest::Client;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::RwLock;

// Shared HTTP client instance
static HTTP_CLIENT: OnceLock<Arc<Client>> = OnceLock::new();

// Cache for track cleaning results to avoid duplicate API calls
static TRACK_CLEANING_CACHE: OnceLock<Arc<RwLock<HashMap<String, String>>>> = OnceLock::new();

/// Get shared cache for track cleaning results
pub fn get_track_cleaning_cache() -> &'static Arc<RwLock<HashMap<String, String>>> {
    TRACK_CLEANING_CACHE.get_or_init(|| Arc::new(RwLock::new(HashMap::new())))
}

/// Get shared HTTP client with connection pooling
pub fn get_shared_http_client() -> &'static Arc<Client> {
    HTTP_CLIENT.get_or_init(|| {
        Arc::new(
            Client::builder()
                .timeout(Duration::from_secs(30))
                .pool_max_idle_per_host(10)
                .pool_idle_timeout(Duration::from_secs(90))
                .build()
                .expect("Failed to create HTTP client"),
        )
    })
}
