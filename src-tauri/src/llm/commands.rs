//! Tauri commands implementing `src/services/ipc/llm.ts`.

use serde::Serialize;
use std::time::Instant;
use tauri::State;

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
    input: ProviderInput,
    state: State<'_, LlmState>,
) -> Result<LlmProvider, String> {
    lock(&state).upsert(input)
}

#[tauri::command]
pub fn llm_remove_provider(id: String, state: State<'_, LlmState>) -> Result<(), String> {
    lock(&state).remove(&id)
}

#[tauri::command]
pub fn llm_set_route(
    task: Task,
    route: Option<Route>,
    state: State<'_, LlmState>,
) -> Result<(), String> {
    lock(&state).set_route(task, route)
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
    keys: LegacyKeys,
    state: State<'_, LlmState>,
) -> Result<LlmSettings, String> {
    lock(&state).import_legacy(&keys)
}
