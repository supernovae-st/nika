// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The LOCAL adapter — a user-run OpenAI-images-compatible server. THE
//! sovereign image path (supernovae-alignment Rule 3: local-first).
//!
//! Wire facts (landscape verified 2026-07-05): ONE route now covers five
//! self-hosted servers — `LocalAI` (`:8080` · first-party, spec-complete),
//! Ollama (`:11434` · experimental, `macOS`-first, `n` unsupported),
//! stable-diffusion.cpp `sd-server` (`:1234`), `SGLang` Diffusion and
//! `vLLM-Omni` — all speaking `POST {base}/v1/images/generations` →
//! `data[].b64_json`. Two hard rules the landscape forces:
//!
//! 1. **`response_format: b64_json` is sent explicitly** — `LocalAI`
//!    DEFAULTS to url mode, and result URLs are never fetched.
//! 2. **`n` is best-effort** — Ollama ignores it; the pipeline's
//!    `count_shortfall:` warning reports what actually arrived.
//!
//! The base URL is ENGINE CONFIG (`NIKA_IMAGE_LOCAL_URL`, resolved at the
//! composition root — never workflow data, the same sanction as the const
//! cloud endpoints). Default: `LocalAI`'s `http://localhost:8080`. The
//! optional `NIKA_IMAGE_LOCAL_API_KEY` rides as a Bearer when the server
//! enforces one (`LocalAI` `--api-keys`).

use nika_kernel::io::http::{HttpError, HttpPostDyn, HttpRequest};
use nika_kernel::secret::Secret;

use crate::BuiltinFailure;

use super::args::ImageArgs;
use super::types::{
    C_NO_IMAGE, C_REQUEST, ProviderBatch, Quality, RawImage, SizeSpec, Usage, sanitize_raw,
};

/// The zero-config default — `LocalAI`'s conventional port.
pub(crate) const DEFAULT_BASE_URL: &str = "http://localhost:8080";

/// Run one generation batch against the local server.
pub(crate) async fn generate<H: HttpPostDyn>(
    http: &H,
    base_url: &str,
    key: Option<&Secret>,
    args: &ImageArgs,
) -> Result<ProviderBatch, BuiltinFailure> {
    let (request, warnings) = build_request(base_url, key, args)?;
    let response = http.post(request).await.map_err(map_transport)?;
    let mut batch = parse_response(response.status, &response.body, base_url, args)?;
    batch.warnings.splice(0..0, warnings);
    Ok(batch)
}

/// `{base}/v1/images/generations` with the trailing-slash class handled.
fn endpoint_of(base_url: &str) -> String {
    format!("{}/v1/images/generations", base_url.trim_end_matches('/'))
}

/// The HOST of the configured base (observability + manifest provenance —
/// which server rendered this asset). Best-effort split, display-only:
/// the value never gates anything.
pub(crate) fn host_of(base_url: &str) -> String {
    base_url
        .split("://")
        .nth(1)
        .unwrap_or(base_url)
        .split('/')
        .next()
        .unwrap_or(base_url)
        .to_owned()
}

/// Build the wire request (pure — unit-testable without a transport).
fn build_request(
    base_url: &str,
    key: Option<&Secret>,
    args: &ImageArgs,
) -> Result<(HttpRequest, Vec<String>), BuiltinFailure> {
    let mut warnings = Vec::new();
    let mut body = serde_json::Map::new();
    body.insert("model".into(), args.model.clone().into());
    body.insert("prompt".into(), args.prompt.clone().into());
    body.insert("n".into(), args.n.into());
    // LocalAI defaults to url mode — the payload must ride the body.
    body.insert("response_format".into(), "b64_json".into());
    if let Some(size) = resolve_size(args) {
        body.insert("size".into(), size.into());
    }
    push_capability_warnings(args, &mut warnings);

    let mut request = HttpRequest::post(endpoint_of(base_url));
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
    Ok((request, warnings))
}

