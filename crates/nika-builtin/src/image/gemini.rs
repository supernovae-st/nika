// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The Gemini image adapter — `POST …/models/{model}:generateContent`.
//!
//! Wire facts (primary-source verified 2026-07-05, ai.google.dev): auth is
//! the `x-goog-api-key` header; images arrive as `inlineData { mimeType,
//! data(base64) }` parts, possibly INTERLEAVED with text parts; sizes are
//! CLASS-based (`imageConfig { aspectRatio, imageSize: 512|1K|2K|4K }` —
//! uppercase K mandatory); `thinkingLevel` is documented for the
//! `gemini-3.1-flash-image` family only; safety outcomes ride
//! `finishReason` (`IMAGE_SAFETY` · `NO_IMAGE` · …) or
//! `promptFeedback.blockReason`. Multi-image variation has no verified
//! `candidateCount` path → `n:` runs n sequential calls (documented).

use nika_kernel::io::http::{HttpError, HttpPostDyn, HttpRequest};
use nika_kernel::secret::Secret;

use crate::BuiltinFailure;
use crate::media::wire;

use super::args::ImageArgs;
use super::types::{
    Background, C_ARGS, C_NO_IMAGE, C_POLICY, C_REQUEST, ProviderBatch, Quality, RawImage,
    SizeSpec, Usage, sanitize_raw,
};

/// The fixed endpoint STEM (the providers-crate idiom) — the model id is
/// appended per request, charset-confined at parse time.
const ENDPOINT_STEM: &str = "https://generativelanguage.googleapis.com/v1beta/models";

/// The `provider_text` ceiling — gemini's accompanying narration is a
/// caption, not a payload channel; anything bigger is clamped with a
/// stable warning (the same philosophy as the 300-char error-message
/// truncation below).
const MAX_PROVIDER_TEXT: usize = 2_000;

/// Finish reasons that mean « the safety layer declined » (the policy
/// plane · never retried).
const POLICY_FINISH: [&str; 8] = [
    "IMAGE_SAFETY",
    "IMAGE_PROHIBITED_CONTENT",
    "PROHIBITED_CONTENT",
    "SAFETY",
    "BLOCKLIST",
    "SPII",
    "RECITATION",
    "IMAGE_RECITATION",
];

/// Run one generation batch against Gemini — `n:` sequential calls (each
/// call's deadline is `timeout_ms:`, so the worst case is n × timeout).
pub(crate) async fn generate<H: HttpPostDyn>(
    http: &H,
    key: &Secret,
    args: &ImageArgs,
) -> Result<ProviderBatch, BuiltinFailure> {
    let (body, mut warnings) = build_body(args)?;
    let mut batch = ProviderBatch::default();
    let mut raw_calls = Vec::new();
    let mut texts = Vec::new();

    for call_index in 0..args.n {
        let request = wire_request(args, key, &body)?;
        let response = http.post(request).await.map_err(map_transport)?;
        let call = parse_response(response.status, &response.body)?;
        if call.images.len() > 1 {
            batch.warnings.push(format!(
                "gemini_extra_images: call {call_index} returned {} image parts for one \
                 request — all kept",
                call.images.len()
            ));
        }
        batch.images.extend(call.images);
        batch.usage.accumulate(call.usage);
        if let Some(text) = call.text {
            texts.push(text);
        }
        if args.debug {
            raw_calls.push(call.raw);
        }
    }

    batch.warnings.splice(0..0, warnings.drain(..));
    // cost_usd stays None (Default): gemini bills image output in tokens,
    // and the image model ids are unpriced in the catalog (roadmap).
    batch.endpoint_host = Some("generativelanguage.googleapis.com".to_owned());
    batch.provider_text = (!texts.is_empty()).then(|| texts.join("\n"));
    clamp_provider_text(&mut batch);
    batch.raw_debug = args.debug.then_some(serde_json::Value::Array(raw_calls));
    if batch.is_empty() {
        // Unreachable in practice (each call fails loudly on zero images)
        // — kept as the honest belt for a future refactor.
        return Err(BuiltinFailure::new(
            C_NO_IMAGE,
            "gemini returned no images across the batch",
        ));
    }
    Ok(batch)
}

