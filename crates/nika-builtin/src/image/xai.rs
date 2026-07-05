// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The xAI Imagine adapter — `POST /v1/images/generations`.
//!
//! Wire facts (primary-source verified 2026-07-05, docs.x.ai): the Imagine
//! models take `aspect_ratio` (a 14-value enum) + `resolution` (`1k`|`2k`)
//! instead of exact sizes; `response_format` defaults to `url`, so the
//! engine requests `b64_json` explicitly (result URLs are never fetched);
//! `n` runs up to 10 natively; there is NO size/quality/style/seed knob
//! (the `-quality` model tier is the quality knob); usage is billed in
//! `cost_in_usd_ticks`, not tokens (surfaced only in the `debug:` echo).

use nika_kernel::io::http::{HttpError, HttpPostDyn, HttpRequest};
use nika_kernel::secret::Secret;

use crate::BuiltinFailure;

use super::args::ImageArgs;
use super::types::{
    C_NO_IMAGE, C_REQUEST, ProviderBatch, Quality, RawImage, SizeSpec, Usage, sanitize_raw,
};

/// The fixed endpoint — never workflow-controlled.
const ENDPOINT: &str = "https://api.x.ai/v1/images/generations";

/// The documented `aspect_ratio` enum (docs.x.ai REST reference · minus
/// `auto`, which the engine expresses by omitting the field).
const NATIVE_ASPECTS: [(&str, f64); 13] = [
    ("1:1", 1.0),
    ("3:4", 0.75),
    ("4:3", 4.0 / 3.0),
    ("9:16", 9.0 / 16.0),
    ("16:9", 16.0 / 9.0),
    ("2:3", 2.0 / 3.0),
    ("3:2", 1.5),
    ("9:19.5", 9.0 / 19.5),
    ("19.5:9", 19.5 / 9.0),
    ("9:20", 0.45),
    ("20:9", 20.0 / 9.0),
    ("1:2", 0.5),
    ("2:1", 2.0),
];

/// Run one generation batch against xAI (native `n` — one request).
pub(crate) async fn generate<H: HttpPostDyn>(
    http: &H,
    key: &Secret,
    args: &ImageArgs,
) -> Result<ProviderBatch, BuiltinFailure> {
    let (request, warnings) = build_request(args, key)?;
    let response = http.post(request).await.map_err(map_transport)?;
    let mut batch = parse_response(response.status, &response.body, args)?;
    batch.warnings.splice(0..0, warnings);
    Ok(batch)
}

/// Build the wire request (pure — unit-testable without a transport).
fn build_request(
    args: &ImageArgs,
    key: &Secret,
) -> Result<(HttpRequest, Vec<String>), BuiltinFailure> {
    let mut warnings = Vec::new();
    let mut body = serde_json::Map::new();
    body.insert("model".into(), args.model.clone().into());
    body.insert("prompt".into(), args.prompt.clone().into());
    body.insert("n".into(), args.n.into());
    // Result URLs are never fetched (the SSRF/net-boundary honesty rule) —
    // the payload must ride the response body.
    body.insert("response_format".into(), "b64_json".into());

    let (aspect, resolution) = resolve_shape(args, &mut warnings);
    if let Some(aspect) = aspect {
        body.insert("aspect_ratio".into(), aspect.into());
    }
    // An explicit provider option overrides the derived class.
    let resolution_override = args
        .provider_options
        .get("resolution")
        .and_then(serde_json::Value::as_str);
    if let Some(res) = resolution_override.or(resolution) {
        body.insert("resolution".into(), res.into());
    }
    if let Some(user) = args.provider_options.get("user") {
        body.insert("user".into(), user.clone());
    }
    push_capability_warnings(args, &mut warnings);

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
    Ok((request, warnings))
}

