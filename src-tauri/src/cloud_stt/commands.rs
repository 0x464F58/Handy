//! Tauri commands for the cloud transcription settings panel. Named
//! `cloud_stt_*` so `chain` can route them without the specta bindings.

use super::client::{self, CloudModel};
use super::config;
use serde::Serialize;
use tauri::AppHandle;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudStatus {
    pub enabled: bool,
    pub model: String,
    pub has_key: bool,
}

#[tauri::command]
pub fn cloud_stt_get_status(app: AppHandle) -> CloudStatus {
    let config = config::load(&app);
    CloudStatus {
        enabled: config.enabled,
        model: config.model,
        has_key: !config.api_key.is_empty(),
    }
}

#[tauri::command]
pub fn cloud_stt_set_api_key(app: AppHandle, api_key: String) -> Result<(), String> {
    config::update(&app, |c| c.api_key = api_key.trim().to_string())
}

#[tauri::command]
pub fn cloud_stt_set_model(app: AppHandle, model: String) -> Result<(), String> {
    config::update(&app, |c| c.model = model)
}

#[tauri::command]
pub fn cloud_stt_set_enabled(app: AppHandle, enabled: bool) -> Result<(), String> {
    config::update(&app, |c| c.enabled = enabled)
}

#[tauri::command]
pub async fn cloud_stt_list_models(app: AppHandle) -> Result<Vec<CloudModel>, String> {
    let api_key = config::load(&app).api_key;
    if api_key.is_empty() {
        return Err("Paste an OpenRouter API key first".to_string());
    }
    client::list_models(client::DEFAULT_BASE_URL, &api_key)
        .await
        .map_err(|e| format!("{e:#}"))
}