/// « Assets, not blobs » applies to TEXT too: a multimodal response can
/// legally interleave megabytes of text (or base64-shaped junk) — the
/// output object rides into `ToolResult.content` and the agent's next
/// turn, so the accompanying narration is clamped, loudly (the full text
/// never belonged in workflow outputs).
fn clamp_provider_text(batch: &mut ProviderBatch) {
    use std::fmt::Write as _;
    let Some(text) = &mut batch.provider_text else {
        return;
    };
    let total = text.chars().count();
    if total <= MAX_PROVIDER_TEXT {
        return;
    }
    let mut clamped: String = text.chars().take(MAX_PROVIDER_TEXT).collect();
    let _ = write!(
        clamped,
        " … <{} more chars clamped>",
        total - MAX_PROVIDER_TEXT
    );
    *text = clamped;
    batch.warnings.push(format!(
        "provider_text_clamped: gemini returned {total} chars of accompanying \
         text — clamped to {MAX_PROVIDER_TEXT} (outputs carry assets by path, \
         never payloads)"
    ));
}

/// Build the (reused) request body + request-level warnings.
fn build_body(args: &ImageArgs) -> Result<(serde_json::Value, Vec<String>), BuiltinFailure> {
    let mut warnings = Vec::new();
    let mut generation = serde_json::Map::new();
    generation.insert(
        "responseModalities".into(),
        serde_json::json!(["TEXT", "IMAGE"]),
    );

    let mut image_config = serde_json::Map::new();
    match args.size {
        SizeSpec::Auto => {}
        SizeSpec::Aspect(ratio) => {
            image_config.insert("aspectRatio".into(), ratio.name().into());
        }
        SizeSpec::Exact { width, height } => {
            let divisor = gcd(width, height);
            let reduced = format!("{}:{}", width / divisor, height / divisor);
            let ratio = super::types::AspectRatio::ALL
                .into_iter()
                .find(|r| r.name() == reduced)
                .ok_or_else(|| {
                    BuiltinFailure::new(
                        C_ARGS,
                        format!(
                            "gemini renders size CLASSES, not exact pixels — \
                             `size: {width}x{height}` reduces to {reduced}, which is not \
                             a supported ratio; use `aspect_ratio:` (+ optional \
                             `provider_options.image_size: 1K|2K|4K`) or an exact size \
                             on openai"
                        ),
                    )
                })?;
            let class = match width.max(height) {
                0..=512 => "512",
                513..=1024 => "1K",
                1025..=2048 => "2K",
                _ => "4K",
            };
            warnings.push(format!(
                "gemini_size_class: gemini renders size classes — {width}x{height} sent \
                 as aspect {reduced} @ {class} (exact pixel sizes are an openai \
                 gpt-image-2 feature)"
            ));
            image_config.insert("aspectRatio".into(), ratio.name().into());
            image_config.insert("imageSize".into(), class.into());
        }
    }
    // An explicit provider option overrides the derived class.
    if let Some(size) = args
        .provider_options
        .get("image_size")
        .and_then(serde_json::Value::as_str)
    {
        image_config.insert("imageSize".into(), size.into());
    }
    if !image_config.is_empty() {
        generation.insert(
            "imageConfig".into(),
            serde_json::Value::Object(image_config),
        );
    }

    if let Some(seed) = args.seed {
        generation.insert("seed".into(), seed.into());
    }
    if let Some(level) = args
        .provider_options
        .get("thinking_level")
        .and_then(serde_json::Value::as_str)
    {
        // Documented for the 3.1 flash image family only — sending it to
        // other models risks a hard 400, so it is withheld with a warning.
        if args.model.starts_with("gemini-3.1-flash") {
            generation.insert(
                "thinkingConfig".into(),
                serde_json::json!({ "thinkingLevel": level }),
            );
        } else {
            warnings.push(format!(
                "thinking_level_ignored: `thinking_level` is documented for the \
                 gemini-3.1-flash-image family — not sent to `{}`",
                args.model
            ));
        }
    }
    push_capability_warnings(args, &mut warnings);

    let body = serde_json::json!({
        "contents": [{ "parts": [{ "text": args.prompt }] }],
        "generationConfig": serde_json::Value::Object(generation),
    });
    Ok((body, warnings))
}