/// The compat wire takes arbitrary `WxH` — exact sizes pass through, an
/// `aspect_ratio:` computes the same pairs the openai map uses (servers
/// accept or internally round; SD-family models prefer /64 grids and
/// handle the difference themselves).
fn resolve_size(args: &ImageArgs) -> Option<String> {
    match args.size {
        SizeSpec::Auto => None,
        SizeSpec::Exact { width, height } => Some(format!("{width}x{height}")),
        SizeSpec::Aspect(ratio) => {
            let (width, height) = ratio.openai_size();
            Some(format!("{width}x{height}"))
        }
    }
}

/// The local-wire capability gaps, warned loudly.
fn push_capability_warnings(args: &ImageArgs, warnings: &mut Vec<String>) {
    if args.quality != Quality::Auto {
        warnings.push(format!(
            "quality_ignored: the compat wire has no quality field — `{}` not sent \
             (LocalAI accepts-and-ignores it; pick the model instead)",
            args.quality.name()
        ));
    }
    if args.compression.is_some() {
        warnings.push(
            "compression_unsupported: the compat wire has no compression field — \
             the value is not sent"
                .to_owned(),
        );
    }
}

fn map_transport(error: HttpError) -> BuiltinFailure {
    match error {
        HttpError::Timeout { duration_ms } => BuiltinFailure::new(
            C_REQUEST,
            format!(
                "local image server timed out after {duration_ms}ms — CPU renders \
                 routinely run minutes; raise `timeout_ms:` (max 600000)"
            ),
        )
        .with_transient(true),
        HttpError::Connection { reason } => BuiltinFailure::new(
            C_REQUEST,
            format!(
                "no local image server answered ({reason}) — start one (LocalAI \
                 :8080 · Ollama :11434 · sd-server :1234) or point \
                 NIKA_IMAGE_LOCAL_URL at it"
            ),
        )
        .with_transient(true),
        HttpError::TooLarge { size, max } => BuiltinFailure::new(
            C_REQUEST,
            format!("local server response was {size} bytes (cap {max}) — lower `n:`"),
        ),
        other => BuiltinFailure::new(C_REQUEST, format!("local image request failed: {other}")),
    }
}

/// Parse the provider response (pure — fixture-testable).
fn parse_response(
    status: u16,
    body: &[u8],
    base_url: &str,
    args: &ImageArgs,
) -> Result<ProviderBatch, BuiltinFailure> {
    if !(200..300).contains(&status) {
        return Err(map_error_status(status, body));
    }
    let parsed: serde_json::Value = serde_json::from_slice(body).map_err(|e| {
        BuiltinFailure::new(
            C_NO_IMAGE,
            format!("the local server returned a non-JSON 2xx body ({e})"),
        )
    })?;
    let data = parsed
        .get("data")
        .and_then(serde_json::Value::as_array)
        .filter(|entries| !entries.is_empty())
        .ok_or_else(|| {
            BuiltinFailure::new(
                C_NO_IMAGE,
                "the local server returned no images (`data` is empty)",
            )
        })?;

    let mut images = Vec::with_capacity(data.len());
    for entry in data {
        let Some(b64) = entry.get("b64_json").and_then(serde_json::Value::as_str) else {
            if entry.get("url").is_some() {
                return Err(BuiltinFailure::new(
                    C_NO_IMAGE,
                    "the local server answered with a result URL despite \
                     `response_format: b64_json` — the engine never fetches result \
                     URLs; update the server (LocalAI honors request-level \
                     response_format) or switch it to b64 mode",
                ));
            }
            return Err(BuiltinFailure::new(
                C_NO_IMAGE,
                "local image entry carries no `b64_json` payload — malformed response",
            ));
        };
        let bytes = crate::data::base64_decode(b64).map_err(|e| {
            BuiltinFailure::new(
                C_NO_IMAGE,
                format!("local server base64 payload is corrupt: {e}"),
            )
        })?;
        images.push(RawImage {
            bytes,
            revised_prompt: None, // no compat-wire equivalent
            seed: None,
        });
    }

    let raw_debug = args.debug.then(|| {
        let mut raw = parsed.clone();
        sanitize_raw(&mut raw);
        raw
    });
    Ok(ProviderBatch {
        images,
        usage: Usage::default(), // self-hosted — no billing axes to report
        endpoint_host: Some(host_of(base_url)),
        provider_text: None,
        warnings: Vec::new(),
        raw_debug,
    })
}

