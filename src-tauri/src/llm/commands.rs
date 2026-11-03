//! Tauri commands implementing `src/services/ipc/llm.ts`.

use serde::Serialize;
use std::time::Instant;
use tauri::{AppHandle, Emitter, State};

use super::presets::{Preset, PRESETS};
use super::store::{lock, LegacyKeys, LlmProvider, LlmSettings, ProviderInput, Route};
use super::{ChatRequest, LlmState, Task};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmTestResult {
    pub latency_ms: u64,
    pub model: String,
    pub sample: String,
}

/// Providers or routes changed, from any window. The overlay retries a
/// translation that failed for want of a provider.
const SETTINGS_CHANGED: &str = "llm-settings-changed";

fn announce<T>(app: &AppHandle, result: Result<T, String>) -> Result<T, String> {
    if result.is_ok() {
        let _ = app.emit(SETTINGS_CHANGED, ());
    }
    result
}

#[tauri::command]
pub fn llm_list_presets() -> &'static [Preset] {
    PRESETS
}

#[tauri::command]
pub fn llm_get_settings(state: State<'_, LlmState>) -> LlmSettings {
    lock(&state).view()
}

#[tauri::command]
pub fn llm_upsert_provider(
    app: AppHandle,
    input: ProviderInput,
    state: State<'_, LlmState>,
) -> Result<LlmProvider, String> {
    announce(&app, lock(&state).upsert(input))
}

#[tauri::command]
pub fn llm_remove_provider(app: AppHandle, id: String, state: State<'_, LlmState>) -> Result<(), String> {
    announce(&app, lock(&state).remove(&id))
}

#[tauri::command]
pub fn llm_set_route(
    app: AppHandle,
    task: Task,
    route: Option<Route>,
    state: State<'_, LlmState>,
) -> Result<(), String> {
    announce(&app, lock(&state).set_route(task, route))
}

#[tauri::command]
pub async fn llm_list_models(
    provider_id: String,
    state: State<'_, LlmState>,
) -> Result<Vec<String>, String> {
    let target = lock(&state).target_for(&provider_id, "")?;
    Ok(super::list_models(&target).await?)
}

#[tauri::command]
pub async fn llm_test(
    provider_id: String,
    model: String,
    state: State<'_, LlmState>,
) -> Result<LlmTestResult, String> {
    let target = lock(&state).target_for(&provider_id, &model)?;
    if target.model.is_empty() {
        return Err("Choose a model to test".to_string());
    }
    let started = Instant::now();
    let reply = super::complete(
        &target,
        &ChatRequest {
            system: None,
            user: "Reply with the single word: pong",
            temperature: None,
            // Generous so reasoning models have room to think before answering.
            max_tokens: 512,
            json: false,
        },
    )
    .await?;
    Ok(LlmTestResult {
        latency_ms: started.elapsed().as_millis() as u64,
        model: target.model,
        sample: reply.trim().chars().take(80).collect(),
    })
}

#[tauri::command]
pub fn llm_import_legacy_keys(
    app: AppHandle,
    keys: LegacyKeys,
    state: State<'_, LlmState>,
) -> Result<LlmSettings, String> {
    announce(&app, lock(&state).import_legacy(&keys))
}
