// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The LOCAL TTS adapter — one `OpenAI`-speech-compatible wire covers
//! `LocalAI` (also `ElevenLabs`-compatible upstream), Kokoro-FastAPI,
//! Speaches and openedai-speech: `POST {base}/v1/audio/speech` → raw
//! audio bytes. The base URL is ENGINE CONFIG (`NIKA_TTS_LOCAL_URL` ·
//! default `LocalAI`'s `http://localhost:8080`), never workflow data —
//! the same sanctioned-boundary reasoning as the image family, and the
//! reflected-credential scrub applies here too.

use nika_kernel::io::http::{HttpError, HttpPostDyn, HttpRequest};
use nika_kernel::secret::Secret;

use crate::BuiltinFailure;
use crate::media::wire;

use super::args::TtsArgs;
use super::types::{AudioFormat, C_REQUEST, ProviderAudio};

pub(crate) const DEFAULT_BASE_URL: &str = "http://localhost:8080";

pub(crate) async fn generate<H: HttpPostDyn>(
    http: &H,
    base_url: &str,
    key: Option<&Secret>,
    args: &TtsArgs,
) -> Result<ProviderAudio, BuiltinFailure> {
    let request = build_request(base_url, key, args)?;
    let response = http.post(request).await.map_err(map_transport)?;
    if !(200..300).contains(&response.status) {
        return Err(map_error_status(response.status, &response.body, key));
    }
    Ok(ProviderAudio {
        bytes: response.body.to_vec(),
        cost_usd: None, // self-hosted — no billing axis
        endpoint_host: Some(super::super::image::local::host_of(base_url)),
        warnings: Vec::new(),
    })
}

fn build_request(
    base_url: &str,
    key: Option<&Secret>,
    args: &TtsArgs,
) -> Result<HttpRequest, BuiltinFailure> {
    let mut body = serde_json::Map::new();
    body.insert("model".into(), args.model.clone().into());
    body.insert("input".into(), args.text.clone().into());
    body.insert("voice".into(), args.voice.clone().into());
    body.insert(
        "response_format".into(),
        match args.format {
            Some(AudioFormat::Wav) => "wav",
            _ => "mp3",
        }
        .into(),
    );
    if args.speed_explicit {
        body.insert("speed".into(), args.speed_f64().into());
    }

    let endpoint = format!("{}/v1/audio/speech", base_url.trim_end_matches('/'));
    let mut request = HttpRequest::post(&endpoint);
    if let Some(key) = key {
        request.headers.insert(
            "authorization".to_owned(),
            format!("Bearer {}", key.expose()),
        );
    }
    request
        .headers
        .insert("content-type".to_owned(), "application/json".to_owned());
    request.timeout = Some(args.timeout);
    request.body = Some(
        serde_json::to_vec(&serde_json::Value::Object(body))
            .map_err(|e| BuiltinFailure::new(C_REQUEST, format!("request serialization: {e}")))?
            .into(),
    );
    Ok(request)
}

fn map_transport(error: HttpError) -> BuiltinFailure {
    match error {
        HttpError::Timeout { duration_ms } => BuiltinFailure::new(
            C_REQUEST,
            format!(
                "local TTS server timed out after {duration_ms}ms — CPU synthesis can run \
                 minutes on long texts; raise `timeout_ms:` (max 600000)"
            ),
        )
        .with_transient(true),
        HttpError::Connection { reason } => BuiltinFailure::new(
            C_REQUEST,
            format!(
                "no local TTS server answered ({reason}) — start one (LocalAI :8080 · \
                 Kokoro-FastAPI · Speaches) or point NIKA_TTS_LOCAL_URL at it"
            ),
        )
        .with_transient(true),
        HttpError::TooLarge { size, max } => BuiltinFailure::new(
            C_REQUEST,
            format!(
                "local TTS response was {size} bytes (cap {max}) — shorten `text:` or \\
                 split the script across tasks"
            ),
        ),
        other => BuiltinFailure::new(C_REQUEST, format!("local TTS request failed: {other}")),
    }
}

/// Best-effort error mapping with the raw-body fallback that makes local
/// errors USEFUL — and the same reflected-Bearer scrub the image family
/// carries (a verbose self-hosted server may echo the Authorization
/// header into its error body).
fn map_error_status(status: u16, body: &[u8], key: Option<&Secret>) -> BuiltinFailure {
    let envelope: serde_json::Value = serde_json::from_slice(body).unwrap_or_default();
    let mut message: String = envelope
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(serde_json::Value::as_str)
        .map_or_else(|| String::from_utf8_lossy(body).into_owned(), str::to_owned)
        .chars()
        .take(300)
        .collect();
    if let Some(key) = key {
        let exposed = key.expose();
        if !exposed.is_empty() && message.contains(exposed) {
            message = message.replace(exposed, "***");
        }
    }
    BuiltinFailure::new(
        C_REQUEST,
        format!("local TTS server HTTP {status}: {message}"),
    )
    .with_transient(wire::transient_status(status))
    .with_details(serde_json::json!({ "status_code": status }))
}

#[cfg(test)]
mod tests {
    use nika_kernel_mock::MockHttp;

    use super::super::args;
    use super::*;

    fn parsed() -> TtsArgs {
        let serde_json::Value::Object(map) = serde_json::json!({
            "provider": "local", "text": "bonjour", "output_dir": "./out"
        }) else {
            unreachable!()
        };
        args::parse(&map).expect("valid")
    }

    #[tokio::test]
    async fn keyless_default_custom_base_and_reflected_key_scrub() {
        let http = MockHttp::new().enqueue_ok(200, b"ID3\x04mp3".to_vec());
        let audio = generate(&http, DEFAULT_BASE_URL, None, &parsed())
            .await
            .expect("keyless ok");
        assert_eq!(audio.endpoint_host.as_deref(), Some("localhost:8080"));
        let sent = http.sent_requests();
        assert_eq!(sent[0].url, "http://localhost:8080/v1/audio/speech");
        assert!(!sent[0].headers.contains_key("authorization"));

        let http = MockHttp::new().enqueue_ok(200, b"ID3\x04mp3".to_vec());
        let key = Secret::new("local-TTS-key".to_owned());
        let _ = generate(&http, "http://10.0.0.5:8880/", Some(&key), &parsed())
            .await
            .expect("custom base");
        assert_eq!(
            http.sent_requests()[0].url,
            "http://10.0.0.5:8880/v1/audio/speech",
            "trailing slash folded"
        );

        let reflected = format!("unauthorized, you sent: Bearer {}", "local-TTS-key");
        let http = MockHttp::new().enqueue_ok(401, reflected.into_bytes());
        let err = generate(&http, DEFAULT_BASE_URL, Some(&key), &parsed())
            .await
            .expect_err("401");
        assert!(!err.message.contains("local-TTS-key"), "{}", err.message);
        assert!(err.message.contains("***"));
    }
}
