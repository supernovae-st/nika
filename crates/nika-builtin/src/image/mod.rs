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
pub(crate) mod credentials;
pub(crate) mod embed;
pub(crate) mod gemini;
pub(crate) mod local;
pub(crate) mod manifest;
pub(crate) mod mock;
pub(crate) mod openai;
pub(crate) mod save;
pub(crate) mod sniff;
pub(crate) mod types;
pub(crate) mod xai;

use nika_kernel::io::clock::ClockDyn;
use nika_kernel::io::fs::{FsReadDyn, FsWriteDyn};
use nika_kernel::io::http::HttpPostDyn;
use nika_kernel::secret::Secret;

use crate::media::time::rfc3339_now;
use crate::permits::{FsAccess, FsBoundary};
use crate::{Args, BuiltinFailure, BuiltinOutcome, Emitter};

use self::args::ImageArgs;
use self::save::SavedImage;
use self::types::{C_PROVIDER_UNAVAILABLE, Provider, ProviderBatch};

/// The image-plane connection config, resolved by the composition root
/// (the sanctioned env boundary — `NIKA_OPENAI_API_KEY` → `OPENAI_API_KEY`
/// · `NIKA_GEMINI_API_KEY` → `GEMINI_API_KEY` · `NIKA_XAI_API_KEY` →
/// `XAI_API_KEY` · `NIKA_IMAGE_LOCAL_URL` + optional
/// `NIKA_IMAGE_LOCAL_API_KEY`) and injected via
/// [`crate::BuiltinDispatcher::with_image_plane`]. The `local` base URL is
/// engine CONFIG resolved at the same boundary — never workflow data,
/// which is exactly why the SSRF-disabled provider client may carry it.
/// The mock provider needs none of this (no key, no http).
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct ImageKeys {
    /// The `OpenAI` API key, when present in the environment.
    pub openai: Option<Secret>,
    /// The Gemini API key, when present in the environment.
    pub gemini: Option<Secret>,
    /// The xAI API key, when present in the environment.
    pub xai: Option<Secret>,
    /// The LOCAL image server base URL (`NIKA_IMAGE_LOCAL_URL`) — defaults
    /// to `LocalAI`'s `http://localhost:8080` when unset.
    pub local_base_url: Option<String>,
    /// The optional local-server key (`NIKA_IMAGE_LOCAL_API_KEY`) — most
    /// self-hosted servers run keyless; `LocalAI`'s `--api-keys` mode wants
    /// a Bearer.
    pub local_api_key: Option<Secret>,
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

    /// Attach the xAI key.
    #[must_use]
    pub fn with_xai(mut self, key: Secret) -> Self {
        self.xai = Some(key);
        self
    }

    /// Point the `local` provider at a specific compat server (`LocalAI` ·
    /// Ollama · sd-server · `SGLang` · `vLLM-Omni`).
    #[must_use]
    pub fn with_local_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.local_base_url = Some(base_url.into());
        self
    }

    /// Attach the optional local-server key.
    #[must_use]
    pub fn with_local_api_key(mut self, key: Secret) -> Self {
        self.local_api_key = Some(key);
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
            "provider": args.provider.id(), "model": args.model,
            "mode": args.mode.name(), "n": args.n,
        }),
    );

    // ── edit inputs (permit-gated reads · the mirror of the save boundary) ─
    let inputs = read_edit_inputs(fs, boundary, &args, emitter).await?;

    // ── provider call ────────────────────────────────────────────────────
    let batch = call_provider(http, keys, &args, &inputs, clock, emitter, started).await?;
    let ProviderBatch {
        images,
        usage,
        cost_usd,
        endpoint_host,
        provider_text,
        warnings: batch_warnings,
        raw_debug,
    } = batch;
    warnings.extend(batch_warnings);
    push_count_shortfall(&args, images.len(), &mut warnings);

    // ── decode validation (header-only · magic authority) ───────────────
    let mut decoded = validate_decoded(images, &args, emitter, &mut warnings)?;

    let content_credentials = detect_and_preserve_credentials(&mut decoded, &args, emitter);

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
    let manifest_path = write_manifest(
        fs,
        boundary,
        &args,
        &inputs,
        &saved,
        usage,
        cost_usd,
        content_credentials,
        &warnings,
        &created_at,
        revised_prompt.as_deref(),
        provider_text.as_deref(),
        endpoint_host.as_deref(),
        emitter,
    )
    .await?;

    emit_closing_events(emitter, clock, started, &saved, cost_usd, &warnings);

    Ok(output_json(
        &args,
        &saved,
        usage,
        cost_usd,
        content_credentials,
        &warnings,
        &created_at,
        revised_prompt.as_deref(),
        provider_text.as_deref(),
        endpoint_host.as_deref(),
        manifest_path.as_deref(),
        raw_debug,
    ))
}