/// Map a non-2xx envelope best-effort (server families differ — `LocalAI`
/// speaks the `OpenAI` error shape, others send plain text).
fn map_error_status(status: u16, body: &[u8]) -> BuiltinFailure {
    let envelope: serde_json::Value = serde_json::from_slice(body).unwrap_or_default();
    let message: String = envelope
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(serde_json::Value::as_str)
        .map_or_else(|| String::from_utf8_lossy(body).into_owned(), str::to_owned)
        .chars()
        .take(300)
        .collect();
    BuiltinFailure::new(
        C_REQUEST,
        format!("local image server HTTP {status}: {message}"),
    )
    .with_transient(matches!(status, 500..=599 | 408 | 429))
    .with_details(serde_json::json!({ "status_code": status }))
}

#[cfg(test)]
mod tests {
    use nika_kernel_mock::MockHttp;

    use super::super::args;
    use super::*;

    fn parsed(patch: serde_json::Value) -> ImageArgs {
        let serde_json::Value::Object(mut map) = serde_json::json!({
            "provider": "local", "prompt": "an aurora over a glacier",
            "output_dir": "./out"
        }) else {
            unreachable!()
        };
        if let serde_json::Value::Object(p) = patch {
            for (k, v) in p {
                map.insert(k, v);
            }
        }
        args::parse(&map).expect("valid")
    }

    fn wire_png_b64() -> (String, Vec<u8>) {
        let mock_args = args::parse(
            serde_json::json!({
                "provider": "mock", "prompt": "fixture", "output_dir": ".",
                "size": "24x16"
            })
            .as_object()
            .expect("object"),
        )
        .expect("valid");
        let batch = super::super::mock::generate(&mock_args).expect("mock png");
        let bytes = batch.images.into_iter().next().expect("one").bytes;
        (crate::data::base64_encode(&bytes), bytes)
    }

    #[tokio::test]
    async fn happy_path_speaks_the_compat_wire_keyless() {
        let (b64, bytes) = wire_png_b64();
        let http = MockHttp::new().enqueue_ok(
            200,
            serde_json::json!({ "data": [{ "b64_json": b64 }] })
                .to_string()
                .into_bytes(),
        );
        let args = parsed(serde_json::json!({ "size": "512x512", "n": 2 }));
        let batch = generate(&http, DEFAULT_BASE_URL, None, &args)
            .await
            .expect("generates");
        assert_eq!(
            batch.images.len(),
            1,
            "count_shortfall is the pipeline's job"
        );
        assert_eq!(batch.images[0].bytes, bytes);
        assert_eq!(batch.endpoint_host.as_deref(), Some("localhost:8080"));

        let sent = http.sent_requests();
        let request = &sent[0];
        assert_eq!(request.url, "http://localhost:8080/v1/images/generations");
        assert!(
            !request.headers.contains_key("authorization"),
            "keyless by default — sovereignty means no mandatory credential"
        );
        let body: serde_json::Value =
            serde_json::from_slice(request.body.as_ref().expect("body")).expect("json");
        assert_eq!(
            body["model"], "stablediffusion",
            "the LocalAI convention default"
        );
        assert_eq!(body["size"], "512x512");
        assert_eq!(body["n"], 2);
        assert_eq!(
            body["response_format"], "b64_json",
            "LocalAI defaults to url mode — the engine must ask"
        );
    }

