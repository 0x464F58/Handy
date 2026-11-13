//! OpenRouter speech-to-text client: request building, response parsing and
//! the two HTTP calls. No Tauri types here, so it is testable on its own.

use anyhow::{anyhow, Context, Result};
use base64::Engine;
use serde::Serialize;
use serde_json::{json, Value};
use std::io::Cursor;
use std::time::Duration;

pub const DEFAULT_BASE_URL: &str = "https://openrouter.ai/api/v1";

const MODELS_TIMEOUT: Duration = Duration::from_secs(15);
/// Covers upload plus provider processing of a long dictation.
const TRANSCRIBE_TIMEOUT: Duration = Duration::from_secs(120);

/// A transcription model offered by the provider.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudModel {
    pub id: String,
    pub name: String,
    /// Short price tag for the picker, e.g. "$0.36/h". `None` if unpriced.
    pub price: Option<String>,
}

/// The tag for a model's `prompt`/`completion` prices as the provider lists
/// them (strings, USD). The list has no unit field: duration-priced models
/// give USD per second, token-priced ones USD per token, and a few give USD
/// per hour directly.
pub fn price_label(prompt: &str, completion: &str) -> Option<String> {
    let prompt: f64 = prompt.parse().ok()?;
    let completion: f64 = completion.parse().unwrap_or(0.0);
    if prompt == 0.0 && completion == 0.0 {
        return Some("free".to_string());
    }
    if completion > 0.0 {
        let per_million = |price: f64| trim_zeros(format!("{:.2}", price * 1e6));
        return Some(format!(
            "${} in / ${} out per 1M tokens",
            per_million(prompt),
            per_million(completion)
        ));
    }
    // No real per-second price comes near a cent, so a value this large was
    // already given per hour.
    let per_hour = if prompt >= 0.01 {
        prompt
    } else {
        prompt * 3600.0
    };
    Some(if per_hour < 0.005 {
        "<$0.01/h".to_string()
    } else {
        format!("${per_hour:.2}/h")
    })
}

/// "10.00" -> "10", "2.50" -> "2.5".
fn trim_zeros(value: String) -> String {
    value
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

/// The language to pin the provider to, from Handy's language settings.
/// `None` lets the provider auto-detect.
pub fn language_hint(selected: &str, spoken: &[String]) -> Option<String> {
    let code = match (selected, spoken) {
        ("auto", [only]) => only.as_str(),
        ("auto", _) => return None,
        (explicit, _) => explicit,
    };
    // The API takes ISO-639-1 ("zh", not "zh-Hans" or "pt_BR").
    let base = code.split(['-', '_']).next().unwrap_or_default();
    (!base.is_empty()).then(|| base.to_lowercase())
}

/// 16 kHz mono 16-bit WAV of `samples`, in memory.
pub fn wav_bytes(samples: &[f32]) -> Result<Vec<u8>> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut out = Cursor::new(Vec::with_capacity(44 + samples.len() * 2));
    let mut writer = hound::WavWriter::new(&mut out, spec)?;
    for sample in samples {
        writer.write_sample((sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)?;
    }
    writer.finalize()?;
    Ok(out.into_inner())
}

/// JSON body for `POST /audio/transcriptions`.
pub fn transcription_request(model: &str, wav: &[u8], language: Option<&str>) -> Value {
    let mut body = json!({
        "model": model,
        "input_audio": {
            "data": base64::engine::general_purpose::STANDARD.encode(wav),
            "format": "wav",
        },
    });
    if let Some(language) = language {
        body["language"] = json!(language);
    }
    body
}

/// The provider's own message from an error body, if it sent one.
fn error_message(body: &str) -> Option<String> {
    let value: Value = serde_json::from_str(body).ok()?;
    value["error"]["message"].as_str().map(str::to_string)
}

fn check_status(what: &str, status: u16, body: &str) -> Result<()> {
    if (200..300).contains(&status) {
        return Ok(());
    }
    match error_message(body) {
        Some(message) => Err(anyhow!("{what} failed ({status}): {message}")),
        None => Err(anyhow!("{what} failed ({status})")),
    }
}

/// The transcript from a `/audio/transcriptions` response.
pub fn parse_transcription(status: u16, body: &str) -> Result<String> {
    check_status("Cloud transcription", status, body)?;
    let value: Value = serde_json::from_str(body).context("Unreadable transcription response")?;
    value["text"]
        .as_str()
        .map(|text| text.trim().to_string())
        .ok_or_else(|| anyhow!("Transcription response had no text"))
}

/// The models from a `/models?output_modalities=transcription` response.
pub fn parse_models(body: &str) -> Result<Vec<CloudModel>> {
    let value: Value = serde_json::from_str(body).context("Unreadable model list")?;
    let data = value["data"]
        .as_array()
        .ok_or_else(|| anyhow!("Model list response had no data"))?;
    Ok(data
        .iter()
        .filter_map(|m| {
            let id = m["id"].as_str()?;
            let name = m["name"].as_str().unwrap_or(id);
            let pricing = &m["pricing"];
            Some(CloudModel {
                id: id.to_string(),
                name: name.to_string(),
                price: price_label(
                    pricing["prompt"].as_str().unwrap_or_default(),
                    pricing["completion"].as_str().unwrap_or_default(),
                ),
            })
        })
        .collect())
}

fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder().user_agent("Handy").build()?)
}