/// The gemini capability gaps — every tolerated-but-lossy mapping lands
/// its stable `code: message` warning (the module's honest-degradation
/// contract; silence is non-conformant).
fn push_capability_warnings(args: &ImageArgs, warnings: &mut Vec<String>) {
    if args.quality != Quality::Auto {
        warnings.push(format!(
            "quality_is_model_tier: gemini has no quality knob — `{}` not sent; pick \
             the model tier instead (gemini-3-pro-image for peak quality)",
            args.quality.name()
        ));
    }
    if args.seed.is_some() && args.n > 1 {
        warnings.push(format!(
            "seed_repeated: the same seed rides all {} sequential gemini calls — \
             variants may come back identical (drop `seed:` for variety)",
            args.n
        ));
    }
    if args.compression.is_some() {
        warnings.push(
            "compression_unsupported: gemini has no compression knob — the value \
             is not sent"
                .to_owned(),
        );
    }
    if args.background == Background::Opaque {
        warnings.push(
            "background_ignored: gemini has no background knob — `opaque` not sent \
             (its output is opaque by default)"
                .to_owned(),
        );
    }
}

/// Assemble one wire request from the shared body.
fn wire_request(
    args: &ImageArgs,
    key: &Secret,
    body: &serde_json::Value,
) -> Result<HttpRequest, BuiltinFailure> {
    let mut request = HttpRequest::post(format!("{ENDPOINT_STEM}/{}:generateContent", args.model));
    request
        .headers
        .insert("x-goog-api-key".to_owned(), key.expose().to_owned());
    request
        .headers
        .insert("content-type".to_owned(), "application/json".to_owned());
    request.timeout = Some(args.timeout);
    request.body = Some(
        serde_json::to_vec(body)
            .map_err(|e| BuiltinFailure::new(C_REQUEST, format!("request serialization: {e}")))?
            .into(),
    );
    Ok(request)
}

fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a.max(1)
}

fn map_transport(error: HttpError) -> BuiltinFailure {
    match error {
        HttpError::Timeout { duration_ms } => BuiltinFailure::new(
            C_REQUEST,
            format!(
                "gemini request timed out after {duration_ms}ms — 4K/thinking renders \
                 can run minutes; raise `timeout_ms:` if needed"
            ),
        )
        .with_transient(true),
        HttpError::Connection { reason } => {
            BuiltinFailure::new(C_REQUEST, format!("gemini connection failed: {reason}"))
                .with_transient(true)
        }
        HttpError::TooLarge { size, max } => BuiltinFailure::new(
            C_REQUEST,
            format!(
                "gemini response was {size} bytes (cap {max}) — lower the \
                 `provider_options.image_size` class"
            ),
        ),
        other => BuiltinFailure::new(C_REQUEST, format!("gemini request failed: {other}")),
    }
}

/// One parsed call.
struct CallResult {
    images: Vec<RawImage>,
    usage: Usage,
    text: Option<String>,
    raw: serde_json::Value,
}