/// The batch's closing telemetry — every accumulated warning as its own
/// event, then the ONE `completed` summary (count · bytes · real spend ·
/// wall duration).
fn emit_closing_events<C: ClockDyn, Em: Emitter>(
    emitter: &Em,
    clock: &C,
    started: std::time::Instant,
    saved: &[save::SavedImage],
    cost_usd: Option<f64>,
    warnings: &[String],
) {
    for warning in warnings {
        emitter.emit(
            "image_generation.warning",
            serde_json::json!({ "message": warning }),
        );
    }
    emitter.emit(
        "image_generation.completed",
        serde_json::json!({
            "count": saved.len(),
            "total_bytes": saved.iter().map(|s| s.size_bytes).sum::<u64>(),
            "cost_usd": cost_usd,
            "duration_ms":
                u64::try_from(clock.elapsed(started).as_millis()).unwrap_or(u64::MAX),
        }),
    );
}

/// Build + write the provenance manifest beside the assets (when
/// `manifest:` is on) and land its event — `None` when disabled.
#[allow(clippy::too_many_arguments)] // a pure projection of the pipeline's products
async fn write_manifest<F, Em>(
    fs: &F,
    boundary: &FsBoundary,
    args: &ImageArgs,
    inputs: &[types::InputImage],
    saved: &[SavedImage],
    usage: types::Usage,
    cost_usd: Option<f64>,
    content_credentials: Option<&str>,
    warnings: &[String],
    created_at: &str,
    revised_prompt: Option<&str>,
    provider_text: Option<&str>,
    endpoint_host: Option<&str>,
    emitter: &Em,
) -> Result<Option<String>, BuiltinFailure>
where
    F: FsReadDyn + FsWriteDyn,
    Em: Emitter,
{
    if !args.manifest {
        return Ok(None);
    }
    let document = manifest::build(
        args,
        inputs,
        saved,
        usage,
        cost_usd,
        content_credentials,
        warnings,
        created_at,
        revised_prompt,
        provider_text,
        endpoint_host,
    );
    let path = manifest::write(fs, boundary, args, saved, &document).await?;
    emitter.emit(
        "image_generation.manifest_written",
        serde_json::json!({ "path": path }),
    );
    Ok(Some(path))
}

/// In-file provenance (PNG tEXt · deterministic core) — the manifest is
/// the sidecar; the `nika` chunk is what SURVIVES a `cp` (the
/// `ComfyUI`/`InvokeAI` interchange practice — no workflow engine does
/// it). Embedded BEFORE hashing/saving so the filename sha and manifest
/// sha cover the byte that lands on disk.
/// Detect upstream content credentials, then embed the `nika` tEXt chunk
/// ONLY into unsigned PNG payloads — C2PA hard bindings hash the file's
/// byte ranges, so inserting our chunk into a signed render would
/// INVALIDATE the generator's own credentials (detect-and-PRESERVE ·
/// their signed manifest outranks our informal chunk). Returns the batch
/// label for output/manifest (`"c2pa"` when ANY image carried a signal).
fn detect_and_preserve_credentials<Em: Emitter>(
    decoded: &mut [(types::RawImage, sniff::Sniffed, Vec<String>)],
    args: &ImageArgs,
    emitter: &Em,
) -> Option<&'static str> {
    let mut batch_signal = None;
    for (image, sniffed, image_warnings) in decoded {
        match credentials::detect(&image.bytes) {
            Some(signal) => {
                batch_signal = Some(signal.label());
                image_warnings.push(format!(
                    "content_credentials_preserved: upstream {} manifest detected — the `nika` tEXt chunk was NOT embedded (it would invalidate the signature)",
                    signal.label()
                ));
                emitter.emit(
                    "image_generation.credentials_detected",
                    serde_json::json!({ "standard": signal.label() }),
                );
            }
            None if sniffed.format == types::ImageFormat::Png => {
                let seed = image.seed;
                image.bytes = embed::embed_provenance(std::mem::take(&mut image.bytes), args, seed);
            }
            None => {}
        }
    }
    batch_signal
}

