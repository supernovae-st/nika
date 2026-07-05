// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `OpenAI` Images adapter — `POST /v1/images/generations`.
//!
//! Wire facts (primary-source verified 2026-07-05, developers.openai.com):
//! the GPT image models ALWAYS return base64 (`data[].b64_json` · no
//! `response_format` param), have NO seed, cap prompts at 32k chars, and
//! `gpt-image-2` takes arbitrary `WxH` sizes (both edges /16 · ratio
//! within 1:3–3:1 · 655,360 ≤ px ≤ 8,294,400 · edge ≤ 3840) while the
//! older models take the three standard sizes. User-error responses carry
//! `error.type == "image_generation_user_error"` (never auto-retried);
//! `error.code == "moderation_blocked"` may add `moderation_details`.

use nika_kernel::io::http::{HttpError, HttpPostDyn, HttpRequest};
use nika_kernel::secret::Secret;

use crate::BuiltinFailure;

use super::args::ImageArgs;
use super::types::{
    C_ARGS, C_NO_IMAGE, C_POLICY, C_REQUEST, ProviderBatch, Quality, RawImage, SizeSpec, Usage,
    sanitize_raw,
};

/// The fixed endpoint — never workflow-controlled (the reason the image
/// plane may ride the SSRF-disabled provider client).
const ENDPOINT: &str = "https://api.openai.com/v1/images/generations";

/// The documented gpt-image-2 size window.
const PIXEL_BUDGET: std::ops::RangeInclusive<u64> = 655_360..=8_294_400;
const MAX_EDGE: u32 = 3_840;

