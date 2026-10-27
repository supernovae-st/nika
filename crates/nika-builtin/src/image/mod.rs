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

use crate::media::cost::{CostBasis, MediaBill};
use crate::media::time::rfc3339_now;
use crate::permits::{FsAccess, FsBoundary};
use crate::{Args, BuiltinFailure, BuiltinOutcome, Emitter};

use self::args::ImageArgs;
use self::save::SavedImage;
use self::types::{C_PROVIDER_UNAVAILABLE, Provider, ProviderBatch, SizeSpec};

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

/// Catalog USD floor for one `nika:image_generate` call on a provider priced
/// whatever the model.
///
/// xAI's workhorse (`grok-imagine-image`) bills $0.02 per image; mock and
/// local are unpriced (`None`), and `OpenAI` and Gemini images are priced per
/// model, size class and quality from the dated media table
/// (`nika_catalog::MediaModel::image_floor`), never by this provider-wide
/// floor. `n` scales the floor when it is a known integer (≥ 1). The runtime
/// admission gate (NIKA-1709) consults the same catalog row so check-skip of
/// `invoke:` cannot spend-then-abort (B24 / issue 1296).
#[must_use]
pub fn static_floor_usd(provider: &str, n: u32) -> Option<f64> {
    let per = nika_catalog::builtin_provider_floor_usd("image_generate", provider)?;
    Some(per * f64::from(n.max(1)))
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
        unreported_images,
        cost_usd: reported,
        endpoint_host,
        provider_text,
        warnings: batch_warnings,
        raw_debug,
    } = batch;
    warnings.extend(batch_warnings);
    push_count_shortfall(&args, images.len(), &mut warnings);
    let delivered = Delivered {
        usage,
        count: images.len(),
        unreported: unreported_images,
    };
    let bill = bill(&args, reported, delivered); // C-D3 · what it cost
    // The provider rendered and billed: a failure from here on keeps what the
    // call cost, so the run's ledger and its cap still see it (C-D3).
    let spent = crate::media::cost::answered(
        bill,
        crate::media::cost::is_paid("image_generate", args.provider.id()),
    );
    let answer = Answer {
        usage,
        bill,
        endpoint_host,
        provider_text,
        raw_debug,
    };
    deliver(
        fs,
        clock,
        emitter,
        boundary,
        (&args, &inputs),
        images,
        answer,
        warnings,
        started,
    )
    .await
    .map_err(|failure| failure.with_spent(spent))
}

/// What the provider answered beside its images: the usage it reported, the
/// call's bill, and the provenance the output and the manifest carry.
struct Answer {
    usage: types::Usage,
    bill: Option<MediaBill>,
    endpoint_host: Option<String>,
    provider_text: Option<String>,
    raw_debug: Option<serde_json::Value>,
}