/// Route one EDIT call — openai carries the first wire (M-A.2); the
/// other providers refuse loudly until their adapters land (M-B/M-C).
/// The ONE provider match means no panic-class `unreachable!` arm exists
/// (the zero-panic pattern · mock returns before any wire is built).
async fn dispatch_edit<H: HttpPostDyn>(
    http: &H,
    wire: Wire<'_>,
    args: &ImageArgs,
    inputs: &[types::InputImage],
) -> Result<ProviderBatch, BuiltinFailure> {
    let id = match wire {
        Wire::Openai(key) => return openai::edit(http, key, args, inputs).await,
        Wire::Gemini(_) => "gemini",
        Wire::Xai(_) => "xai",
        Wire::Local { .. } => "local",
    };
    Err(BuiltinFailure::new(
        types::C_ARGS,
        format!(
            "`mode: edit` is wired for openai (and mock) today — the {id} edit \
             adapter ships next (use provider: openai, or mock offline)"
        ),
    ))
}

/// A provider returning fewer images than `n:` is a WARNED degradation,
/// never silent (Ollama's compat route ignores `n` · openai may under-
/// deliver on moderation-filtered variants).
fn push_count_shortfall(args: &ImageArgs, delivered: usize, warnings: &mut Vec<String>) {
    if delivered < args.n as usize {
        warnings.push(format!(
            "count_shortfall: requested n: {} — the provider returned {delivered} image(s)",
            args.n
        ));
    }
}