/// Run one generation batch against `OpenAI` (native `n` — one request).
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
/// Returns request-level warnings (size remaps on non-gpt-image-2).
fn build_request(
    args: &ImageArgs,
    key: &Secret,
) -> Result<(HttpRequest, Vec<String>), BuiltinFailure> {
    let mut warnings = Vec::new();
    let mut body = serde_json::Map::new();
    body.insert("model".into(), args.model.clone().into());
    body.insert("prompt".into(), args.prompt.clone().into());
    body.insert("n".into(), args.n.into());
    if let Some(size) = resolve_size(args, &mut warnings)? {
        body.insert("size".into(), size.into());
    }
    match args.quality {
        Quality::Auto => {}
        Quality::Ultra => {
            warnings.push(
                "quality_folded: openai's ladder tops at `high` — `ultra` sent as `high`"
                    .to_owned(),
            );
            body.insert("quality".into(), "high".into());
        }
        other => {
            body.insert("quality".into(), other.name().into());
        }
    }
    body.insert("output_format".into(), args.format.name().into());
    if let Some(compression) = args.compression {
        // parse guarantees jpeg/webp here.
        body.insert("output_compression".into(), compression.into());
    }
    if args.background != super::types::Background::Auto {
        body.insert("background".into(), args.background.name().into());
    }
    for pass_through in ["moderation", "user"] {
        if let Some(value) = args.provider_options.get(pass_through) {
            body.insert(pass_through.into(), value.clone());
        }
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
    Ok((request, warnings))
}

/// Resolve the wire `size` string. `gpt-image-2` accepts arbitrary
/// /16-aligned sizes (validated against the documented window); the older
/// GPT image models take the three standard sizes (an off-grid request is
/// remapped by orientation, loudly).
fn resolve_size(
    args: &ImageArgs,
    warnings: &mut Vec<String>,
) -> Result<Option<String>, BuiltinFailure> {
    let flexible = args.model.starts_with("gpt-image-2");
    Ok(match args.size {
        SizeSpec::Auto => None,
        SizeSpec::Exact { width, height } => {
            if flexible {
                validate_flexible(width, height)?;
                Some(format!("{width}x{height}"))
            } else {
                Some(standard_size(width, height, args, warnings))
            }
        }
        SizeSpec::Aspect(ratio) => {
            if flexible {
                let (width, height) = ratio.openai_size();
                Some(format!("{width}x{height}"))
            } else {
                let (width, height) = ratio.openai_size();
                Some(standard_size(width, height, args, warnings))
            }
        }
    })
}

/// The documented gpt-image-2 window — checked LOCALLY so a bad size
/// fails with a precise 001 instead of a spent request + provider 400.
fn validate_flexible(width: u32, height: u32) -> Result<(), BuiltinFailure> {
    let fail = |detail: String| {
        Err(BuiltinFailure::new(
            C_ARGS,
            format!(
                "`size: {width}x{height}` is outside the gpt-image-2 window — {detail} \
                 (both edges /16 · ratio within 1:3–3:1 · 655,360..=8,294,400 px · \
                 edge ≤ 3840)"
            ),
        ))
    };
    if !width.is_multiple_of(16) || !height.is_multiple_of(16) {
        return fail("each edge must be divisible by 16".into());
    }
    if width > MAX_EDGE || height > MAX_EDGE {
        return fail(format!("max edge is {MAX_EDGE}px"));
    }
    let pixels = u64::from(width) * u64::from(height);
    if !PIXEL_BUDGET.contains(&pixels) {
        return fail(format!("{pixels} px is outside the pixel budget"));
    }
    let ratio = f64::from(width) / f64::from(height);
    if !(1.0 / 3.0..=3.0).contains(&ratio) {
        return fail("aspect ratio must stay within 1:3–3:1".into());
    }
    Ok(())
}

/// Fold a request onto the standard-size triple for the pre-gpt-image-2
/// models, by orientation — loud, never silent.
fn standard_size(width: u32, height: u32, args: &ImageArgs, warnings: &mut Vec<String>) -> String {
    let standard = match width.cmp(&height) {
        std::cmp::Ordering::Greater => "1536x1024",
        std::cmp::Ordering::Less => "1024x1536",
        std::cmp::Ordering::Equal => "1024x1024",
    };
    if format!("{width}x{height}") != standard {
        warnings.push(format!(
            "size_remapped: `{}` supports the standard sizes only — {width}x{height} \
             sent as {standard} (gpt-image-2 takes arbitrary /16 sizes)",
            args.model
        ));
    }
    standard.to_owned()
}

/// Map transport-plane failures (no HTTP response existed).
fn map_transport(error: HttpError) -> BuiltinFailure {
    match error {
        HttpError::Timeout { duration_ms } => BuiltinFailure::new(
            C_REQUEST,
            format!(
                "openai request timed out after {duration_ms}ms — image generation can \
                 run ~2min on complex prompts; raise `timeout_ms:` if needed"
            ),
        )
        .with_transient(true),
        HttpError::Connection { reason } => {
            BuiltinFailure::new(C_REQUEST, format!("openai connection failed: {reason}"))
                .with_transient(true)
        }
        HttpError::TooLarge { size, max } => BuiltinFailure::new(
            C_REQUEST,
            format!(
                "openai response was {size} bytes (cap {max}) — lower `n:` or switch \
                 `format:` to jpeg/webp with `compression:`"
            ),
        ),
        other => BuiltinFailure::new(C_REQUEST, format!("openai request failed: {other}")),
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
            format!("openai returned a non-JSON 2xx body ({e}) — malformed response"),
        )
    })?;
    let data = parsed
        .get("data")
        .and_then(serde_json::Value::as_array)
        .filter(|entries| !entries.is_empty())
        .ok_or_else(|| {
            BuiltinFailure::new(
                C_NO_IMAGE,
                "openai returned no images (`data` is empty) — nothing to save",
            )
        })?;

    let mut images = Vec::with_capacity(data.len());
    let mut warnings = Vec::new();
    for entry in data {
        let b64 = entry
            .get("b64_json")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                BuiltinFailure::new(
                    C_NO_IMAGE,
                    "openai image entry carries no `b64_json` payload (the GPT image \
                     models always return base64) — malformed response",
                )
            })?;
        let bytes = crate::data::base64_decode(b64).map_err(|e| {
            BuiltinFailure::new(C_NO_IMAGE, format!("openai base64 payload is corrupt: {e}"))
        })?;
        let revised_prompt = entry
            .get("revised_prompt")
            .and_then(serde_json::Value::as_str)
            .map(|s| {
                // Capped at the source: a revised prompt is a caption, not
                // a payload channel — outputs/manifest inherit the bound,
                // and the clamp is LOUD (the honest-degradation contract).
                let total = s.chars().count();
                if total > 2_000 && warnings.is_empty() {
                    warnings.push(format!(
                        "revised_prompt_clamped: the provider's revised prompt ran \
                         {total} chars — clamped to 2000"
                    ));
                }
                s.chars().take(2_000).collect::<String>()
            });
        images.push(RawImage {
            bytes,
            revised_prompt,
            seed: None, // no seed on the OpenAI Images API
        });
    }

    let read = |path: &str| {
        parsed
            .get("usage")
            .and_then(|u| u.get(path))
            .and_then(serde_json::Value::as_u64)
    };
    let usage = Usage {
        input_tokens: read("input_tokens"),
        output_tokens: read("output_tokens"),
        total_tokens: read("total_tokens"),
        thoughts_tokens: None,
    };

    let raw_debug = args.debug.then(|| {
        let mut raw = parsed.clone();
        sanitize_raw(&mut raw);
        raw
    });
    Ok(ProviderBatch {
        images,
        usage,
        endpoint_host: Some("api.openai.com".to_owned()),
        cost_usd: None, // token-priced · image models unpriced in the catalog (roadmap)
        provider_text: None,
        warnings,
        raw_debug,
    })
}