/// Everything after the provider answered (decode validation, saves, the
/// manifest, the output) — billed work: its caller keeps the bill on any
/// failure.
#[allow(clippy::too_many_arguments)] // the pipeline's seams, threaded once
async fn deliver<F, C, Em>(
    fs: &F,
    clock: &C,
    emitter: &Em,
    boundary: &FsBoundary,
    (args, inputs): (&ImageArgs, &[types::InputImage]),
    images: Vec<types::RawImage>,
    answer: Answer,
    mut warnings: Vec<String>,
    started: std::time::Instant,
) -> BuiltinOutcome
where
    F: FsReadDyn + FsWriteDyn,
    C: ClockDyn,
    Em: Emitter,
{
    let Answer {
        usage,
        bill,
        endpoint_host,
        provider_text,
        raw_debug,
    } = answer;
    // ── decode validation (header-only · magic authority) ───────────────
    let mut decoded = validate_decoded(images, args, emitter, &mut warnings)?;

    let content_credentials = detect_and_preserve_credentials(&mut decoded, args, emitter);

    // ── save (boundary-gated · atomic · cleanup on partial failure) ─────
    let saved = save::save_all(fs, boundary, args, decoded).await?;
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
        args,
        inputs,
        &saved,
        usage,
        bill,
        content_credentials,
        &warnings,
        &created_at,
        revised_prompt.as_deref(),
        provider_text.as_deref(),
        endpoint_host.as_deref(),
        emitter,
    )
    .await?;

    let cost_usd = crate::media::cost::usd(bill);
    emit_closing_events(emitter, clock, started, &saved, cost_usd, &warnings);

    Ok(output_json(
        args,
        &saved,
        usage,
        bill,
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

/// The images a provider delivered, and the usage the responses that
/// reported one carried (a gemini `n:` batch is `n` responses).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Delivered {
    /// The usage of the responses that reported their output.
    pub(crate) usage: types::Usage,
    /// Every image delivered.
    pub(crate) count: usize,
    /// The images whose response reported no usage (every one when no
    /// response reported its output).
    pub(crate) unreported: usize,
}

/// What this call cost (C-D3): the provider's own report when it gives one
/// (xAI's cost ticks), else the dated media table's price of its model, each
/// response billed on its own — its rates applied to the usage it reported,
/// or, for the images of a response that reported none, the per-image price
/// at the requested size class and quality (the dearest the table lists over
/// what the request leaves open: never below what the provider may have
/// rendered). `None` for a model the table does not price, and for a token-
/// priced model whose response reports no usage (the spend is real and
/// unknown, never free), and for unmetered providers.
pub(crate) fn bill(
    args: &ImageArgs,
    reported: Option<f64>,
    delivered: Delivered,
) -> Option<MediaBill> {
    if let Some(usd) = reported {
        return Some(MediaBill::reported(usd));
    }
    let nika_catalog::MediaPrice::Rated(model) =
        nika_catalog::builtin_media_price("image_generate", args.provider.id(), Some(&args.model))
    else {
        return None;
    };
    let unreported = if delivered.usage.reports_output() {
        delivered.unreported.min(delivered.count)
    } else {
        delivered.count
    };
    let by_usage = match delivered.count - unreported {
        0 => None,
        _ => Some(model.image_usage_usd(&delivered.usage.to_catalog())?),
    };
    let by_image = match unreported {
        0 => None,
        images => {
            let rate = image_ceiling(args, model)?;
            #[allow(clippy::cast_precision_loss)] // `n:` is capped at 10 images a call
            Some((rate.usd * images as f64, rate.as_of))
        }
    };
    match (by_usage, by_image) {
        (Some(usage_usd), images) => Some(MediaBill::catalog(
            usage_usd + images.map_or(0.0, |(usd, _)| usd),
            CostBasis::RateTimesUsage,
            model.as_of(),
        )),
        (None, Some((usd, as_of))) => {
            Some(MediaBill::catalog(usd, CostBasis::PriceTimesImages, as_of))
        }
        (None, None) => None,
    }
}

/// The dearest per-image row of `model` the request can be billed at: its
/// size class and quality as the dated table reads them.
fn image_ceiling(
    args: &ImageArgs,
    model: nika_catalog::MediaModel,
) -> Option<&'static nika_catalog::MediaRate> {
    let size = match args.size {
        SizeSpec::Exact { width, height } => Some(format!("{width}x{height}")),
        SizeSpec::Aspect(_) | SizeSpec::Auto => None,
    };
    let aspect = match args.size {
        SizeSpec::Aspect(ratio) => Some(ratio.name()),
        SizeSpec::Exact { .. } | SizeSpec::Auto => None,
    };
    let request = nika_catalog::ImageRequest::new()
        .with_size(size.as_deref())
        .with_aspect_ratio(aspect)
        .with_image_size(
            args.provider_options
                .get("image_size")
                .and_then(serde_json::Value::as_str),
        )
        .with_quality(Some(args.quality.name()));
    model.image_ceiling(&request)
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
    bill: Option<MediaBill>,
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
    let mut document = manifest::build(
        args,
        inputs,
        saved,
        usage,
        crate::media::cost::usd(bill),
        content_credentials,
        warnings,
        created_at,
        revised_prompt,
        provider_text,
        endpoint_host,
    );
    if let Some(bill) = bill {
        bill.annotate(&mut document);
    }
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
        let per_image = if args.provider == types::Provider::Mock {
            vec!["rehearsal · not a real image".to_owned()]
        } else {
            Vec::new()
        };
        decoded.push((image, sniffed, per_image));
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
/// bytes; a billed call's `cost_usd` with its `cost_basis` and `cost_as_of`
/// (C-D3). `raw_provider_response` appears ONLY under `debug: true` (and is
/// pre-sanitized by the adapters).
#[allow(clippy::too_many_arguments)] // a pure projection of the pipeline's products
fn output_json(
    args: &ImageArgs,
    saved: &[SavedImage],
    usage: types::Usage,
    bill: Option<MediaBill>,
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
        "cost_usd": crate::media::cost::usd(bill),
        "content_credentials": content_credentials,
        "warnings": warnings,
        "manifest_path": manifest_path,
        "output_dir": args.output_dir,
    });
    if let Some(bill) = bill {
        bill.annotate(&mut output);
    }
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

    fn approx(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < 1e-12, "{expected} ≠ {actual}");
    }

    /// `count` images delivered, `unreported` of them by a response that
    /// reported no usage, the others' usage `usage`.
    fn delivered(usage: types::Usage, count: usize, unreported: usize) -> Delivered {
        Delivered {
            usage,
            count,
            unreported,
        }
    }

    /// C-D3 · a priced image is billed from the dated media table: its
    /// token rates × the usage the response reported, else (no usage) its
    /// per-image price at the requested size class and quality × the images
    /// delivered; the provider's own report wins; an unpriced model, and a
    /// token-priced one with no usage, bill nothing (unknown, never free).
    #[test]
    fn a_priced_image_is_billed_from_its_usage_else_its_per_image_price() {
        let high = parsed(serde_json::json!({ "size": "1024x1024", "quality": "high" }));
        let usage = types::Usage {
            input_tokens: Some(10),
            input_text_tokens: Some(10),
            input_image_tokens: Some(0),
            output_tokens: Some(1_000),
            ..types::Usage::default()
        };
        let none = types::Usage::default();
        let billed = bill(&high, None, delivered(usage, 1, 0)).expect("gpt-image-2 is priced");
        approx(billed.usd, 10.0 * 5.0 / 1e6 + 1_000.0 * 30.0 / 1e6);
        assert_eq!(billed.basis, CostBasis::RateTimesUsage);
        assert_eq!(billed.as_of, Some(nika_catalog::MEDIA_PRICING_AS_OF));

        let billed = bill(&high, None, delivered(none, 2, 0)).expect("priced per image");
        approx(billed.usd, 0.422);
        assert_eq!(billed.basis, CostBasis::PriceTimesImages);

        let gemini = parsed(serde_json::json!({ "provider": "gemini", "aspect_ratio": "16:9" }));
        let billed = bill(&gemini, None, delivered(none, 1, 0)).expect("priced");
        approx(billed.usd, 0.067);

        let reported = bill(&high, Some(0.5), delivered(usage, 1, 0)).expect("reported");
        assert_eq!(reported, MediaBill::reported(0.5));

        let flare = parsed(serde_json::json!({ "model": "gpt-image-2.5-flare" }));
        assert_eq!(bill(&flare, None, delivered(none, 1, 0)), None);
        let billed = bill(&flare, None, delivered(usage, 1, 0)).expect("metered from its usage");
        assert_eq!(billed.basis, CostBasis::RateTimesUsage);

        let older = parsed(serde_json::json!({ "model": "gpt-image-1.5" }));
        assert_eq!(
            bill(&older, None, delivered(usage, 1, 0)),
            None,
            "no catalog price"
        );
        let mock = parsed(serde_json::json!({ "provider": "mock" }));
        assert_eq!(
            bill(&mock, None, delivered(usage, 1, 0)),
            None,
            "never billed"
        );
    }

    /// C-D3 · an image whose response reports no usage is billed
    /// at the dearest row the request leaves open, never the cheapest: a
    /// default request (`quality: auto`, no size) is billed $0.211, not
    /// $0.005; a request naming its size and quality is billed that row.
    #[test]
    fn an_image_without_usage_is_billed_at_the_dearest_row_left_open() {
        let none = types::Usage::default();
        let open = parsed(serde_json::json!({}));
        approx(
            bill(&open, None, delivered(none, 1, 0))
                .expect("priced")
                .usd,
            0.211,
        );
        let sized = parsed(serde_json::json!({ "size": "1536x1024" }));
        approx(
            bill(&sized, None, delivered(none, 1, 0))
                .expect("priced")
                .usd,
            0.165,
        );
        let named = parsed(serde_json::json!({ "size": "1024x1024", "quality": "low" }));
        approx(
            bill(&named, None, delivered(none, 1, 0))
                .expect("priced")
                .usd,
            0.006,
        );
    }

    /// C-D3 · each response of a gemini batch is billed on its
    /// own: two images, one whose response reported its usage and one whose
    /// did not, bill the first from its usage and the second at the price
    /// per image (the batch used to bill one image).
    #[test]
    fn each_response_of_a_batch_is_billed_on_its_own() {
        let gemini = parsed(serde_json::json!({ "provider": "gemini" }));
        let one = types::Usage {
            input_tokens: Some(20),
            output_tokens: Some(1_120),
            output_image_tokens: Some(1_120),
            ..types::Usage::default()
        };
        let billed = bill(&gemini, None, delivered(one, 2, 1)).expect("priced");
        approx(billed.usd, 20.0 * 0.5 / 1e6 + 1_120.0 * 60.0 / 1e6 + 0.067);
        assert_eq!(billed.basis, CostBasis::RateTimesUsage);
        let billed = bill(&gemini, None, delivered(one, 2, 0)).expect("priced");
        approx(billed.usd, 20.0 * 0.5 / 1e6 + 1_120.0 * 60.0 / 1e6);
    }

    /// The catalog reads an aspect ratio as the exact size this adapter
    /// sends `gpt-image-2` for it: the floor of the ratio is the floor of the
    /// size, quality by quality.
    #[test]
    fn the_catalog_reads_an_aspect_ratio_as_the_size_the_adapter_sends() {
        let model = nika_catalog::find_media_model("image_generate", "openai", "gpt-image-2")
            .expect("priced");
        for ratio in types::AspectRatio::ALL {
            let (width, height) = ratio.openai_size();
            let size = format!("{width}x{height}");
            for quality in ["auto", "low", "medium", "high", "ultra"] {
                let by_ratio = nika_catalog::ImageRequest::new()
                    .with_aspect_ratio(Some(ratio.name()))
                    .with_quality(Some(quality));
                let by_size = nika_catalog::ImageRequest::new()
                    .with_size(Some(&size))
                    .with_quality(Some(quality));
                assert_eq!(
                    model.image_floor(&by_ratio),
                    model.image_floor(&by_size),
                    "{} at {quality}",
                    ratio.name()
                );
            }
        }
    }

    #[test]
    fn xai_static_floor_is_two_cents_per_image() {
        let one = static_floor_usd("xai", 1).expect("priced");
        assert!(
            (one - 0.02).abs() < 1e-12,
            "xAI workhorse is $0.02/image, got {one}"
        );
        let two = static_floor_usd("xai", 2).expect("priced");
        assert!((two - 0.04).abs() < 1e-12, "n scales the floor, got {two}");
        assert_eq!(static_floor_usd("mock", 1), None);
        assert_eq!(static_floor_usd("local", 1), None);
        assert_eq!(
            static_floor_usd("xai", 1),
            nika_catalog::builtin_provider_floor_usd("image_generate", "xai"),
            "builtin floor and catalog row are one number"
        );
    }
}