pub async fn list_models(base_url: &str, api_key: &str) -> Result<Vec<CloudModel>> {
    let url = format!(
        "{}/models?output_modalities=transcription",
        base_url.trim_end_matches('/')
    );
    let response = client()?
        .get(url)
        .bearer_auth(api_key)
        .timeout(MODELS_TIMEOUT)
        .send()
        .await
        .context("Could not reach the model list")?;
    let status = response.status().as_u16();
    let body = response.text().await.context("Could not read model list")?;
    check_status("Model list request", status, &body)?;
    parse_models(&body)
}

pub async fn transcribe(
    base_url: &str,
    api_key: &str,
    model: &str,
    samples: &[f32],
    language: Option<&str>,
) -> Result<String> {
    let request = transcription_request(model, &wav_bytes(samples)?, language);
    let response = client()?
        .post(format!(
            "{}/audio/transcriptions",
            base_url.trim_end_matches('/')
        ))
        .bearer_auth(api_key)
        .timeout(TRANSCRIBE_TIMEOUT)
        .json(&request)
        .send()
        .await
        .context("Could not reach cloud transcription")?;
    let status = response.status().as_u16();
    let body = response
        .text()
        .await
        .context("Could not read cloud transcription response")?;
    parse_transcription(status, &body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use std::collections::HashMap;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    // Recorded from the live API (see tests/fixtures/cloud_stt).
    const MODELS_OK: &str =
        include_str!("../../tests/fixtures/cloud_stt/models_transcription.json");
    const TRANSCRIPTION_OK: &str =
        include_str!("../../tests/fixtures/cloud_stt/transcription_ok.json");
    const UNAUTHORIZED: &str =
        include_str!("../../tests/fixtures/cloud_stt/error_unauthorized.json");
    const FOX_WAV: &[u8] = include_bytes!("../../tests/fixtures/cloud_stt/fox_16k_mono.wav");

    fn fox_samples() -> Vec<f32> {
        hound::WavReader::new(FOX_WAV)
            .unwrap()
            .samples::<i16>()
            .map(|s| s.unwrap() as f32 / i16::MAX as f32)
            .collect()
    }

    fn codes(c: &[&str]) -> Vec<String> {
        c.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn explicit_language_is_passed_as_its_base_code() {
        assert_eq!(language_hint("de", &[]), Some("de".into()));
        assert_eq!(language_hint("zh-Hans", &[]), Some("zh".into()));
        assert_eq!(language_hint("pt_BR", &[]), Some("pt".into()));
    }

    #[test]
    fn auto_with_one_spoken_language_pins_it() {
        assert_eq!(language_hint("auto", &codes(&["en"])), Some("en".into()));
    }

    #[test]
    fn auto_with_zero_or_many_spoken_languages_lets_provider_detect() {
        assert_eq!(language_hint("auto", &[]), None);
        assert_eq!(language_hint("auto", &codes(&["en", "zh"])), None);
    }

    #[test]
    fn wav_bytes_is_16k_mono_16bit_with_every_sample() {
        let samples = fox_samples();
        let wav = wav_bytes(&samples).unwrap();
        let reader = hound::WavReader::new(wav.as_slice()).unwrap();
        let spec = reader.spec();
        assert_eq!(
            (spec.channels, spec.sample_rate, spec.bits_per_sample),
            (1, 16_000, 16)
        );
        assert_eq!(reader.len() as usize, samples.len());
    }

    #[test]
    fn request_carries_model_base64_wav_and_language() {
        let body = transcription_request("openai/whisper-1", FOX_WAV, Some("en"));
        assert_eq!(body["model"], "openai/whisper-1");
        assert_eq!(body["language"], "en");
        assert_eq!(body["input_audio"]["format"], "wav");
        let data = body["input_audio"]["data"].as_str().unwrap();
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(data)
            .unwrap();
        assert_eq!(decoded, FOX_WAV);
    }

    #[test]
    fn request_omits_language_when_unset() {
        let body = transcription_request("openai/whisper-1", FOX_WAV, None);
        assert!(body.get("language").is_none());
    }

    #[test]
    fn parses_recorded_transcription() {
        assert_eq!(
            parse_transcription(200, TRANSCRIPTION_OK).unwrap(),
            "the quick brown fox jumps over the lazy dog."
        );
    }

    #[test]
    fn unauthorized_error_names_status_and_provider_message() {
        let err = parse_transcription(401, UNAUTHORIZED)
            .unwrap_err()
            .to_string();
        assert!(err.contains("401"), "{err}");
        assert!(err.contains("User not found."), "{err}");
    }

    #[test]
    fn non_json_error_body_still_reports_status() {
        let err = parse_transcription(502, "<html>bad gateway</html>")
            .unwrap_err()
            .to_string();
        assert!(err.contains("502"), "{err}");
    }

    #[test]
    fn success_without_text_is_an_error_not_an_empty_transcript() {
        assert!(parse_transcription(200, r#"{"usage":{}}"#).is_err());
    }

    #[test]
    fn parses_recorded_model_list() {
        let models = parse_models(MODELS_OK).unwrap();
        let ids: Vec<_> = models.iter().map(|m| m.id.as_str()).collect();
        assert!(ids.contains(&"openai/whisper-1"), "{ids:?}");
        assert_eq!(models.len(), 4);
        let whisper = models.iter().find(|m| m.id == "openai/whisper-1").unwrap();
        assert!(!whisper.name.is_empty());
    }

    const PRICING_VARIANTS: &str =
        include_str!("../../tests/fixtures/cloud_stt/models_pricing_variants.json");

    fn price_of(models: &[CloudModel], id: &str) -> Option<String> {
        models.iter().find(|m| m.id == id).unwrap().price.clone()
    }

    #[test]
    fn per_second_prices_become_dollars_per_hour() {
        let models = parse_models(MODELS_OK).unwrap();
        assert_eq!(
            price_of(&models, "openai/whisper-1"),
            Some("$0.36/h".into())
        );
        assert_eq!(
            price_of(&models, "x-ai/grok-stt-2.0"),
            Some("$0.10/h".into())
        );
        assert_eq!(
            price_of(&models, "mistralai/voxtral-mini-transcribe-3-0"),
            Some("$0.21/h".into())
        );
    }

    #[test]
    fn prices_already_per_hour_are_kept() {
        let models = parse_models(PRICING_VARIANTS).unwrap();
        assert_eq!(
            price_of(&models, "microsoft/mai-transcribe-1.5"),
            Some("$0.36/h".into())
        );
    }

    #[test]
    fn token_priced_models_show_per_million_rates() {
        let models = parse_models(PRICING_VARIANTS).unwrap();
        assert_eq!(
            price_of(&models, "openai/gpt-4o-transcribe"),
            Some("$2.5 in / $10 out per 1M tokens".into())
        );
    }

    #[test]
    fn free_unpriced_and_tiny_prices() {
        assert_eq!(price_label("0", "0"), Some("free".into()));
        assert_eq!(price_label("", ""), None);
        assert_eq!(price_label("n/a", "0"), None);
        assert_eq!(price_label("0.000001", "0"), Some("<$0.01/h".into()));
    }

    struct Captured {
        request_line: String,
        headers: HashMap<String, String>,
        body: String,
    }

    /// Serves one canned response on localhost and reports what it received.
    async fn serve_once(
        status: u16,
        body: &'static str,
    ) -> (String, tokio::task::JoinHandle<Captured>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let handle = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = Vec::new();
            let header_end = loop {
                let mut chunk = [0u8; 4096];
                let n = sock.read(&mut chunk).await.unwrap();
                buf.extend_from_slice(&chunk[..n]);
                if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                    break i + 4;
                }
            };
            let head = String::from_utf8_lossy(&buf[..header_end]).to_string();
            let mut lines = head.lines();
            let request_line = lines.next().unwrap().to_string();
            let headers: HashMap<String, String> = lines
                .filter_map(|l| l.split_once(':'))
                .map(|(k, v)| (k.trim().to_lowercase(), v.trim().to_string()))
                .collect();
            let len: usize = headers
                .get("content-length")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            while buf.len() < header_end + len {
                let mut chunk = [0u8; 65536];
                let n = sock.read(&mut chunk).await.unwrap();
                buf.extend_from_slice(&chunk[..n]);
            }
            let reply = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            sock.write_all(reply.as_bytes()).await.unwrap();
            Captured {
                request_line,
                headers,
                body: String::from_utf8_lossy(&buf[header_end..]).to_string(),
            }
        });
        (base, handle)
    }

    #[tokio::test]
    async fn transcribe_posts_audio_with_bearer_key_and_returns_text() {
        let (base, server) = serve_once(200, TRANSCRIPTION_OK).await;
        let text = transcribe(
            &base,
            "sk-test",
            "openai/whisper-1",
            &fox_samples(),
            Some("en"),
        )
        .await
        .unwrap();
        assert_eq!(text, "the quick brown fox jumps over the lazy dog.");
        let got = server.await.unwrap();
        assert!(got.request_line.starts_with("POST /audio/transcriptions "));
        assert_eq!(got.headers["authorization"], "Bearer sk-test");
        let json: Value = serde_json::from_str(&got.body).unwrap();
        assert_eq!(json["model"], "openai/whisper-1");
        assert_eq!(json["language"], "en");
    }

    #[tokio::test]
    async fn transcribe_surfaces_rejected_key() {
        let (base, _server) = serve_once(401, UNAUTHORIZED).await;
        let err = transcribe(&base, "bad", "openai/whisper-1", &fox_samples(), None)
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("401"), "{err}");
    }

    #[tokio::test]
    async fn list_models_asks_only_for_transcription_models() {
        let (base, server) = serve_once(200, MODELS_OK).await;
        let models = list_models(&base, "sk-test").await.unwrap();
        assert_eq!(models.len(), 4);
        let got = server.await.unwrap();
        assert!(
            got.request_line
                .starts_with("GET /models?output_modalities=transcription "),
            "{}",
            got.request_line
        );
        assert_eq!(got.headers["authorization"], "Bearer sk-test");
    }

    /// Smoke test against the real API. Run with
    /// `OPENROUTER_API_KEY=... cargo test --lib live_ -- --ignored`.
    #[tokio::test]
    #[ignore = "needs OPENROUTER_API_KEY and network"]
    async fn live_lists_models_and_transcribes_speech() {
        let key = std::env::var("OPENROUTER_API_KEY").expect("OPENROUTER_API_KEY");
        let models = list_models(DEFAULT_BASE_URL, &key).await.unwrap();
        assert!(models.iter().any(|m| m.id == "openai/whisper-1"));
        let text = transcribe(
            DEFAULT_BASE_URL,
            &key,
            "openai/whisper-1",
            &fox_samples(),
            Some("en"),
        )
        .await
        .unwrap();
        assert!(text.to_lowercase().contains("quick brown fox"), "{text}");
    }
}
