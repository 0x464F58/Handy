//! Cloud transcription settings, kept in their own store file so they never
//! touch the app's `AppSettings`.

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

const STORE_PATH: &str = "cloud_stt.json";
const KEY: &str = "config";

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CloudConfig {
    pub enabled: bool,
    /// Provider model slug, e.g. "openai/whisper-1". Empty until picked.
    pub model: String,
    /// OpenRouter API key. Never sent to the frontend.
    pub api_key: String,
}

impl CloudConfig {
    /// Cloud mode applies only when switched on and fully set up, so a
    /// half-configured toggle can't leave dictation with no transcriber.
    pub fn is_active(&self) -> bool {
        self.enabled && !self.model.is_empty() && !self.api_key.is_empty()
    }
}

pub fn load(app: &AppHandle) -> CloudConfig {
    app.store(crate::portable::store_path(STORE_PATH))
        .ok()
        .and_then(|store| store.get(KEY))
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

pub fn update(app: &AppHandle, change: impl FnOnce(&mut CloudConfig)) -> Result<(), String> {
    let store = app
        .store(crate::portable::store_path(STORE_PATH))
        .map_err(|e| e.to_string())?;
    let mut config = store
        .get(KEY)
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default();
    change(&mut config);
    store.set(
        KEY,
        serde_json::to_value(&config).map_err(|e| e.to_string())?,
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(enabled: bool, model: &str, key: &str) -> CloudConfig {
        CloudConfig {
            enabled,
            model: model.into(),
            api_key: key.into(),
        }
    }

    #[test]
    fn active_only_when_enabled_with_model_and_key() {
        assert!(config(true, "openai/whisper-1", "sk").is_active());
        assert!(!config(false, "openai/whisper-1", "sk").is_active());
        assert!(!config(true, "", "sk").is_active());
        assert!(!config(true, "openai/whisper-1", "").is_active());
    }

    #[test]
    fn stored_config_missing_fields_defaults_to_inactive() {
        let config: CloudConfig = serde_json::from_str("{}").unwrap();
        assert!(!config.is_active());
    }
}