    #[tokio::test]
    async fn custom_base_url_and_optional_key_ride_correctly() {
        let (b64, _) = wire_png_b64();
        let http = MockHttp::new().enqueue_ok(
            200,
            serde_json::json!({ "data": [{ "b64_json": b64 }] })
                .to_string()
                .into_bytes(),
        );
        let key = Secret::new("local-KEY-do-not-leak".to_owned());
        // Trailing slash + custom port + aspect_ratio → computed WxH.
        let args = parsed(serde_json::json!({
            "aspect_ratio": "16:9", "model": "x/z-image-turbo"
        }));
        generate(&http, "http://127.0.0.1:11434/", Some(&key), &args)
            .await
            .expect("generates");
        let request = &http.sent_requests()[0];
        assert_eq!(
            request.url, "http://127.0.0.1:11434/v1/images/generations",
            "trailing slash folded"
        );
        assert_eq!(
            request.headers.get("authorization").map(String::as_str),
            Some("Bearer local-KEY-do-not-leak")
        );
        let body: serde_json::Value =
            serde_json::from_slice(request.body.as_ref().expect("body")).expect("json");
        assert_eq!(body["model"], "x/z-image-turbo");
        assert_eq!(
            body["size"], "1536x864",
            "aspect computes the same map pairs"
        );
    }

    #[tokio::test]
    async fn url_only_responses_are_refused_with_the_localai_hint() {
        let http = MockHttp::new().enqueue_ok(
            200,
            serde_json::json!({
                "data": [{ "url": "http://localhost:8080/generated-images/x.png" }]
            })
            .to_string()
            .into_bytes(),
        );
        let err = generate(
            &http,
            DEFAULT_BASE_URL,
            None,
            &parsed(serde_json::json!({})),
        )
        .await
        .expect_err("refused");
        assert_eq!(err.code, C_NO_IMAGE);
        assert!(err.message.contains("never fetches"), "{}", err.message);
        assert_eq!(http.sent_requests().len(), 1, "no follow-up fetch");
    }

    #[tokio::test]
    async fn connection_refused_hints_the_server_landscape() {
        let http = MockHttp::new().enqueue_err(HttpError::Connection {
            reason: "connection refused".into(),
        });
        let err = generate(
            &http,
            DEFAULT_BASE_URL,
            None,
            &parsed(serde_json::json!({})),
        )
        .await
        .expect_err("down");
        assert!(err.transient, "a down server is retryable once started");
        assert!(err.message.contains("LocalAI"), "{}", err.message);
        assert!(
            err.message.contains("NIKA_IMAGE_LOCAL_URL"),
            "{}",
            err.message
        );
    }

    #[tokio::test]
    async fn plain_text_errors_parse_best_effort_and_key_never_leaks() {
        let http = MockHttp::new().enqueue_ok(500, b"model failed to load".to_vec());
        let key = Secret::new("local-KEY-do-not-leak".to_owned());
        let err = generate(
            &http,
            DEFAULT_BASE_URL,
            Some(&key),
            &parsed(serde_json::json!({})),
        )
        .await
        .expect_err("fails");
        assert!(err.transient, "5xx retryable");
        assert!(
            err.message.contains("model failed to load"),
            "{}",
            err.message
        );
        assert!(!err.message.contains("local-KEY"), "{}", err.message);
    }

    #[test]
    fn quality_and_compression_warn_and_host_parses() {
        let args = parsed(serde_json::json!({
            "quality": "high", "format": "webp", "compression": 70
        }));
        let (_, warnings) = build_request(DEFAULT_BASE_URL, None, &args).expect("builds");
        assert!(warnings.iter().any(|w| w.starts_with("quality_ignored:")));
        assert!(
            warnings
                .iter()
                .any(|w| w.starts_with("compression_unsupported:"))
        );
        assert_eq!(host_of("http://localhost:8080"), "localhost:8080");
        assert_eq!(host_of("http://127.0.0.1:1234/api"), "127.0.0.1:1234");
        assert_eq!(host_of("nonsense"), "nonsense", "display-only · total");
    }
}