/// Parse one `generateContent` response (pure — fixture-testable).
fn parse_response(status: u16, body: &[u8]) -> Result<CallResult, BuiltinFailure> {
    if !(200..300).contains(&status) {
        return Err(map_error_status(status, body));
    }
    let parsed: serde_json::Value = serde_json::from_slice(body).map_err(|e| {
        BuiltinFailure::new(
            C_NO_IMAGE,
            format!("gemini returned a non-JSON 2xx body ({e}) — malformed response"),
        )
    })?;

    // A prompt-level block yields no candidates at all.
    if let Some(reason) = parsed
        .get("promptFeedback")
        .and_then(|f| f.get("blockReason"))
        .and_then(serde_json::Value::as_str)
    {
        return Err(BuiltinFailure::new(
            C_POLICY,
            format!("gemini blocked the prompt ({reason}) — adjust the prompt"),
        )
        .with_details(serde_json::json!({ "block_reason": reason })));
    }
    let Some(candidate) = parsed
        .get("candidates")
        .and_then(serde_json::Value::as_array)
        .and_then(|c| c.first())
    else {
        return Err(BuiltinFailure::new(
            C_NO_IMAGE,
            "gemini returned no candidates — malformed or empty response",
        ));
    };

    let mut images = Vec::new();
    let mut texts: Vec<String> = Vec::new();
    let parts: &[serde_json::Value] = candidate
        .get("content")
        .and_then(|c| c.get("parts"))
        .and_then(serde_json::Value::as_array)
        .map_or(&[], Vec::as_slice);
    for part in parts {
        // Interim thought parts (thinking models) are reasoning artifacts,
        // not deliverables — never saved as assets.
        if part.get("thought").and_then(serde_json::Value::as_bool) == Some(true) {
            continue;
        }
        // The wire is camelCase; some official examples show snake_case —
        // accept both spellings liberally.
        let inline = part.get("inlineData").or_else(|| part.get("inline_data"));
        if let Some(data) = inline
            .and_then(|i| i.get("data"))
            .and_then(serde_json::Value::as_str)
        {
            let bytes = crate::data::base64_decode(data).map_err(|e| {
                BuiltinFailure::new(C_NO_IMAGE, format!("gemini base64 payload is corrupt: {e}"))
            })?;
            images.push(RawImage {
                bytes,
                revised_prompt: None, // no gemini equivalent
                seed: None,           // echoed at the batch level, not per part
            });
        } else if let Some(text) = part.get("text").and_then(serde_json::Value::as_str) {
            texts.push(text.to_owned());
        }
    }

    if images.is_empty() {
        return Err(no_image_failure(candidate));
    }

    let meta = parsed.get("usageMetadata");
    let read = |key: &str| {
        meta.and_then(|m| m.get(key))
            .and_then(serde_json::Value::as_u64)
    };
    let usage = Usage {
        input_tokens: read("promptTokenCount"),
        output_tokens: read("candidatesTokenCount"),
        total_tokens: read("totalTokenCount"),
        thoughts_tokens: read("thoughtsTokenCount"),
    };
    let mut raw = parsed;
    sanitize_raw(&mut raw);
    Ok(CallResult {
        images,
        usage,
        text: (!texts.is_empty()).then(|| texts.join("\n")),
        raw,
    })
}

/// Map an image-less candidate to the right plane — safety finish reasons
/// are the policy plane (never retried), everything else is `-004`.
fn no_image_failure(candidate: &serde_json::Value) -> BuiltinFailure {
    let finish = candidate
        .get("finishReason")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("UNKNOWN");
    let details = serde_json::json!({ "finish_reason": finish });
    if POLICY_FINISH.contains(&finish) {
        BuiltinFailure::new(
            C_POLICY,
            format!("gemini declined the generation ({finish}) — adjust the prompt"),
        )
        .with_details(details)
    } else {
        BuiltinFailure::new(
            C_NO_IMAGE,
            format!("gemini returned no image parts (finishReason: {finish}) — nothing to save"),
        )
        .with_details(details)
    }
}