/// Fold the resolved size request onto xAI's `aspect_ratio` + `resolution`
/// classes — loudly, never silently.
fn resolve_shape(
    args: &ImageArgs,
    warnings: &mut Vec<String>,
) -> (Option<&'static str>, Option<&'static str>) {
    match args.size {
        SizeSpec::Auto => (None, None),
        SizeSpec::Aspect(ratio) => {
            let name = ratio.name();
            if let Some((native, _)) = NATIVE_ASPECTS.iter().find(|(n, _)| *n == name) {
                (Some(native), None)
            } else {
                // 21:9 is the one common ratio xAI lacks — 20:9 is nearest.
                warnings.push(format!(
                    "aspect_remapped: xai has no `{name}` — sent as `20:9` (the \
                     nearest native ratio)"
                ));
                (Some("20:9"), None)
            }
        }
        SizeSpec::Exact { width, height } => {
            let target = f64::from(width) / f64::from(height);
            let nearest = NATIVE_ASPECTS
                .iter()
                .min_by(|(_, a), (_, b)| {
                    (a - target)
                        .abs()
                        .partial_cmp(&(b - target).abs())
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map_or("1:1", |(n, _)| *n);
            let resolution = if width.max(height) > 1_024 {
                "2k"
            } else {
                "1k"
            };
            warnings.push(format!(
                "xai_size_class: xai renders aspect/resolution classes — \
                 {width}x{height} sent as `{nearest}` @ {resolution} (exact pixel \
                 sizes are an openai gpt-image-2 feature)"
            ));
            (Some(nearest), Some(resolution))
        }
    }
}

/// The xAI capability gaps — every tolerated-but-lossy mapping lands its
/// stable `code: message` warning (honest-degradation contract).
fn push_capability_warnings(args: &ImageArgs, warnings: &mut Vec<String>) {
    if args.quality != Quality::Auto {
        warnings.push(format!(
            "quality_is_model_tier: xai has no quality knob — `{}` not sent; pick \
             the model tier instead (grok-imagine-image-quality for peak quality)",
            args.quality.name()
        ));
    }
    if args.compression.is_some() {
        warnings.push(
            "compression_unsupported: xai has no compression knob — the value is \
             not sent"
                .to_owned(),
        );
    }
}

fn map_transport(error: HttpError) -> BuiltinFailure {
    match error {
        HttpError::Timeout { duration_ms } => BuiltinFailure::new(
            C_REQUEST,
            format!(
                "xai request timed out after {duration_ms}ms — raise `timeout_ms:` \
                 if needed"
            ),
        )
        .with_transient(true),
        HttpError::Connection { reason } => {
            BuiltinFailure::new(C_REQUEST, format!("xai connection failed: {reason}"))
                .with_transient(true)
        }
        HttpError::TooLarge { size, max } => BuiltinFailure::new(
            C_REQUEST,
            format!("xai response was {size} bytes (cap {max}) — lower `n:`"),
        ),
        other => BuiltinFailure::new(C_REQUEST, format!("xai request failed: {other}")),
    }
}

/// Parse the provider response (pure — fixture-testable).
fn parse_response(
    status: u16,
    body: &[u8],
    args: &ImageArgs,
) -> Result<ProviderBatch, BuiltinFailure> {
    if !(200..300).contains(&status) {
        return Err(map_error_status(status, body));
    }
    let parsed: serde_json::Value = serde_json::from_slice(body).map_err(|e| {
        BuiltinFailure::new(
            C_NO_IMAGE,
            format!("xai returned a non-JSON 2xx body ({e}) — malformed response"),
        )
    })?;
    let data = parsed
        .get("data")
        .and_then(serde_json::Value::as_array)
        .filter(|entries| !entries.is_empty())
        .ok_or_else(|| {
            BuiltinFailure::new(
                C_NO_IMAGE,
                "xai returned no images (`data` is empty) — nothing to save",
            )
        })?;

    let mut images = Vec::with_capacity(data.len());
    for entry in data {
        let Some(b64) = entry.get("b64_json").and_then(serde_json::Value::as_str) else {
            // The engine asked for b64_json; a url-only entry means the
            // request-level response_format was not honored — refusing to
            // fetch result URLs is the boundary-honesty rule.
            if entry.get("url").is_some() {
                return Err(BuiltinFailure::new(
                    C_NO_IMAGE,
                    "xai answered with a result URL despite `response_format: \
                     b64_json` — the engine never fetches result URLs; retry, or \
                     report the api regression",
                ));
            }
            return Err(BuiltinFailure::new(
                C_NO_IMAGE,
                "xai image entry carries no `b64_json` payload — malformed response \
                 (possibly a moderation-filtered image)",
            ));
        };
        let bytes = crate::data::base64_decode(b64).map_err(|e| {
            BuiltinFailure::new(C_NO_IMAGE, format!("xai base64 payload is corrupt: {e}"))
        })?;
        images.push(RawImage {
            bytes,
            // grok-imagine responses may carry a vestigial empty
            // `revised_prompt` — only a non-empty one is provenance.
            revised_prompt: entry
                .get("revised_prompt")
                .and_then(serde_json::Value::as_str)
                .filter(|s| !s.is_empty())
                .map(|s| s.chars().take(2_000).collect()),
            seed: None, // no seed on the Imagine API
        });
    }

    // Billed in cost ticks, not tokens — the normalized usage axes stay
    // honestly null; the tick count rides the debug echo only.
    let raw_debug = args.debug.then(|| {
        let mut raw = parsed.clone();
        sanitize_raw(&mut raw);
        raw
    });
    Ok(ProviderBatch {
        images,
        usage: Usage::default(),
        endpoint_host: Some("api.x.ai".to_owned()),
        provider_text: None,
        warnings: Vec::new(),
        raw_debug,
    })
}

/// Map a non-2xx envelope to the spec error planes (xAI documents status
/// codes 400/401/403/404/405/415/422/429 but no envelope schema — parse
/// best-effort, never trust shape).
fn map_error_status(status: u16, body: &[u8]) -> BuiltinFailure {
    let envelope: serde_json::Value = serde_json::from_slice(body).unwrap_or_default();
    let message: String = envelope
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(serde_json::Value::as_str)
        .or_else(|| envelope.get("error").and_then(serde_json::Value::as_str))
        .or_else(|| envelope.get("msg").and_then(serde_json::Value::as_str))
        .unwrap_or("no error message")
        .chars()
        .take(300)
        .collect();
    BuiltinFailure::new(C_REQUEST, format!("xai HTTP {status}: {message}"))
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
            "provider": "xai", "prompt": "a monarch butterfly over a nebula",
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

    fn key() -> Secret {
        Secret::new("xai-test-KEY-do-not-leak".to_owned())
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
    async fn happy_path_builds_the_documented_request_and_decodes_b64() {
        let (b64, bytes) = wire_png_b64();
        let response = serde_json::json!({
            "data": [{ "b64_json": b64, "revised_prompt": "" }]
        });
        let http = MockHttp::new().enqueue_ok(200, response.to_string().into_bytes());
        let args = parsed(serde_json::json!({
            "aspect_ratio": "16:9", "n": 2,
            "provider_options": { "user": "qrcodeai-batch" }
        }));
        let batch = generate(&http, &key(), &args).await.expect("generates");
        assert_eq!(batch.images.len(), 1);
        assert_eq!(batch.images[0].bytes, bytes);
        assert_eq!(
            batch.images[0].revised_prompt, None,
            "vestigial empty revised_prompt is not provenance"
        );
        assert_eq!(batch.usage.input_tokens, None, "ticks are not tokens");
        assert_eq!(batch.endpoint_host.as_deref(), Some("api.x.ai"));

        let sent = http.sent_requests();
        assert_eq!(sent.len(), 1);
        let request = &sent[0];
        assert_eq!(request.url, ENDPOINT);
        assert_eq!(
            request.headers.get("authorization").map(String::as_str),
            Some("Bearer xai-test-KEY-do-not-leak")
        );
        let body: serde_json::Value =
            serde_json::from_slice(request.body.as_ref().expect("body")).expect("json");
        assert_eq!(body["model"], "grok-imagine-image");
        assert_eq!(body["n"], 2);
        assert_eq!(
            body["response_format"], "b64_json",
            "urls are never fetched"
        );
        assert_eq!(body["aspect_ratio"], "16:9");
        assert_eq!(body["user"], "qrcodeai-batch");
        assert!(body.get("size").is_none(), "xai has no size param");
        assert!(body.get("quality").is_none(), "xai has no quality param");
    }

    #[test]
    fn cinematic_ratio_remaps_loudly_and_exact_sizes_fold_to_classes() {
        // 21:9 is absent from the native enum → 20:9 + a stable warning.
        let args = parsed(serde_json::json!({ "aspect_ratio": "21:9" }));
        let (request, warnings) = build_request(&args, &key()).expect("builds");
        let body: serde_json::Value =
            serde_json::from_slice(request.body.as_ref().expect("body")).expect("json");
        assert_eq!(body["aspect_ratio"], "20:9");
        assert!(warnings.iter().any(|w| w.starts_with("aspect_remapped:")));
        // An exact size folds to nearest-aspect + resolution class.
        let args = parsed(serde_json::json!({ "size": "2048x1152" }));
        let (request, warnings) = build_request(&args, &key()).expect("builds");
        let body: serde_json::Value =
            serde_json::from_slice(request.body.as_ref().expect("body")).expect("json");
        assert_eq!(body["aspect_ratio"], "16:9");
        assert_eq!(body["resolution"], "2k", "max edge > 1024");
        assert!(warnings.iter().any(|w| w.starts_with("xai_size_class:")));
        // provider_options.resolution overrides the derived class.
        let args = parsed(serde_json::json!({
            "size": "2048x1152", "provider_options": { "resolution": "1k" }
        }));
        let (request, _) = build_request(&args, &key()).expect("builds");
        let body: serde_json::Value =
            serde_json::from_slice(request.body.as_ref().expect("body")).expect("json");
        assert_eq!(body["resolution"], "1k");
    }

    #[test]
    fn quality_and_compression_warn_never_silently() {
        let args = parsed(serde_json::json!({
            "quality": "ultra", "format": "jpeg", "compression": 80
        }));
        let (_, warnings) = build_request(&args, &key()).expect("builds");
        assert!(
            warnings
                .iter()
                .any(|w| w.starts_with("quality_is_model_tier:")
                    && w.contains("grok-imagine-image-quality")),
            "{warnings:?}"
        );
        assert!(
            warnings
                .iter()
                .any(|w| w.starts_with("compression_unsupported:")),
            "{warnings:?}"
        );
    }

    #[tokio::test]
    async fn url_only_responses_are_refused_never_fetched() {
        let response = serde_json::json!({
            "data": [{ "url": "https://imgen.x.ai/xai-imgen/xai-tmp-imgen-abc.jpeg" }]
        });
        let http = MockHttp::new().enqueue_ok(200, response.to_string().into_bytes());
        let err = generate(&http, &key(), &parsed(serde_json::json!({})))
            .await
            .expect_err("refused");
        assert_eq!(err.code, C_NO_IMAGE);
        assert!(err.message.contains("never fetches"), "{}", err.message);
        assert_eq!(http.sent_requests().len(), 1, "no follow-up fetch happened");
    }

    #[tokio::test]
    async fn status_retryability_follows_the_table() {
        for (status, transient) in [(429u16, true), (500, true), (422, false), (403, false)] {
            let http = MockHttp::new().enqueue_ok(
                status,
                serde_json::json!({ "error": { "message": "x" } })
                    .to_string()
                    .into_bytes(),
            );
            let err = generate(&http, &key(), &parsed(serde_json::json!({})))
                .await
                .expect_err("fails");
            assert_eq!(err.code, C_REQUEST, "HTTP {status}");
            assert_eq!(err.transient, transient, "HTTP {status}");
        }
        // Envelope-less bodies parse best-effort, never panic.
        let http = MockHttp::new().enqueue_ok(400, b"plain text error".to_vec());
        let err = generate(&http, &key(), &parsed(serde_json::json!({})))
            .await
            .expect_err("fails");
        assert!(err.message.contains("xai HTTP 400"), "{}", err.message);
    }

    #[tokio::test]
    async fn the_key_never_reaches_messages_or_details() {
        let failures = [
            MockHttp::new().enqueue_ok(500, b"{}".to_vec()),
            MockHttp::new().enqueue_ok(200, b"{\"data\":[]}".to_vec()),
            MockHttp::new().enqueue_err(HttpError::Connection {
                reason: "refused".into(),
            }),
        ];
        for http in failures {
            let err = generate(&http, &key(), &parsed(serde_json::json!({})))
                .await
                .expect_err("fails");
            assert!(!err.message.contains("xai-test-KEY"), "{}", err.message);
            if let Some(details) = &err.details {
                assert!(!details.to_string().contains("xai-test-KEY"));
            }
        }
    }

    #[tokio::test]
    async fn debug_raw_is_sanitized_of_base64() {
        let (b64, _) = wire_png_b64();
        assert!(b64.len() > 256);
        let http = MockHttp::new().enqueue_ok(
            200,
            serde_json::json!({ "data": [{ "b64_json": b64.clone() }] })
                .to_string()
                .into_bytes(),
        );
        let batch = generate(&http, &key(), &parsed(serde_json::json!({ "debug": true })))
            .await
            .expect("generates");
        let text = batch.raw_debug.expect("debug raw").to_string();
        assert!(!text.contains(&b64), "base64 stripped from the debug echo");
    }
}
