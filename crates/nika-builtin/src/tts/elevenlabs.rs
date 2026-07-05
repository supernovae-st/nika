// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `ElevenLabs` adapter — `POST /v1/text-to-speech/{voice_id}` (the voice
//! rides the PATH, so it is sanitized against injection), `xi-api-key`
//! header, raw audio bytes back. `output_format` rides the query string.

use nika_kernel::io::http::{HttpError, HttpPostDyn, HttpRequest};
use nika_kernel::secret::Secret;

use crate::BuiltinFailure;

use super::args::TtsArgs;
use super::types::{AudioFormat, C_ARGS, C_POLICY, C_REQUEST, ProviderAudio};

const ENDPOINT_STEM: &str = "https://api.elevenlabs.io/v1/text-to-speech/";

pub(crate) async fn generate<H: HttpPostDyn>(
    http: &H,
    key: &Secret,
    args: &TtsArgs,
) -> Result<ProviderAudio, BuiltinFailure> {
    let (request, warnings) = build_request(args, key)?;
    let response = http.post(request).await.map_err(map_transport)?;
    if !(200..300).contains(&response.status) {
        return Err(map_error_status(response.status, &response.body));
    }
    Ok(ProviderAudio {
        bytes: response.body.to_vec(),
        cost_usd: None, // subscription-quota billing · no response cost field
        endpoint_host: Some("api.elevenlabs.io".to_owned()),
        warnings,
    })
}

fn build_request(
    args: &TtsArgs,
    key: &Secret,
) -> Result<(HttpRequest, Vec<String>), BuiltinFailure> {
    let mut warnings = Vec::new();
    // The voice id rides the URL path — restrict to the id alphabet so a
    // crafted `voice:` can never smuggle path segments or a query string.
    if !args
        .voice
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        || args.voice.len() > 64
    {
        return Err(BuiltinFailure::new(
            C_ARGS,
            "elevenlabs `voice:` must be a voice id ([A-Za-z0-9_-] · ≤64 chars) — find ids \
             in your ElevenLabs voice library",
        ));
    }
    let format_param = match args.format {
        Some(AudioFormat::Wav) => {
            // The compat-plan reality: raw PCM tiers exist but plan-gated;
            // mp3 is the universal tier. The bytes decide the extension.
            warnings.push(
                "format_mismatch: elevenlabs serves mp3 on the standard tier — the wav ask \
                 rides as mp3 (the saved extension follows the bytes)"
                    .to_owned(),
            );
            "mp3_44100_128"
        }
        _ => "mp3_44100_128",
    };
    let url = format!("{ENDPOINT_STEM}{}?output_format={format_param}", args.voice);

    let body = serde_json::json!({
        "text": args.text,
        "model_id": args.model,
    });

    let mut request = HttpRequest::post(&url);
    request
        .headers
        .insert("xi-api-key".to_owned(), key.expose().to_owned());
    request
        .headers
        .insert("content-type".to_owned(), "application/json".to_owned());
    request.timeout = Some(args.timeout);
    request.body = Some(
        serde_json::to_vec(&body)
            .map_err(|e| BuiltinFailure::new(C_REQUEST, format!("request serialization: {e}")))?
            .into(),
    );
    Ok((request, warnings))
}

fn map_transport(error: HttpError) -> BuiltinFailure {
    match error {
        HttpError::Timeout { duration_ms } => BuiltinFailure::new(
            C_REQUEST,
            format!("elevenlabs timed out after {duration_ms}ms — raise `timeout_ms:`"),
        )
        .with_transient(true),
        HttpError::Connection { reason } => {
            BuiltinFailure::new(C_REQUEST, format!("elevenlabs connection failed: {reason}"))
                .with_transient(true)
        }
        HttpError::TooLarge { size, max } => BuiltinFailure::new(
            C_REQUEST,
            format!(
                "elevenlabs response was {size} bytes (cap {max}) — shorten `text:` or \\
                 split the script across tasks"
            ),
        ),
        other => BuiltinFailure::new(C_REQUEST, format!("elevenlabs request failed: {other}")),
    }
}

fn map_error_status(status: u16, body: &[u8]) -> BuiltinFailure {
    let envelope: serde_json::Value = serde_json::from_slice(body).unwrap_or_default();
    // ElevenLabs error shape: {"detail": {"status": "...", "message": "..."}}
    // (or a bare {"detail": "..."} string) — never the raw body (the local
    // adapter's reflected-credential lesson applies to every vendor).
    let message: String = envelope
        .get("detail")
        .map_or_else(
            || "no error message".to_owned(),
            |d| {
                d.get("message")
                    .and_then(serde_json::Value::as_str)
                    .or_else(|| d.as_str())
                    .unwrap_or("no error message")
                    .to_owned()
            },
        )
        .chars()
        .take(300)
        .collect();
    let code = if message.to_ascii_lowercase().contains("moderat") {
        C_POLICY
    } else {
        C_REQUEST
    };
    BuiltinFailure::new(code, format!("elevenlabs HTTP {status}: {message}"))
        .with_transient(matches!(status, 500..=599 | 408 | 429))
        .with_details(serde_json::json!({ "status_code": status }))
}

#[cfg(test)]
mod tests {
    use nika_kernel_mock::MockHttp;

    use super::super::args;
    use super::*;

    fn parsed(extra: serde_json::Value) -> TtsArgs {
        let serde_json::Value::Object(mut map) = serde_json::json!({
            "provider": "elevenlabs", "text": "bonjour", "output_dir": "./out"
        }) else {
            unreachable!()
        };
        if let serde_json::Value::Object(e) = extra {
            map.extend(e);
        }
        args::parse(&map).expect("valid")
    }

    #[test]
    fn voice_path_injection_is_structurally_impossible() {
        let key = Secret::new("xi-KEY".to_owned());
        let (request, _) = build_request(&parsed(serde_json::json!({})), &key).expect("builds");
        assert!(request.url.starts_with(ENDPOINT_STEM));
        assert!(request.url.contains("21m00Tcm4TlvDq8ikWAM"));
        assert_eq!(
            request.headers.get("xi-api-key").map(String::as_str),
            Some("xi-KEY")
        );
        for evil in ["../admin", "v?stream=true", "a/b", "x y", &"v".repeat(65)] {
            let err = build_request(&parsed(serde_json::json!({ "voice": evil })), &key)
                .expect_err("refused");
            assert!(err.code.ends_with("-001"), "{evil} → {}", err.code);
        }
    }

    #[tokio::test]
    async fn success_and_detail_error_shapes() {
        let key = Secret::new("xi-KEY-never-echoed".to_owned());
        let http = MockHttp::new().enqueue_ok(200, b"ID3\x04audio".to_vec());
        let audio = generate(&http, &key, &parsed(serde_json::json!({})))
            .await
            .expect("ok");
        assert_eq!(audio.endpoint_host.as_deref(), Some("api.elevenlabs.io"));

        let http = MockHttp::new().enqueue_ok(
            401,
            br#"{"detail":{"status":"invalid_api_key","message":"bad key"}}"#.to_vec(),
        );
        let err = generate(&http, &key, &parsed(serde_json::json!({})))
            .await
            .expect_err("401");
        assert!(err.message.contains("bad key"));
        assert!(!err.message.contains("never-echoed"), "{}", err.message);

        // wav ask on the standard tier warns + rides as mp3.
        let (req, warns) =
            build_request(&parsed(serde_json::json!({ "format": "wav" })), &key).expect("builds");
        assert!(req.url.contains("output_format=mp3_44100_128"));
        assert!(warns.iter().any(|w| w.starts_with("format_mismatch:")));
    }
}