/// Header-only decode validation over a provider batch — magic bytes are
/// the authority (a mislabel warns ONCE · a non-image payload hard-fails
/// `-007`), and each accepted image lands a `decoded` event.
fn validate_decoded<Em: Emitter>(
    images: Vec<types::RawImage>,
    args: &ImageArgs,
    emitter: &Em,
    warnings: &mut Vec<String>,
) -> Result<Vec<(types::RawImage, sniff::Sniffed, Vec<String>)>, BuiltinFailure> {
    let mut decoded = Vec::with_capacity(images.len());
    let mut format_mismatch_seen = false;
    for (index, image) in images.into_iter().enumerate() {
        let sniffed = sniff::sniff(&image.bytes)?;
        // A mismatch on the DEFAULT png is only worth a warning when the
        // caller ASKED for a format — xai returns jpeg by design and has
        // no output-format control; filenames follow magic either way.
        if sniffed.format != args.format && args.format_explicit && !format_mismatch_seen {
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
    Ok(decoded)
}

/// The over-the-wire subset of [`Provider`] — bound by the SAME match
/// that resolves credentials/config, so the adapter dispatch below needs
/// no second look at `Provider::Mock` (and therefore no panic-class
/// `unreachable!` arm in production src — zero-panic discipline).
enum Wire<'k> {
    Openai(&'k Secret),
    Gemini(&'k Secret),
    Xai(&'k Secret),
    Local {
        base_url: String,
        key: Option<&'k Secret>,
    },
}

/// The largest edit input we read — the `OpenAI` JSON data-URL ceiling,
/// held portably (bigger inputs get a clear `-001`, not a silent OOM).
const MAX_INPUT_BYTES: u64 = 20 * 1024 * 1024;

/// Read + permit-gate the `mode: edit` source images (and mask). Each path
/// is enforced against `permits.fs` for READ before any byte is read (the
/// exact mirror of the save-side `FsAccess::Write` gate · NIKA-SEC-004),
/// capped, and magic-byte-sniffed — a non-image input is a hard -001.
/// Generate mode reads nothing.
async fn read_edit_inputs<F, Em>(
    fs: &F,
    boundary: &FsBoundary,
    args: &ImageArgs,
    emitter: &Em,
) -> Result<Vec<types::InputImage>, BuiltinFailure>
where
    F: FsReadDyn + FsWriteDyn,
    Em: Emitter,
{
    if args.mode != types::Mode::Edit {
        return Ok(Vec::new());
    }
    let paths: Vec<&str> = args
        .input_paths
        .iter()
        .map(String::as_str)
        .chain(args.mask_path.as_deref())
        .collect();
    let mut inputs = Vec::with_capacity(args.input_paths.len());
    for (i, path) in paths.iter().enumerate() {
        boundary.enforce(fs, path, FsAccess::Read).await?;
        let bytes = fs.read(std::path::Path::new(path)).await.map_err(|e| {
            BuiltinFailure::new(
                types::C_ARGS,
                format!("edit input `{path}` could not be read: {e}"),
            )
        })?;
        if bytes.len() as u64 > MAX_INPUT_BYTES {
            return Err(BuiltinFailure::new(
                types::C_ARGS,
                format!(
                    "edit input `{path}` is {} bytes — the cap is {MAX_INPUT_BYTES} \
                     (~20MB · downscale it first)",
                    bytes.len()
                ),
            ));
        }
        let sniffed = sniff::sniff(&bytes).map_err(|_| {
            BuiltinFailure::new(
                types::C_ARGS,
                format!("edit input `{path}` is not a PNG/JPEG/WebP image"),
            )
        })?;
        emitter.emit(
            "image_generation.input_read",
            serde_json::json!({ "path": path, "bytes": bytes.len(),
                "role": if args.mask_path.is_some() && i + 1 == paths.len() { "mask" } else { "source" } }),
        );
        inputs.push(types::InputImage {
            path: (*path).to_owned(),
            sha256: save::sha256_hex(&bytes),
            format: sniffed.format,
            bytes: bytes.to_vec(),
        });
    }
    Ok(inputs)
}

async fn call_provider<H, C, Em>(
    http: Option<&H>,
    keys: &ImageKeys,
    args: &ImageArgs,
    inputs: &[types::InputImage],
    clock: &C,
    emitter: &Em,
    started: std::time::Instant,
) -> Result<ProviderBatch, BuiltinFailure>
where
    H: HttpPostDyn,
    C: ClockDyn,
    Em: Emitter,
{
    let (endpoint_host, wire) = match args.provider {
        Provider::Mock => return mock::generate(args, inputs),
        Provider::Openai => (
            "api.openai.com".to_owned(),
            Wire::Openai(keys.openai.as_ref().ok_or_else(|| {
                BuiltinFailure::new(
                    C_PROVIDER_UNAVAILABLE,
                    "no OpenAI API key — set NIKA_OPENAI_API_KEY or OPENAI_API_KEY \
                     in the engine's environment",
                )
            })?),
        ),
        Provider::Gemini => (
            "generativelanguage.googleapis.com".to_owned(),
            Wire::Gemini(keys.gemini.as_ref().ok_or_else(|| {
                BuiltinFailure::new(
                    C_PROVIDER_UNAVAILABLE,
                    "no Gemini API key — set NIKA_GEMINI_API_KEY or GEMINI_API_KEY \
                     in the engine's environment",
                )
            })?),
        ),
        Provider::Xai => (
            "api.x.ai".to_owned(),
            Wire::Xai(keys.xai.as_ref().ok_or_else(|| {
                BuiltinFailure::new(
                    C_PROVIDER_UNAVAILABLE,
                    "no xAI API key — set NIKA_XAI_API_KEY or XAI_API_KEY in the \
                     engine's environment",
                )
            })?),
        ),
        Provider::Local => {
            let base_url = keys
                .local_base_url
                .clone()
                .unwrap_or_else(|| local::DEFAULT_BASE_URL.to_owned());
            (
                local::host_of(&base_url),
                Wire::Local {
                    base_url,
                    key: keys.local_api_key.as_ref(),
                },
            )
        }
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
    let batch = match (args.mode, wire) {
        // ── generate (the default text→image path) ──
        (types::Mode::Generate, Wire::Openai(key)) => openai::generate(http, key, args).await?,
        (types::Mode::Generate, Wire::Gemini(key)) => gemini::generate(http, key, args).await?,
        (types::Mode::Generate, Wire::Xai(key)) => xai::generate(http, key, args).await?,
        (types::Mode::Generate, Wire::Local { base_url, key }) => {
            local::generate(http, &base_url, key, args).await?
        }
        // ── edit (source image(s) + instruction) · openai first (M-A.2) ──
        (types::Mode::Edit, wire) => dispatch_edit(http, wire, args, inputs).await?,
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

/// Assemble the normalized output object — paths + hashes + usage, never
/// bytes. `raw_provider_response` appears ONLY under `debug: true` (and is
/// pre-sanitized by the adapters).
#[allow(clippy::too_many_arguments)] // a pure projection of the pipeline's products
fn output_json(
    args: &ImageArgs,
    saved: &[SavedImage],
    usage: types::Usage,
    cost_usd: Option<f64>,
    content_credentials: Option<&str>,
    warnings: &[String],
    created_at: &str,
    revised_prompt: Option<&str>,
    provider_text: Option<&str>,
    endpoint_host: Option<&str>,
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
        "mode": args.mode.name(),
        "prompt": args.prompt,
        "revised_prompt": revised_prompt,
        "provider_text": provider_text,
        "endpoint_host": endpoint_host,
        "created_at": created_at,
        "count": saved.len(),
        "images": images,
        "usage": usage.to_json(),
        "cost_usd": cost_usd,
        "content_credentials": content_credentials,
        "warnings": warnings,
        "manifest_path": manifest_path,
        "output_dir": args.output_dir,
    });
    if let (Some(map), Some(raw)) = (output.as_object_mut(), raw_debug) {
        map.insert("raw_provider_response".to_owned(), raw);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(patch: serde_json::Value) -> ImageArgs {
        let serde_json::Value::Object(mut map) = serde_json::json!({
            "provider": "openai", "prompt": "star", "output_dir": "./out"
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

    fn saved() -> SavedImage {
        SavedImage {
            index: 0,
            path: "out/x.png".into(),
            filename: "x.png".into(),
            mime_type: "image/png",
            format: "png",
            width: 1,
            height: 1,
            size_bytes: 8,
            sha256: "ab".repeat(32),
            variant_id: "v".into(),
            seed: None,
            revised_prompt: None,
            warnings: Vec::new(),
        }
    }

    #[test]
    fn output_json_mode_is_edit_when_the_args_are_edit() {
        // #1136 · the output object hard-coded `"mode": "generate"` even
        // on an edit run. Downstream `outputs:` could never observe edit.
        let args = parsed(serde_json::json!({
            "mode": "edit", "image": "src.png", "model": "gpt-image-1.5",
        }));
        let out = output_json(
            &args,
            &[saved()],
            types::Usage::default(),
            None,
            None,
            &[],
            "2026-01-01T00:00:00Z",
            None,
            None,
            None,
            None,
            None,
        );
        assert_eq!(out["mode"], "edit");
        let generate_args = parsed(serde_json::json!({ "model": "gpt-image-1.5" }));
        let out = output_json(
            &generate_args,
            &[saved()],
            types::Usage::default(),
            None,
            None,
            &[],
            "2026-01-01T00:00:00Z",
            None,
            None,
            None,
            None,
            None,
        );
        assert_eq!(out["mode"], "generate");
    }
}
