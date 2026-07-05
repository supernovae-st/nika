// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika:image_generate` — the Media builtin (stdlib §Media · the first
//! of the deferred media class to graduate).
//!
//! One official pipeline: parse+validate args → provider adapter (openai ·
//! gemini · mock) over the IMAGE PLANE http seam → header-only decode
//! validation (magic bytes + dimensions · no pixel decode → no
//! decompression surface) → boundary-gated atomic saves → provenance
//! manifest → normalized JSON output (paths + hashes + usage — NEVER
//! base64; assets live on disk, workflows pass paths).
//!
//! # The image plane
//!
//! Provider calls ride a DEDICATED http seam injected via
//! [`crate::BuiltinDispatcher::with_image_plane`] — the composition root
//! passes its PROVIDER-plane client (SSRF disabled: endpoints here are
//! const studio-fixed strings, never workflow data — the same sanctioned
//! reasoning as `infer:` · and the 600s transport ceiling image renders
//! need), NOT the fetch client (whose 30s idle guard kills 60–120s image
//! waits — the F1 timeout class). `permits.net.http` does not govern the
//! provider plane (exactly like `infer:`); `permits.tools` and
//! `permits.fs.write` DO govern this builtin.
//!
//! # Keys
//!
//! [`ImageKeys`] is resolved at the composition root (the ONE sanctioned
//! env boundary) — this crate never reads the environment and never logs
//! a credential (the `Secret` type is zeroizing + Debug-redacted).

pub(crate) mod args;
pub(crate) mod gemini;
pub(crate) mod manifest;
pub(crate) mod mock;
pub(crate) mod openai;
pub(crate) mod save;
pub(crate) mod sniff;
pub(crate) mod types;

use nika_kernel::io::clock::ClockDyn;
use nika_kernel::io::fs::{FsReadDyn, FsWriteDyn};
use nika_kernel::io::http::HttpPostDyn;
use nika_kernel::secret::Secret;

use crate::permits::FsBoundary;
use crate::{Args, BuiltinFailure, BuiltinOutcome, Emitter};

use self::args::ImageArgs;
use self::save::SavedImage;
use self::types::{C_PROVIDER_UNAVAILABLE, Provider, ProviderBatch};

/// The image-provider credentials, resolved by the composition root (the
/// sanctioned env boundary — `NIKA_OPENAI_API_KEY` → `OPENAI_API_KEY` ·
/// `NIKA_GEMINI_API_KEY` → `GEMINI_API_KEY`) and injected via
/// [`crate::BuiltinDispatcher::with_image_plane`]. The mock provider
/// needs no key (and no http).
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct ImageKeys {
    /// The `OpenAI` API key, when present in the environment.
    pub openai: Option<Secret>,
    /// The Gemini API key, when present in the environment.
    pub gemini: Option<Secret>,
}

impl ImageKeys {
    /// No keys — the mock provider still works (offline CI · first run).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Attach the `OpenAI` key.
    #[must_use]
    pub fn with_openai(mut self, key: Secret) -> Self {
        self.openai = Some(key);
        self
    }

    /// Attach the Gemini key.
    #[must_use]
    pub fn with_gemini(mut self, key: Secret) -> Self {
        self.gemini = Some(key);
        self
    }
}

/// Run `nika:image_generate` end-to-end. Every failure surfaces one of
/// the spec codes (001–007 + the `NIKA-SEC-004` boundary), and an
/// `image_generation.error` event mirrors it on the observability seam.
pub(crate) async fn generate<F, H, C, Em>(
    fs: &F,
    http: Option<&H>,
    keys: &ImageKeys,
    clock: &C,
    emitter: &Em,
    boundary: &FsBoundary,
    raw_args: &Args,
) -> BuiltinOutcome
where
    F: FsReadDyn + FsWriteDyn,
    H: HttpPostDyn,
    C: ClockDyn,
    Em: Emitter,
{
    match run(fs, http, keys, clock, emitter, boundary, raw_args).await {
        Ok(value) => Ok(value),
        Err(failure) => {
            emitter.emit(
                "image_generation.error",
                serde_json::json!({ "code": failure.code, "message": failure.message }),
            );
            Err(failure)
        }
    }
}

