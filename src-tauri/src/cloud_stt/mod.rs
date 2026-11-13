//! Cloud transcription through OpenRouter. Fork-only and self-contained: the
//! rest of the app touches this module through `is_active`, `transcribe` and
//! `chain` only (see FORK.md).

pub mod client;
mod commands;
mod config;

use tauri::ipc::Invoke;
use tauri::{AppHandle, Wry};

/// Whether dictation should be transcribed in the cloud instead of locally.
pub fn is_active(app: &AppHandle) -> bool {
    config::load(app).is_active()
}

/// Transcribe a finished recording in the cloud, pinning the language when
/// the person's settings name one.
pub async fn transcribe(app: &AppHandle, samples: &[f32]) -> anyhow::Result<String> {
    let config = config::load(app);
    let settings = crate::settings::get_settings(app);
    let language = client::language_hint(&settings.selected_language, &settings.spoken_languages);
    client::transcribe(
        client::DEFAULT_BASE_URL,
        &config.api_key,
        &config.model,
        samples,
        language.as_deref(),
    )
    .await
}

/// Wraps the app's invoke handler so `cloud_stt_*` commands reach this
/// module and everything else reaches `inner`. Keeps the fork's commands out
/// of the generated `bindings.ts`.
pub fn chain(
    inner: impl Fn(Invoke<Wry>) -> bool + Send + Sync + 'static,
) -> impl Fn(Invoke<Wry>) -> bool + Send + Sync + 'static {
    let own: Box<dyn Fn(Invoke<Wry>) -> bool + Send + Sync> = Box::new(tauri::generate_handler![
        commands::cloud_stt_get_status,
        commands::cloud_stt_set_api_key,
        commands::cloud_stt_set_model,
        commands::cloud_stt_set_enabled,
        commands::cloud_stt_list_models,
    ]);
    move |invoke| {
        if invoke.message.command().starts_with("cloud_stt_") {
            own(invoke)
        } else {
            inner(invoke)
        }
    }
}
