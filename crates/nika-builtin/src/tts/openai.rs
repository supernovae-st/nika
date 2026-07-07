// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `OpenAI` speech adapter — `POST /v1/audio/speech` returns RAW audio
//! bytes (no JSON envelope on success · a JSON error body on failure).

use nika_kernel::io::http::{HttpError, HttpPostDyn, HttpRequest};
use nika_kernel::secret::Secret;

use crate::BuiltinFailure;
use crate::wire;

use super::args::TtsArgs;
use super::types::{AudioFormat, C_POLICY, C_REQUEST, ProviderAudio};

const ENDPOINT: &str = "https://api.openai.com/v1/audio/speech";

pub(crate) async fn generate<H: HttpPostDyn>(
    http: &H,
    key: &Secret,
    args: &TtsArgs,
) -> Result<ProviderAudio, BuiltinFailure> {
    let request = build_request(args, key)?;
    let response = http.post(request).await.map_err(map_transport)?;
    if !(200..300).contains(&response.status) {
        return Err(map_error_status(response.status, &response.body));
    }
    Ok(ProviderAudio {
        bytes: response.body.to_vec(),
        cost_usd: None, // token-priced · audio models unpriced in the catalog
        endpoint_host: Some("api.openai.com".to_owned()),
        warnings: Vec::new(),
    })
}

/// Pure request builder (unit-testable without a transport).
fn build_request(args: &TtsArgs, key: &Secret) -> Result<HttpRequest, BuiltinFailure> {
    let mut body = serde_json::Map::new();
    body.insert("model".into(), args.model.clone().into());
    body.insert("input".into(), args.text.clone().into());
    body.insert("voice".into(), args.voice.clone().into());
    body.insert(
        "response_format".into(),
        match args.format {
            Some(AudioFormat::Wav) => "wav",
            // mp3 is both the explicit ask and the provider-native default.
            _ => "mp3",
        }
        .into(),
    );
    if args.speed_explicit {
        body.insert("speed".into(), args.speed_f64().into());
    }

    let mut request = HttpRequest::post(ENDPOINT);
    request.headers.insert(
        "authorization".to_owned(),
        format!("Bearer {}", key.expose()),
    );
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
            format!("openai speech timed out after {duration_ms}ms — raise `timeout_ms:`"),
        )
        .with_transient(true),
        HttpError::Connection { reason } => {
            BuiltinFailure::new(C_REQUEST, format!("openai connection failed: {reason}"))
                .with_transient(true)
        }
        HttpError::TooLarge { size, max } => BuiltinFailure::new(
            C_REQUEST,
            format!(
                "openai speech response was {size} bytes (cap {max}) — shorten `text:` or \\
                 split the script across tasks"
            ),
        ),
        other => BuiltinFailure::new(C_REQUEST, format!("openai request failed: {other}")),
    }
}

fn map_error_status(status: u16, body: &[u8]) -> BuiltinFailure {
    let envelope: serde_json::Value = serde_json::from_slice(body).unwrap_or_default();
    let message: String = envelope
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("no error message")
        .chars()
        .take(300)
        .collect();
    let code = if status == 400 && message.to_ascii_lowercase().contains("policy") {
        C_POLICY
    } else {
        C_REQUEST
    };
    BuiltinFailure::new(code, format!("openai speech HTTP {status}: {message}"))
        .with_transient(wire::transient_status(status))
        .with_details(serde_json::json!({ "status_code": status }))
}

#[cfg(test)]
mod tests {
    use nika_kernel_mock::MockHttp;

    use super::super::args;
    use super::*;

    fn parsed(extra: serde_json::Value) -> TtsArgs {
        let serde_json::Value::Object(mut map) = serde_json::json!({
            "provider": "openai", "text": "bonjour le monde", "output_dir": "./out"
        }) else {
            unreachable!()
        };
        if let serde_json::Value::Object(e) = extra {
            map.extend(e);
        }
        args::parse(&map).expect("valid")
    }

    #[test]
    fn request_shape_and_key_never_leaks_in_errors() {
        let key = Secret::new("sk-SPEECH-not-in-messages".to_owned());
        let request =
            build_request(&parsed(serde_json::json!({ "speed": 1.25 })), &key).expect("builds");
        assert_eq!(request.url, ENDPOINT);
        let body: serde_json::Value =
            serde_json::from_slice(request.body.as_ref().expect("body")).expect("json");
        assert_eq!(body["model"], "gpt-4o-mini-tts");
        assert_eq!(body["voice"], "alloy");
        assert_eq!(body["response_format"], "mp3");
        assert_eq!(body["speed"], 1.25);
        // default speed is omitted (provider default rules).
        let quiet = build_request(&parsed(serde_json::json!({})), &key).expect("builds");
        let qbody: serde_json::Value =
            serde_json::from_slice(quiet.body.as_ref().expect("body")).expect("json");
        assert!(qbody.get("speed").is_none());

        let err = map_error_status(401, br#"{"error":{"message":"bad key"}}"#);
        assert!(!err.message.contains("SPEECH"), "{}", err.message);
        assert!(!err.transient);
        assert!(map_error_status(429, b"{}").transient);
    }

    #[tokio::test]
    async fn raw_bytes_success_and_policy_mapping() {
        let http = MockHttp::new().enqueue_ok(200, b"ID3\x04fake-mp3".to_vec());
        let key = Secret::new("k".to_owned());
        let audio = generate(&http, &key, &parsed(serde_json::json!({})))
            .await
            .expect("ok");
        assert_eq!(&audio.bytes[..3], b"ID3");
        assert_eq!(audio.endpoint_host.as_deref(), Some("api.openai.com"));

        let http = MockHttp::new().enqueue_ok(
            400,
            br#"{"error":{"message":"content policy violation"}}"#.to_vec(),
        );
        let err = generate(&http, &key, &parsed(serde_json::json!({})))
            .await
            .expect_err("policy");
        assert!(err.code.ends_with("-005"), "{}", err.code);
    }
}