/// The pipeline body (the wrapper above owns the error event).
#[allow(clippy::too_many_lines)] // a linear pipeline of small steps — split points are the helpers
async fn run<F, H, C, Em>(
    fs: &F,
    http: Option<&H>,
    keys: &ImageKeys,
    clock: &C,
    emitter: &Em,
    boundary: &FsBoundary,
    raw_args: &Args,
) -> BuiltinOutcome
where
    F: FsReadDyn + FsWriteDyn,
    H: HttpPostDyn,
    C: ClockDyn,
    Em: Emitter,
{
    let started = clock.now();
    let mut args = args::parse(raw_args)?;
    let mut warnings = std::mem::take(&mut args.warnings);
    emitter.emit(
        "image_generation.started",
        serde_json::json!({
            "provider": args.provider.id(), "model": args.model, "n": args.n,
        }),
    );

    // ── provider call ────────────────────────────────────────────────────
    let batch = call_provider(http, keys, &args, clock, emitter, started).await?;
    let ProviderBatch {
        images,
        usage,
        provider_text,
        warnings: batch_warnings,
        raw_debug,
    } = batch;
    warnings.extend(batch_warnings);

    // ── decode validation (header-only · magic authority) ───────────────
    let mut decoded = Vec::with_capacity(images.len());
    let mut format_mismatch_seen = false;
    for (index, image) in images.into_iter().enumerate() {
        let sniffed = sniff::sniff(&image.bytes)?;
        if sniffed.format != args.format && !format_mismatch_seen {
            format_mismatch_seen = true;
            warnings.push(format!(
                "format_mismatch: requested `{}` — the provider returned `{}`; \
                 filenames follow the ACTUAL format (magic bytes are the authority)",
                args.format.name(),
                sniffed.format.name()
            ));
        }
        emitter.emit(
            "image_generation.decoded",
            serde_json::json!({
                "index": index, "mime_type": sniffed.format.mime(),
                "width": sniffed.width, "height": sniffed.height,
                "size_bytes": image.bytes.len(),
            }),
        );
        decoded.push((image, sniffed, Vec::new()));
    }

    // ── save (boundary-gated · atomic · cleanup on partial failure) ─────
    let saved = save::save_all(fs, boundary, &args, decoded).await?;
    for image in &saved {
        emitter.emit(
            "image_generation.saved",
            serde_json::json!({
                "path": image.path, "sha256_8": &image.sha256[..8],
                "size_bytes": image.size_bytes,
            }),
        );
    }

    // ── manifest ─────────────────────────────────────────────────────────
    let created_at = rfc3339_now(clock);
    let revised_prompt = saved.iter().find_map(|s| s.revised_prompt.clone());
    let manifest_path = if args.manifest {
        let document = manifest::build(
            &args,
            &saved,
            usage,
            &warnings,
            &created_at,
            revised_prompt.as_deref(),
            provider_text.as_deref(),
        );
        let path = manifest::write(fs, boundary, &args, &saved, &document).await?;
        emitter.emit(
            "image_generation.manifest_written",
            serde_json::json!({ "path": path }),
        );
        Some(path)
    } else {
        None
    };

    for warning in &warnings {
        emitter.emit(
            "image_generation.warning",
            serde_json::json!({ "message": warning }),
        );
    }
    let duration_ms = u64::try_from(clock.elapsed(started).as_millis()).unwrap_or(u64::MAX);
    let total_bytes: u64 = saved.iter().map(|s| s.size_bytes).sum();
    emitter.emit(
        "image_generation.completed",
        serde_json::json!({
            "count": saved.len(), "total_bytes": total_bytes, "duration_ms": duration_ms,
        }),
    );

    Ok(output_json(
        &args,
        &saved,
        usage,
        &warnings,
        &created_at,
        revised_prompt.as_deref(),
        provider_text.as_deref(),
        manifest_path.as_deref(),
        raw_debug,
    ))
}

/// The cloud subset of [`Provider`] — bound by the SAME match that
/// resolves the credential, so the adapter dispatch below needs no
/// second look at `Provider::Mock` (and therefore no panic-class
/// `unreachable!` arm in production src — zero-panic discipline).
enum Cloud {
    Openai,
    Gemini,
}