/// Map a non-2xx envelope (`{error: {code, message, status}}` — live-probed
/// shape) to the spec error planes.
fn map_error_status(status: u16, body: &[u8]) -> BuiltinFailure {
    let envelope: serde_json::Value = serde_json::from_slice(body).unwrap_or_default();
    let error = envelope.get("error").cloned().unwrap_or_default();
    let message: String = error
        .get("message")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("no error message")
        .chars()
        .take(300)
        .collect();
    let grpc_status = error
        .get("status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    BuiltinFailure::new(C_REQUEST, format!("gemini HTTP {status}: {message}"))
        .with_transient(wire::transient_status(status))
        .with_details(serde_json::json!({ "status_code": status, "status": grpc_status }))
}

#[cfg(test)]
mod tests {
    use nika_kernel_mock::MockHttp;

    use super::super::args;
    use super::*;

    fn parsed(patch: serde_json::Value) -> ImageArgs {
        let serde_json::Value::Object(mut map) = serde_json::json!({
            "provider": "gemini", "prompt": "an aurora over a glacier",
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
        Secret::new("g-test-KEY-do-not-leak".to_owned())
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

    fn ok_response(b64: &str) -> Vec<u8> {
        serde_json::json!({
            "candidates": [{
                "content": { "parts": [
                    { "text": "Here is your aurora." },
                    { "inlineData": { "mimeType": "image/png", "data": b64 } }
                ], "role": "model" },
                "finishReason": "STOP", "index": 0
            }],
            "usageMetadata": {
                "promptTokenCount": 21, "candidatesTokenCount": 1145,
                "thoughtsTokenCount": 312, "totalTokenCount": 1478
            },
            "modelVersion": "gemini-3.1-flash-image"
        })
        .to_string()
        .into_bytes()
    }

    #[tokio::test]
    async fn happy_path_builds_the_documented_request_and_reads_inline_data() {
        let (b64, bytes) = wire_png_b64();
        let http = MockHttp::new().enqueue_ok(200, ok_response(&b64));
        let batch = generate(
            &http,
            &key(),
            &parsed(serde_json::json!({
                "aspect_ratio": "16:9", "seed": 42,
                "provider_options": { "thinking_level": "high", "image_size": "2K" }
            })),
        )
        .await
        .expect("generates");
        assert_eq!(batch.images.len(), 1);
        assert_eq!(batch.images[0].bytes, bytes);
        assert_eq!(batch.provider_text.as_deref(), Some("Here is your aurora."));
        assert_eq!(batch.usage.input_tokens, Some(21));
        assert_eq!(batch.usage.thoughts_tokens, Some(312));

        let sent = http.sent_requests();
        assert_eq!(sent.len(), 1);
        let request = &sent[0];
        assert_eq!(
            request.url,
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-3.1-flash-image:generateContent"
        );
        assert_eq!(
            request.headers.get("x-goog-api-key").map(String::as_str),
            Some("g-test-KEY-do-not-leak")
        );
        assert!(
            !request.headers.contains_key("authorization"),
            "goog auth, not Bearer"
        );
        let body: serde_json::Value =
            serde_json::from_slice(request.body.as_ref().expect("body")).expect("json");
        assert_eq!(
            body["contents"][0]["parts"][0]["text"],
            "an aurora over a glacier"
        );
        let config = &body["generationConfig"];
        assert_eq!(
            config["responseModalities"],
            serde_json::json!(["TEXT", "IMAGE"])
        );
        assert_eq!(config["imageConfig"]["aspectRatio"], "16:9");
        assert_eq!(
            config["imageConfig"]["imageSize"], "2K",
            "explicit option wins"
        );
        assert_eq!(config["seed"], 42);
        assert_eq!(config["thinkingConfig"]["thinkingLevel"], "high");
    }

    #[tokio::test]
    async fn n_runs_sequential_calls_and_accumulates_usage() {
        let (b64, _) = wire_png_b64();
        let http = MockHttp::new()
            .enqueue_ok(200, ok_response(&b64))
            .enqueue_ok(200, ok_response(&b64));
        let batch = generate(&http, &key(), &parsed(serde_json::json!({ "n": 2 })))
            .await
            .expect("generates");
        assert_eq!(batch.images.len(), 2);
        assert_eq!(http.sent_requests().len(), 2, "n=2 → two wire calls");
        assert_eq!(batch.usage.total_tokens, Some(2 * 1478));
        assert_eq!(batch.usage.thoughts_tokens, Some(2 * 312));
    }

    #[test]
    fn exact_size_folds_to_a_class_or_fails_clean() {
        // 1536x864 reduces to 16:9 → class 2K + a loud warning.
        let (body, warnings) =
            build_body(&parsed(serde_json::json!({ "size": "1536x864" }))).expect("builds");
        assert_eq!(
            body["generationConfig"]["imageConfig"]["aspectRatio"],
            "16:9"
        );
        assert_eq!(body["generationConfig"]["imageConfig"]["imageSize"], "2K");
        assert!(warnings.iter().any(|w| w.starts_with("gemini_size_class:")));
        // 640x512 → 5:4 is not in the closed common set → precise 001.
        let err = build_body(&parsed(serde_json::json!({ "size": "640x512" })))
            .expect_err("unsupported ratio");
        assert_eq!(err.code, C_ARGS);
        assert!(err.message.contains("aspect_ratio"), "{}", err.message);
        // Small square folds to the 512 class.
        let (body, _) =
            build_body(&parsed(serde_json::json!({ "size": "512x512" }))).expect("builds");
        assert_eq!(body["generationConfig"]["imageConfig"]["imageSize"], "512");
    }

    #[test]
    fn thinking_level_is_family_gated_and_quality_warns() {
        // On the 3.1 flash family: sent.
        let (body, _) = build_body(&parsed(serde_json::json!({
            "provider_options": { "thinking_level": "minimal" }
        })))
        .expect("builds");
        assert_eq!(
            body["generationConfig"]["thinkingConfig"]["thinkingLevel"],
            "minimal"
        );
        // On pro: withheld + warned (undocumented → a 400 risk).
        let (body, warnings) = build_body(&parsed(serde_json::json!({
            "model": "gemini-3-pro-image",
            "provider_options": { "thinking_level": "high" }
        })))
        .expect("builds");
        assert!(body["generationConfig"].get("thinkingConfig").is_none());
        assert!(
            warnings
                .iter()
                .any(|w| w.starts_with("thinking_level_ignored:"))
        );
        // quality is a model-tier concept on gemini.
        let (body, warnings) =
            build_body(&parsed(serde_json::json!({ "quality": "high" }))).expect("builds");
        assert!(body["generationConfig"].get("quality").is_none());
        assert!(
            warnings
                .iter()
                .any(|w| w.starts_with("quality_is_model_tier:"))
        );
    }

    #[tokio::test]
    async fn thought_parts_are_never_saved_and_snake_case_is_accepted() {
        let (b64, bytes) = wire_png_b64();
        let body = serde_json::json!({
            "candidates": [{
                "content": { "parts": [
                    { "text": "thinking…", "thought": true },
                    { "inline_data": { "mime_type": "image/png", "data": b64 }, "thought": true },
                    { "inline_data": { "mime_type": "image/png", "data": b64 } }
                ]},
                "finishReason": "STOP"
            }]
        });
        let http = MockHttp::new().enqueue_ok(200, body.to_string().into_bytes());
        let batch = generate(&http, &key(), &parsed(serde_json::json!({})))
            .await
            .expect("generates");
        assert_eq!(batch.images.len(), 1, "interim thought image skipped");
        assert_eq!(
            batch.images[0].bytes, bytes,
            "snake_case inline_data accepted"
        );
        assert!(batch.provider_text.is_none(), "thought text skipped");
    }

    #[tokio::test]
    async fn safety_finishes_are_policy_and_no_image_is_004() {
        for (finish, expect_code) in [
            ("IMAGE_SAFETY", C_POLICY),
            ("PROHIBITED_CONTENT", C_POLICY),
            ("NO_IMAGE", C_NO_IMAGE),
            ("MAX_TOKENS", C_NO_IMAGE),
        ] {
            let body = serde_json::json!({
                "candidates": [{ "content": { "parts": [] }, "finishReason": finish }]
            });
            let http = MockHttp::new().enqueue_ok(200, body.to_string().into_bytes());
            let err = generate(&http, &key(), &parsed(serde_json::json!({})))
                .await
                .expect_err("no image");
            assert_eq!(err.code, expect_code, "{finish}");
            assert!(!err.transient);
            assert_eq!(
                err.details.expect("details")["finish_reason"],
                serde_json::json!(finish)
            );
        }
        // Prompt-level block: no candidates + promptFeedback.blockReason.
        let body = serde_json::json!({
            "promptFeedback": { "blockReason": "IMAGE_SAFETY" }
        });
        let http = MockHttp::new().enqueue_ok(200, body.to_string().into_bytes());
        let err = generate(&http, &key(), &parsed(serde_json::json!({})))
            .await
            .expect_err("blocked");
        assert_eq!(err.code, C_POLICY);
        // Empty everything → 004, not a panic.
        let http = MockHttp::new().enqueue_ok(200, b"{}".to_vec());
        let err = generate(&http, &key(), &parsed(serde_json::json!({})))
            .await
            .expect_err("empty");
        assert_eq!(err.code, C_NO_IMAGE);
    }

    #[tokio::test]
    async fn error_envelope_and_retryability_follow_the_table() {
        let envelope = serde_json::json!({
            "error": { "code": 429, "message": "Resource has been exhausted",
                        "status": "RESOURCE_EXHAUSTED" }
        });
        let http = MockHttp::new().enqueue_ok(429, envelope.to_string().into_bytes());
        let err = generate(&http, &key(), &parsed(serde_json::json!({})))
            .await
            .expect_err("rate limited");
        assert_eq!(err.code, C_REQUEST);
        assert!(err.transient, "429 is retryable");
        let details = err.details.expect("details");
        assert_eq!(details["status"], "RESOURCE_EXHAUSTED");
        let http = MockHttp::new().enqueue_ok(400, b"{\"error\":{\"message\":\"bad\"}}".to_vec());
        let err = generate(&http, &key(), &parsed(serde_json::json!({})))
            .await
            .expect_err("bad request");
        assert!(!err.transient, "400 is deterministic");
    }

    #[tokio::test]
    async fn the_key_never_reaches_messages_or_details() {
        let failures = [
            MockHttp::new().enqueue_ok(500, b"{}".to_vec()),
            MockHttp::new().enqueue_ok(200, b"{}".to_vec()),
            MockHttp::new().enqueue_err(HttpError::Connection {
                reason: "refused".into(),
            }),
        ];
        for http in failures {
            let err = generate(&http, &key(), &parsed(serde_json::json!({})))
                .await
                .expect_err("fails");
            assert!(!err.message.contains("g-test-KEY"), "{}", err.message);
            if let Some(details) = &err.details {
                assert!(!details.to_string().contains("g-test-KEY"));
            }
        }
    }

    #[tokio::test]
    async fn debug_raw_is_per_call_and_base64_stripped() {
        let (b64, _) = wire_png_b64();
        assert!(b64.len() > 256);
        let http = MockHttp::new()
            .enqueue_ok(200, ok_response(&b64))
            .enqueue_ok(200, ok_response(&b64));
        let batch = generate(
            &http,
            &key(),
            &parsed(serde_json::json!({ "n": 2, "debug": true })),
        )
        .await
        .expect("generates");
        let raw = batch.raw_debug.expect("debug raw");
        let calls = raw.as_array().expect("array of calls");
        assert_eq!(calls.len(), 2);
        let text = raw.to_string();
        assert!(!text.contains(&b64), "base64 stripped");
        assert!(text.contains("base64 chars omitted"));
    }

    #[tokio::test]
    async fn oversized_text_parts_are_clamped_everywhere() {
        // The refuted claim-3 regression (review swarm 2026-07-05): a
        // multimodal response can interleave megabytes of (base64-shaped)
        // TEXT — it must never ride outputs or the debug echo unbounded.
        let (b64, _) = wire_png_b64();
        let huge = "A".repeat(100_000);
        let body = serde_json::json!({
            "candidates": [{
                "content": { "parts": [
                    { "text": huge.clone() },
                    { "inlineData": { "mimeType": "image/png", "data": b64 } }
                ]},
                "finishReason": "STOP"
            }]
        });
        let http = MockHttp::new().enqueue_ok(200, body.to_string().into_bytes());
        let batch = generate(&http, &key(), &parsed(serde_json::json!({ "debug": true })))
            .await
            .expect("generates");
        let text = batch.provider_text.expect("text present");
        assert!(
            text.chars().count() < MAX_PROVIDER_TEXT + 64,
            "clamped ({} chars)",
            text.chars().count()
        );
        assert!(text.contains("more chars clamped"), "{}", &text[..80]);
        assert!(
            batch
                .warnings
                .iter()
                .any(|w| w.starts_with("provider_text_clamped:")),
            "{:?}",
            batch.warnings
        );
        let raw = batch.raw_debug.expect("debug raw").to_string();
        assert!(
            !raw.contains(&huge),
            "the huge text never survives sanitize_raw"
        );
    }

    #[test]
    fn lossy_gemini_mappings_warn_never_silently() {
        // compression (jpeg/webp) · background: opaque · seed × n>1 — each
        // tolerated-but-lossy mapping lands its stable warning.
        let (_, warnings) = build_body(&parsed(serde_json::json!({
            "format": "webp", "compression": 80,
            "background": "opaque", "seed": 7, "n": 3
        })))
        .expect("builds");
        for prefix in [
            "compression_unsupported:",
            "background_ignored:",
            "seed_repeated:",
        ] {
            assert!(
                warnings.iter().any(|w| w.starts_with(prefix)),
                "missing {prefix} in {warnings:?}"
            );
        }
        // …and none of them fire on a clean request.
        let (_, warnings) = build_body(&parsed(serde_json::json!({}))).expect("builds");
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    #[test]
    fn gcd_reduces_the_canonical_pairs() {
        assert_eq!(gcd(1536, 864), 96);
        assert_eq!(gcd(1024, 1024), 1024);
        assert_eq!(gcd(7, 0), 7, "zero-safe");
        assert_eq!(gcd(0, 0), 1, "degenerate pair never divides by zero");
    }
}