/// Map a non-2xx provider response to the spec error planes.
fn map_error_status(status: u16, body: &[u8]) -> BuiltinFailure {
    let envelope: serde_json::Value = serde_json::from_slice(body).unwrap_or_default();
    let error = envelope.get("error").cloned().unwrap_or_default();
    let field = |key: &str| {
        error
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    };
    let code = field("code");
    let kind = field("type");
    let message = field("message").unwrap_or_else(|| "no error message".to_owned());
    let message: String = message.chars().take(300).collect();

    // The user-error plane is NEVER retried (documented contract):
    // moderation blocks + every other image_generation_user_error.
    if code.as_deref() == Some("moderation_blocked")
        || kind.as_deref() == Some("image_generation_user_error")
    {
        let mut details = serde_json::json!({
            "status_code": status,
            "code": code,
            "type": kind,
        });
        if let (Some(map), Some(moderation)) =
            (details.as_object_mut(), error.get("moderation_details"))
        {
            map.insert("moderation_details".to_owned(), moderation.clone());
        }
        return BuiltinFailure::new(
            C_POLICY,
            format!("openai declined the request ({message}) — adjust the prompt"),
        )
        .with_details(details);
    }

    BuiltinFailure::new(C_REQUEST, format!("openai HTTP {status}: {message}"))
        .with_transient(matches!(status, 500..=599 | 408 | 429))
        .with_details(serde_json::json!({ "status_code": status, "code": code }))
}

#[cfg(test)]
mod tests {
    use nika_kernel_mock::MockHttp;

    use super::super::args;
    use super::*;