/// Dispatch to the provider adapter — the wiring/credential gate.
async fn call_provider<H, C, Em>(
    http: Option<&H>,
    keys: &ImageKeys,
    args: &ImageArgs,
    clock: &C,
    emitter: &Em,
    started: std::time::Instant,
) -> Result<ProviderBatch, BuiltinFailure>
where
    H: HttpPostDyn,
    C: ClockDyn,
    Em: Emitter,
{
    let (endpoint_host, key, cloud) = match args.provider {
        Provider::Mock => return mock::generate(args),
        Provider::Openai => (
            "api.openai.com",
            keys.openai.as_ref().ok_or_else(|| {
                BuiltinFailure::new(
                    C_PROVIDER_UNAVAILABLE,
                    "no OpenAI API key — set NIKA_OPENAI_API_KEY or OPENAI_API_KEY \
                     in the engine's environment",
                )
            })?,
            Cloud::Openai,
        ),
        Provider::Gemini => (
            "generativelanguage.googleapis.com",
            keys.gemini.as_ref().ok_or_else(|| {
                BuiltinFailure::new(
                    C_PROVIDER_UNAVAILABLE,
                    "no Gemini API key — set NIKA_GEMINI_API_KEY or GEMINI_API_KEY \
                     in the engine's environment",
                )
            })?,
            Cloud::Gemini,
        ),
    };
    let Some(http) = http else {
        return Err(BuiltinFailure::new(
            C_PROVIDER_UNAVAILABLE,
            "the image plane is not wired in this engine composition — real image \
             providers need `with_image_plane` (the mock provider works without it)",
        ));
    };
    emitter.emit(
        "image_generation.provider_request",
        serde_json::json!({
            "provider": args.provider.id(), "model": args.model,
            "endpoint_host": endpoint_host, "n": args.n,
        }),
    );
    let batch = match cloud {
        Cloud::Openai => openai::generate(http, key, args).await?,
        Cloud::Gemini => gemini::generate(http, key, args).await?,
    };
    emitter.emit(
        "image_generation.provider_response",
        serde_json::json!({
            "provider": args.provider.id(),
            "images": batch.images.len(),
            "duration_ms": u64::try_from(clock.elapsed(started).as_millis())
                .unwrap_or(u64::MAX),
        }),
    );
    Ok(batch)
}

/// RFC 3339 wall-clock timestamp through the injected clock seam (test
/// hermeticity — invariant #27).
fn rfc3339_now<C: ClockDyn>(clock: &C) -> String {
    jiff::Timestamp::try_from(clock.system_now())
        .map_or_else(|_| "1970-01-01T00:00:00Z".to_owned(), |ts| ts.to_string())
}

/// Assemble the normalized output object — paths + hashes + usage, never
/// bytes. `raw_provider_response` appears ONLY under `debug: true` (and is
/// pre-sanitized by the adapters).
#[allow(clippy::too_many_arguments)] // a pure projection of the pipeline's products
fn output_json(
    args: &ImageArgs,
    saved: &[SavedImage],
    usage: types::Usage,
    warnings: &[String],
    created_at: &str,
    revised_prompt: Option<&str>,
    provider_text: Option<&str>,
    manifest_path: Option<&str>,
    raw_debug: Option<serde_json::Value>,
) -> serde_json::Value {
    let images: Vec<serde_json::Value> = saved
        .iter()
        .map(|image| {
            serde_json::json!({
                "index": image.index,
                "path": image.path,
                "filename": image.filename,
                "mime_type": image.mime_type,
                "format": image.format,
                "width": image.width,
                "height": image.height,
                "size_bytes": image.size_bytes,
                "sha256": image.sha256,
                "provider": args.provider.id(),
                "model": args.model,
                "seed": image.seed,
                "variant_id": image.variant_id,
                "warnings": image.warnings,
                "metadata": args.metadata,
            })
        })
        .collect();
    let mut output = serde_json::json!({
        "provider": args.provider.id(),
        "model": args.model,
        "mode": "generate",
        "prompt": args.prompt,
        "revised_prompt": revised_prompt,
        "provider_text": provider_text,
        "created_at": created_at,
        "count": saved.len(),
        "images": images,
        "usage": usage.to_json(),
        "warnings": warnings,
        "manifest_path": manifest_path,
        "output_dir": args.output_dir,
    });
    if let (Some(map), Some(raw)) = (output.as_object_mut(), raw_debug) {
        map.insert("raw_provider_response".to_owned(), raw);
    }
    output
}