    fn parsed(patch: serde_json::Value) -> ImageArgs {
        let serde_json::Value::Object(mut map) = serde_json::json!({
            "provider": "openai", "prompt": "a monarch butterfly macro",
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
        Secret::new("sk-test-XYZ-do-not-leak".to_owned())
    }

    /// A tiny valid PNG payload (via the mock renderer) base64-encoded
    /// the way the wire carries it.
    fn wire_png_b64() -> (String, Vec<u8>) {
        let mock_args = parsed(serde_json::json!({ "provider": "mock", "size": "24x16" }));
        let batch = super::super::mock::generate(&mock_args).expect("mock png");
        let bytes = batch.images.into_iter().next().expect("one").bytes;
        (crate::data::base64_encode(&bytes), bytes)
    }

    #[tokio::test]
    async fn happy_path_builds_the_documented_request_and_decodes_b64() {
        let (b64, bytes) = wire_png_b64();
        let response = serde_json::json!({
            "created": 1_751_700_000u64,
            "data": [{ "b64_json": b64 }],
            "usage": {
                "input_tokens": 12, "output_tokens": 4160, "total_tokens": 4172,
                "input_tokens_details": { "text_tokens": 12, "image_tokens": 0 }
            }
        });
        let http = MockHttp::new().enqueue_ok(200, response.to_string().into_bytes());
        let args = parsed(serde_json::json!({
            "size": "1536x864", "quality": "high", "format": "webp", "compression": 85,
            "provider_options": { "moderation": "low", "user": "qrcodeai-batch" }
        }));
        let batch = generate(&http, &key(), &args).await.expect("generates");
        assert_eq!(batch.images.len(), 1);
        assert_eq!(
            batch.images[0].bytes, bytes,
            "b64 round-trips to the wire bytes"
        );
        assert_eq!(batch.usage.input_tokens, Some(12));
        assert_eq!(batch.usage.total_tokens, Some(4172));
        assert_eq!(batch.usage.thoughts_tokens, None);

        let sent = http.sent_requests();
        assert_eq!(sent.len(), 1);
        let request = &sent[0];
        assert_eq!(request.url, ENDPOINT);
        assert_eq!(
            request.headers.get("authorization").map(String::as_str),
            Some("Bearer sk-test-XYZ-do-not-leak")
        );
        assert_eq!(
            request.timeout,
            Some(std::time::Duration::from_millis(180_000))
        );
        let body: serde_json::Value =
            serde_json::from_slice(request.body.as_ref().expect("body")).expect("json");
        assert_eq!(body["model"], "gpt-image-2");
        assert_eq!(body["prompt"], "a monarch butterfly macro");
        assert_eq!(body["n"], 1);
        assert_eq!(body["size"], "1536x864");
        assert_eq!(body["quality"], "high");
        assert_eq!(body["output_format"], "webp");
        assert_eq!(body["output_compression"], 85);
        assert_eq!(body["moderation"], "low");
        assert_eq!(body["user"], "qrcodeai-batch");
        assert!(body.get("background").is_none(), "auto is omitted");
        assert!(body.get("seed").is_none(), "openai has no seed");
        assert!(
            body.get("response_format").is_none(),
            "b64 is implicit for gpt-image"
        );
    }

    #[tokio::test]
    async fn aspect_ratio_maps_to_the_exact_gpt_image_2_size() {
        let (b64, _) = wire_png_b64();
        let http = MockHttp::new().enqueue_ok(
            200,
            serde_json::json!({ "data": [{ "b64_json": b64 }] })
                .to_string()
                .into_bytes(),
        );
        let args = parsed(serde_json::json!({ "aspect_ratio": "21:9" }));
        generate(&http, &key(), &args).await.expect("generates");
        let body: serde_json::Value =
            serde_json::from_slice(http.sent_requests()[0].body.as_ref().expect("body"))
                .expect("json");
        assert_eq!(body["size"], "2016x864");
    }

    #[tokio::test]
    async fn quality_ultra_folds_to_high_with_a_warning() {
        let (b64, _) = wire_png_b64();
        let http = MockHttp::new().enqueue_ok(
            200,
            serde_json::json!({ "data": [{ "b64_json": b64 }] })
                .to_string()
                .into_bytes(),
        );
        let args = parsed(serde_json::json!({ "quality": "ultra" }));
        let batch = generate(&http, &key(), &args).await.expect("generates");
        assert!(
            batch
                .warnings
                .iter()
                .any(|w| w.starts_with("quality_folded:"))
        );
        let body: serde_json::Value =
            serde_json::from_slice(http.sent_requests()[0].body.as_ref().expect("body"))
                .expect("json");
        assert_eq!(body["quality"], "high");
    }

    #[test]
    fn gpt_image_2_size_window_is_validated_locally() {
        // not /16
        let args = parsed(serde_json::json!({ "size": "1000x1000" }));
        let err = build_request(&args, &key()).expect_err("off-grid");
        assert_eq!(err.code, C_ARGS);
        assert!(err.message.contains("divisible by 16"), "{}", err.message);
        // over the pixel budget (3840x3840 = 14.7M px > 8.29M)
        let args = parsed(serde_json::json!({ "size": "3840x3840" }));
        let err = build_request(&args, &key()).expect_err("budget");
        assert!(err.message.contains("pixel budget"), "{}", err.message);
        // under the floor (256x256)
        let args = parsed(serde_json::json!({ "size": "256x256" }));
        let err = build_request(&args, &key()).expect_err("floor");
        assert!(err.message.contains("pixel budget"), "{}", err.message);
        // ratio outside 1:3 (512x4096 → also edge>3840 → either message ok)
        let args = parsed(serde_json::json!({ "size": "512x2048" }));
        let err = build_request(&args, &key()).expect_err("ratio+floor");
        assert_eq!(err.code, C_ARGS);
        // a legal flexible size passes
        let args = parsed(serde_json::json!({ "size": "2048x1152" }));
        assert!(build_request(&args, &key()).is_ok());
    }

    #[test]
    fn older_models_fold_to_standard_sizes_loudly() {
        let args = parsed(serde_json::json!({
            "model": "gpt-image-1.5", "size": "1600x832"
        }));
        let (request, warnings) = build_request(&args, &key()).expect("builds");
        let body: serde_json::Value =
            serde_json::from_slice(request.body.as_ref().expect("body")).expect("json");
        assert_eq!(body["size"], "1536x1024", "landscape folds to the standard");
        assert!(warnings.iter().any(|w| w.starts_with("size_remapped:")));
        // aspect on an older model folds too (16:9 → landscape standard).
        let args = parsed(serde_json::json!({
            "model": "gpt-image-1.5", "aspect_ratio": "9:16"
        }));
        let (request, warnings) = build_request(&args, &key()).expect("builds");
        let body: serde_json::Value =
            serde_json::from_slice(request.body.as_ref().expect("body")).expect("json");
        assert_eq!(body["size"], "1024x1536");
        assert!(warnings.iter().any(|w| w.starts_with("size_remapped:")));
        // …but an exact standard size stays silent.
        let args = parsed(serde_json::json!({
            "model": "gpt-image-1.5", "size": "1024x1024"
        }));
        let (_, warnings) = build_request(&args, &key()).expect("builds");
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    #[tokio::test]
    async fn moderation_block_is_the_policy_plane_never_transient() {
        let body = serde_json::json!({
            "error": {
                "type": "image_generation_user_error",
                "code": "moderation_blocked",
                "message": "Your request was rejected by the safety system.",
                "moderation_details": { "moderation_stage": "input", "categories": ["violence"] }
            }
        });
        let http = MockHttp::new().enqueue_ok(400, body.to_string().into_bytes());
        let err = generate(&http, &key(), &parsed(serde_json::json!({})))
            .await
            .expect_err("blocked");
        assert_eq!(err.code, C_POLICY);
        assert!(!err.transient, "policy blocks are deterministic");
        let details = err.details.expect("details");
        assert_eq!(details["code"], "moderation_blocked");
        assert_eq!(details["moderation_details"]["moderation_stage"], "input");
    }

    #[tokio::test]
    async fn status_retryability_follows_the_table() {
        for (status, transient) in [(429u16, true), (500, true), (503, true), (400, false)] {
            let http = MockHttp::new().enqueue_ok(
                status,
                serde_json::json!({ "error": { "message": "x", "type": "server_error" } })
                    .to_string()
                    .into_bytes(),
            );
            let err = generate(&http, &key(), &parsed(serde_json::json!({})))
                .await
                .expect_err("fails");
            assert_eq!(err.code, C_REQUEST, "HTTP {status}");
            assert_eq!(err.transient, transient, "HTTP {status}");
            assert_eq!(
                err.details.expect("details")["status_code"],
                serde_json::json!(status)
            );
        }
    }

    #[tokio::test]
    async fn transport_failures_map_transient_and_too_large_hints() {
        let http = MockHttp::new().enqueue_err(HttpError::Timeout {
            duration_ms: 180_000,
        });
        let err = generate(&http, &key(), &parsed(serde_json::json!({})))
            .await
            .expect_err("timeout");
        assert!(err.transient);
        assert!(err.message.contains("timeout_ms"), "{}", err.message);
        let http = MockHttp::new().enqueue_err(HttpError::TooLarge {
            size: 70_000_000,
            max: 67_108_864,
        });
        let err = generate(&http, &key(), &parsed(serde_json::json!({})))
            .await
            .expect_err("too large");
        assert!(!err.transient);
        assert!(err.message.contains("jpeg/webp"), "{}", err.message);
    }

    #[tokio::test]
    async fn empty_data_and_corrupt_b64_are_the_no_image_plane() {
        let http = MockHttp::new().enqueue_ok(
            200,
            serde_json::json!({ "created": 1, "data": [] })
                .to_string()
                .into_bytes(),
        );
        let err = generate(&http, &key(), &parsed(serde_json::json!({})))
            .await
            .expect_err("empty");
        assert_eq!(err.code, C_NO_IMAGE);
        let http = MockHttp::new().enqueue_ok(
            200,
            serde_json::json!({ "data": [{ "b64_json": "not base64!!!" }] })
                .to_string()
                .into_bytes(),
        );
        let err = generate(&http, &key(), &parsed(serde_json::json!({})))
            .await
            .expect_err("corrupt");
        assert_eq!(err.code, C_NO_IMAGE);
        assert!(err.message.contains("base64"), "{}", err.message);
        let http = MockHttp::new().enqueue_ok(200, b"<html>oops</html>".to_vec());
        let err = generate(&http, &key(), &parsed(serde_json::json!({})))
            .await
            .expect_err("non-json");
        assert_eq!(err.code, C_NO_IMAGE);
    }

    #[tokio::test]
    async fn the_key_never_reaches_messages_or_details() {
        // Exercise every failure plane and assert the secret is absent.
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
            assert!(!err.message.contains("sk-test-XYZ"), "{}", err.message);
            if let Some(details) = &err.details {
                assert!(!details.to_string().contains("sk-test-XYZ"));
            }
        }
    }

    #[tokio::test]
    async fn debug_raw_is_sanitized_of_base64() {
        let (b64, _) = wire_png_b64();
        assert!(
            b64.len() > 256,
            "fixture must be long enough to trigger the strip"
        );
        let http = MockHttp::new().enqueue_ok(
            200,
            serde_json::json!({ "data": [{ "b64_json": b64.clone() }] })
                .to_string()
                .into_bytes(),
        );
        let args = parsed(serde_json::json!({ "debug": true }));
        let batch = generate(&http, &key(), &args).await.expect("generates");
        let raw = batch.raw_debug.expect("debug raw present");
        let text = raw.to_string();
        assert!(!text.contains(&b64), "base64 stripped from the debug echo");
        assert!(text.contains("base64 chars omitted"), "{text}");
        // default (no debug) carries no raw at all
        let http = MockHttp::new().enqueue_ok(
            200,
            serde_json::json!({ "data": [{ "b64_json": b64 }] })
                .to_string()
                .into_bytes(),
        );
        let batch = generate(&http, &key(), &parsed(serde_json::json!({})))
            .await
            .expect("generates");
        assert!(batch.raw_debug.is_none());
    }

    #[tokio::test]
    async fn revised_prompt_rides_through_when_present() {
        let (b64, _) = wire_png_b64();
        let http = MockHttp::new().enqueue_ok(
            200,
            serde_json::json!({
                "data": [{ "b64_json": b64, "revised_prompt": "a refined brief" }]
            })
            .to_string()
            .into_bytes(),
        );
        let batch = generate(&http, &key(), &parsed(serde_json::json!({})))
            .await
            .expect("generates");
        assert_eq!(
            batch.images[0].revised_prompt.as_deref(),
            Some("a refined brief")
        );
    }
}
