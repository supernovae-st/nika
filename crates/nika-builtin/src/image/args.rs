// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika:image_generate` argument parsing + validation — the pure layer
//! (no I/O). Every rejection is `NIKA-BUILTIN-IMAGE_GENERATE-001` with an
//! actionable hint; every tolerated-but-lossy mapping lands a stable
//! `code: message` warning (the mission's « warning clair » contract).
//!
//! The static `nika check` ladder (`analyzer::builtin_shape`) catches the
//! LITERAL versions of these violations pre-run; this is the runtime
//! defense for templated values and hand-built `ToolCall`s.

use std::time::Duration;

use crate::{Args, BuiltinFailure, opt_str};

use super::types::{AspectRatio, Background, C_ARGS, ImageFormat, Provider, Quality, SizeSpec};

/// Default request deadline — image generation routinely runs 30–120s
/// (`OpenAI` documents « up to 2 minutes » for complex prompts); the fetch
/// client's 30s default would kill most calls (the F1 timeout class).
const DEFAULT_TIMEOUT_MS: u64 = 180_000;
/// `timeout_ms:` bounds — under 1s is a typo'd unit; over 600s exceeds the
/// provider transport ceiling (the composition root's idle-read guard).
const TIMEOUT_RANGE_MS: std::ops::RangeInclusive<u64> = 1_000..=600_000;
/// `n:` ceiling — the `OpenAI` native maximum; gemini runs `n` sequential
/// calls, so the same cap bounds spend.
const MAX_N: u64 = 10;
/// `OpenAI`'s documented prompt ceiling for the GPT image models.
const OPENAI_MAX_PROMPT_CHARS: usize = 32_000;
/// Parse-level sanity bounds for an exact `size:` edge — providers apply
/// their own tighter windows on top.
const SIZE_EDGE_RANGE: std::ops::RangeInclusive<u32> = 16..=8_192;

/// The parsed, validated, normalized `image_generate` request.
#[derive(Debug)]
pub(crate) struct ImageArgs {
    pub(crate) provider: Provider,
    pub(crate) model: String,
    pub(crate) prompt: String,
    pub(crate) n: u32,
    pub(crate) size: SizeSpec,
    pub(crate) quality: Quality,
    pub(crate) format: ImageFormat,
    /// 0–100 · `Some` only when applicable (jpeg/webp).
    pub(crate) compression: Option<u8>,
    pub(crate) background: Background,
    pub(crate) seed: Option<i64>,
    /// Provider-specific pass-through (vetted per adapter · never crashes).
    pub(crate) provider_options: serde_json::Map<String, serde_json::Value>,
    pub(crate) output_dir: String,
    pub(crate) filename_prefix: Option<String>,
    pub(crate) manifest: bool,
    /// Free-form provenance (campaign · `page_slug` · locale · …) echoed
    /// into the output and the manifest.
    pub(crate) metadata: serde_json::Map<String, serde_json::Value>,
    pub(crate) timeout: Duration,
    pub(crate) debug: bool,
    /// Parse-time warnings (`code: message`) — the orchestrator appends
    /// adapter + save warnings after these.
    pub(crate) warnings: Vec<String>,
}

/// Parse + validate the raw `args:` object.
pub(crate) fn parse(args: &Args) -> Result<ImageArgs, BuiltinFailure> {
    let mut warnings = Vec::new();

    reject_v1_unsupported(args)?;

    let prompt = args
        .get("prompt")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .ok_or_else(|| {
            BuiltinFailure::new(
                C_ARGS,
                "`prompt:` (non-empty string) is required — the creative brief the \
                 provider renders",
            )
        })?
        .to_owned();

    let output_dir = args
        .get("output_dir")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .ok_or_else(|| {
            BuiltinFailure::new(
                C_ARGS,
                "`output_dir:` (non-empty string) is required — generated assets are \
                 saved to disk (V1 contract)",
            )
        })?
        .to_owned();

    let (provider, model) = resolve_provider_model(args, &mut warnings)?;

    if provider == Provider::Openai && prompt.chars().count() > OPENAI_MAX_PROMPT_CHARS {
        return Err(BuiltinFailure::new(
            C_ARGS,
            format!(
                "`prompt:` is {} chars — the OpenAI image models cap prompts at \
                 {OPENAI_MAX_PROMPT_CHARS} chars",
                prompt.chars().count()
            ),
        ));
    }

    let n = parse_n(args)?;
    let format = parse_format(args)?;
    let quality = parse_quality(args)?;
    let background = parse_background(args, provider, &model, format)?;
    let size = parse_size(args, &mut warnings)?;
    let compression = parse_compression(args, format, &mut warnings)?;
    let seed = parse_seed(args, provider, &mut warnings)?;
    let provider_options = parse_provider_options(args, provider, &mut warnings)?;
    let timeout = parse_timeout(args)?;

    let manifest = crate::opt_bool(args, "manifest", true);
    let debug = crate::opt_bool(args, "debug", false);
    let filename_prefix = opt_str(args, "filename_prefix", C_ARGS)?.map(str::to_owned);
    let metadata = match args.get("metadata") {
        None => serde_json::Map::new(),
        Some(serde_json::Value::Object(map)) => map.clone(),
        Some(_) => {
            return Err(BuiltinFailure::new(
                C_ARGS,
                "`metadata:` must be an object (free-form provenance fields)",
            ));
        }
    };

    Ok(ImageArgs {
        provider,
        model,
        prompt,
        n,
        size,
        quality,
        format,
        compression,
        background,
        seed,
        provider_options,
        output_dir,
        filename_prefix,
        manifest,
        metadata,
        timeout,
        debug,
        warnings,
    })
}

/// The V1-boundary rejections — structurally reserved options that are
/// NOT silently ignored and NOT half-implemented (mission contract).
fn reject_v1_unsupported(args: &Args) -> Result<(), BuiltinFailure> {
    match opt_str(args, "mode", C_ARGS)? {
        None | Some("generate") => {}
        Some("edit") => {
            return Err(BuiltinFailure::new(
                C_ARGS,
                "`mode: edit` is not supported yet — V1 ships `generate` only; image \
                 editing (reference-image edits) is on the media roadmap",
            ));
        }
        Some(other) => {
            return Err(BuiltinFailure::new(
                C_ARGS,
                format!("`mode: {other}` is not a mode — V1 supports `generate`"),
            ));
        }
    }
    if args.contains_key("reference_images") {
        return Err(BuiltinFailure::new(
            C_ARGS,
            "`reference_images:` is not supported yet — V1 is text-to-image only; \
             reference-image composition is on the media roadmap",
        ));
    }
    if args.get("save").and_then(serde_json::Value::as_bool) == Some(false) {
        return Err(BuiltinFailure::new(
            C_ARGS,
            "`save: false` is not supported — V1 always writes assets to \
             `output_dir:` (base64 payloads never ride workflow outputs); an \
             in-memory mode is on the media roadmap",
        ));
    }
    Ok(())
}

/// Resolve `(provider, model)` — explicit `provider:` wins; else inferred
/// from the `model:` prefix; neither → a clear 001.
fn resolve_provider_model(
    args: &Args,
    warnings: &mut Vec<String>,
) -> Result<(Provider, String), BuiltinFailure> {
    let model_arg = opt_str(args, "model", C_ARGS)?.map(str::to_owned);
    let provider = match opt_str(args, "provider", C_ARGS)? {
        Some("openai") => Provider::Openai,
        Some("gemini") => Provider::Gemini,
        Some("mock") => Provider::Mock,
        Some(other) => {
            return Err(BuiltinFailure::new(
                C_ARGS,
                format!(
                    "`provider: {other}` is not a V1 image provider — the set is \
                     `openai` · `gemini` · `mock`"
                ),
            ));
        }
        None => match model_arg.as_deref() {
            Some(m) if m.starts_with("gpt-image") => Provider::Openai,
            Some(m) if m.starts_with("gemini") => Provider::Gemini,
            Some(m) if m.starts_with("mock") => Provider::Mock,
            Some(m) if m.starts_with("dall-e") => {
                return Err(BuiltinFailure::new(
                    C_ARGS,
                    "dall-e models were removed from the OpenAI API (2026-05-12) — \
                     use `gpt-image-2`",
                ));
            }
            Some(m) if m.starts_with("imagen") => {
                return Err(BuiltinFailure::new(
                    C_ARGS,
                    "Imagen rides a separate `:predict` API (shut down 2026-08-17) — \
                     use `gemini-3.1-flash-image`",
                ));
            }
            Some(m) => {
                return Err(BuiltinFailure::new(
                    C_ARGS,
                    format!(
                        "cannot infer a provider from `model: {m}` — set `provider:` \
                         (`openai` · `gemini` · `mock`)"
                    ),
                ));
            }
            None => {
                return Err(BuiltinFailure::new(
                    C_ARGS,
                    "`provider:` (or a `model:` it can be inferred from) is required — \
                     `openai` · `gemini` · `mock`",
                ));
            }
        },
    };
    let model = model_arg.unwrap_or_else(|| provider.default_model().to_owned());

    if provider == Provider::Gemini {
        // The model lands in the URL path (`models/{model}:generateContent`)
        // — confine it to the id charset so a templated model can never
        // splice a different API path (host is const · this pins the path).
        if !model
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        {
            return Err(BuiltinFailure::new(
                C_ARGS,
                format!("`model: {model}` has characters outside the gemini id charset"),
            ));
        }
        if model.ends_with("-preview") {
            warnings.push(format!(
                "preview_model: `{model}` is a preview id — previews are short-lived \
                 (the gemini-3 image previews shut down 2026-06-25); prefer the GA id"
            ));
        }
    }
    Ok((provider, model))
}

fn parse_n(args: &Args) -> Result<u32, BuiltinFailure> {
    match args.get("n") {
        None => Ok(1),
        Some(value) => {
            let n = value.as_u64().ok_or_else(|| {
                BuiltinFailure::new(C_ARGS, "`n:` must be an integer between 1 and 10")
            })?;
            if n == 0 || n > MAX_N {
                return Err(BuiltinFailure::new(
                    C_ARGS,
                    format!("`n: {n}` is out of range — 1..=10 (got the request budget?)"),
                ));
            }
            u32::try_from(n).map_err(|_| BuiltinFailure::new(C_ARGS, "`n:` must fit an integer"))
        }
    }
}

fn parse_format(args: &Args) -> Result<ImageFormat, BuiltinFailure> {
    match opt_str(args, "format", C_ARGS)? {
        None | Some("png") => Ok(ImageFormat::Png),
        Some("jpeg" | "jpg") => Ok(ImageFormat::Jpeg),
        Some("webp") => Ok(ImageFormat::Webp),
        Some(other) => Err(BuiltinFailure::new(
            C_ARGS,
            format!("`format: {other}` is not supported — png (default) · jpeg · webp"),
        )),
    }
}

fn parse_quality(args: &Args) -> Result<Quality, BuiltinFailure> {
    match opt_str(args, "quality", C_ARGS)? {
        None | Some("auto") => Ok(Quality::Auto),
        Some("low") => Ok(Quality::Low),
        Some("medium") => Ok(Quality::Medium),
        Some("high") => Ok(Quality::High),
        Some("ultra") => Ok(Quality::Ultra),
        Some(other) => Err(BuiltinFailure::new(
            C_ARGS,
            format!("`quality: {other}` is not a level — auto · low · medium · high · ultra"),
        )),
    }
}

/// `background:` — transparency is validated HERE, before any request is
/// spent: it needs png/webp AND a provider/model documented to honor it.
fn parse_background(
    args: &Args,
    provider: Provider,
    model: &str,
    format: ImageFormat,
) -> Result<Background, BuiltinFailure> {
    let background = match opt_str(args, "background", C_ARGS)? {
        None | Some("auto") => Background::Auto,
        Some("transparent") => Background::Transparent,
        Some("opaque") => Background::Opaque,
        Some(other) => {
            return Err(BuiltinFailure::new(
                C_ARGS,
                format!("`background: {other}` is not a value — auto · transparent · opaque"),
            ));
        }
    };
    if background == Background::Transparent {
        if format == ImageFormat::Jpeg {
            return Err(BuiltinFailure::new(
                C_ARGS,
                "`background: transparent` needs an alpha-capable `format:` — png or webp",
            ));
        }
        match provider {
            Provider::Openai if model.starts_with("gpt-image-2") => {
                return Err(BuiltinFailure::new(
                    C_ARGS,
                    "gpt-image-2 does not support `background: transparent` (documented \
                     provider limitation) — use `gpt-image-1.5` (available until \
                     2026-12-01) or drop the transparency requirement",
                ));
            }
            Provider::Gemini => {
                return Err(BuiltinFailure::new(
                    C_ARGS,
                    "the gemini image models do not document transparent-background \
                     output — use openai `gpt-image-1.5` for transparency",
                ));
            }
            Provider::Openai | Provider::Mock => {}
        }
    }
    Ok(background)
}

/// `size:` vs `aspect_ratio:` — an exact `size:` wins (with a warning when
/// both are given); `size: auto` defers to `aspect_ratio:`.
fn parse_size(args: &Args, warnings: &mut Vec<String>) -> Result<SizeSpec, BuiltinFailure> {
    let aspect = match opt_str(args, "aspect_ratio", C_ARGS)? {
        None => None,
        Some(raw) => Some(
            AspectRatio::ALL
                .into_iter()
                .find(|a| a.name() == raw)
                .ok_or_else(|| {
                    let all: Vec<&str> = AspectRatio::ALL.iter().map(|a| a.name()).collect();
                    BuiltinFailure::new(
                        C_ARGS,
                        format!(
                            "`aspect_ratio: {raw}` is not in the closed set — {}",
                            all.join(" · ")
                        ),
                    )
                })?,
        ),
    };
    let size = match opt_str(args, "size", C_ARGS)? {
        None | Some("auto") => None,
        Some(raw) => {
            let (w, h) = raw.split_once('x').ok_or_else(|| {
                BuiltinFailure::new(
                    C_ARGS,
                    format!("`size: {raw}` must be `WIDTHxHEIGHT` (e.g. 1024x1024) or `auto`"),
                )
            })?;
            let width: u32 = w.parse().map_err(|_| {
                BuiltinFailure::new(C_ARGS, format!("`size: {raw}` — width is not an integer"))
            })?;
            let height: u32 = h.parse().map_err(|_| {
                BuiltinFailure::new(C_ARGS, format!("`size: {raw}` — height is not an integer"))
            })?;
            if !SIZE_EDGE_RANGE.contains(&width) || !SIZE_EDGE_RANGE.contains(&height) {
                return Err(BuiltinFailure::new(
                    C_ARGS,
                    format!("`size: {raw}` — each edge must be within 16..=8192"),
                ));
            }
            Some(SizeSpec::Exact { width, height })
        }
    };
    Ok(match (size, aspect) {
        (Some(exact), Some(ratio)) => {
            warnings.push(format!(
                "size_conflict: both `size:` and `aspect_ratio: {}` were given — the \
                 exact `size:` wins (documented rule)",
                ratio.name()
            ));
            exact
        }
        (Some(exact), None) => exact,
        (None, Some(ratio)) => SizeSpec::Aspect(ratio),
        (None, None) => SizeSpec::Auto,
    })
}

fn parse_compression(
    args: &Args,
    format: ImageFormat,
    warnings: &mut Vec<String>,
) -> Result<Option<u8>, BuiltinFailure> {
    let Some(value) = args.get("compression") else {
        return Ok(None);
    };
    let raw = value.as_u64().ok_or_else(|| {
        BuiltinFailure::new(
            C_ARGS,
            "`compression:` must be an integer between 0 and 100",
        )
    })?;
    if raw > 100 {
        return Err(BuiltinFailure::new(
            C_ARGS,
            format!("`compression: {raw}` is out of range — 0..=100"),
        ));
    }
    if format == ImageFormat::Png {
        warnings.push(
            "compression_ignored: `compression:` applies to jpeg/webp — png is \
             lossless, the value is ignored"
                .to_owned(),
        );
        return Ok(None);
    }
    #[allow(clippy::cast_possible_truncation)] // 0..=100 proven above
    Ok(Some(raw as u8))
}

fn parse_seed(
    args: &Args,
    provider: Provider,
    warnings: &mut Vec<String>,
) -> Result<Option<i64>, BuiltinFailure> {
    let Some(value) = args.get("seed") else {
        return Ok(None);
    };
    let seed = value
        .as_i64()
        .ok_or_else(|| BuiltinFailure::new(C_ARGS, "`seed:` must be an integer"))?;
    match provider {
        Provider::Openai => {
            warnings.push(
                "seed_unsupported: the OpenAI Images API has no seed parameter — \
                 the seed is ignored"
                    .to_owned(),
            );
            Ok(None)
        }
        Provider::Gemini => {
            warnings.push(
                "seed_best_effort: gemini accepts `generationConfig.seed` but does \
                 not document image determinism — treat variants as best-effort"
                    .to_owned(),
            );
            Ok(Some(seed))
        }
        Provider::Mock => Ok(Some(seed)),
    }
}

/// Vet `provider_options:` per provider — KNOWN keys get their values
/// validated; unknown keys warn and pass through untouched (new provider
/// options must never crash the builtin — forward-compat contract).
fn parse_provider_options(
    args: &Args,
    provider: Provider,
    warnings: &mut Vec<String>,
) -> Result<serde_json::Map<String, serde_json::Value>, BuiltinFailure> {
    let options = match args.get("provider_options") {
        None => return Ok(serde_json::Map::new()),
        Some(serde_json::Value::Object(map)) => map.clone(),
        Some(_) => {
            return Err(BuiltinFailure::new(
                C_ARGS,
                "`provider_options:` must be an object",
            ));
        }
    };
    let known: &[&str] = match provider {
        Provider::Openai => &["moderation", "user"],
        Provider::Gemini => &["thinking_level", "image_size"],
        Provider::Mock => &[],
    };
    for (key, value) in &options {
        if !known.contains(&key.as_str()) {
            warnings.push(format!(
                "provider_option_unknown: `provider_options.{key}` is not a known \
                 {} option — passed over, not sent",
                provider.id()
            ));
            continue;
        }
        // Order-dependent: each OK-arm must fire BEFORE its key's
        // catch-all error arm — merging the identical `{}` bodies would
        // reorder the match and reroute valid values into the errors.
        #[allow(clippy::match_same_arms)]
        match (provider, key.as_str(), value.as_str()) {
            (Provider::Openai, "moderation", Some("auto" | "low")) => {}
            (Provider::Openai, "moderation", _) => {
                return Err(BuiltinFailure::new(
                    C_ARGS,
                    "`provider_options.moderation` must be `auto` or `low`",
                ));
            }
            (Provider::Openai, "user", Some(_)) => {}
            (Provider::Openai, "user", None) => {
                return Err(BuiltinFailure::new(
                    C_ARGS,
                    "`provider_options.user` must be a string",
                ));
            }
            (Provider::Gemini, "thinking_level", Some("minimal" | "high")) => {}
            (Provider::Gemini, "thinking_level", _) => {
                return Err(BuiltinFailure::new(
                    C_ARGS,
                    "`provider_options.thinking_level` must be `minimal` or `high` \
                     (gemini-3.1-flash-image family)",
                ));
            }
            (Provider::Gemini, "image_size", Some("512" | "1K" | "2K" | "4K")) => {}
            (Provider::Gemini, "image_size", _) => {
                return Err(BuiltinFailure::new(
                    C_ARGS,
                    "`provider_options.image_size` must be `512` · `1K` · `2K` · `4K` \
                     (uppercase K — gemini rejects lowercase)",
                ));
            }
            _ => {}
        }
    }
    Ok(options)
}

fn parse_timeout(args: &Args) -> Result<Duration, BuiltinFailure> {
    let Some(value) = args.get("timeout_ms") else {
        return Ok(Duration::from_millis(DEFAULT_TIMEOUT_MS));
    };
    let ms = value.as_u64().ok_or_else(|| {
        BuiltinFailure::new(C_ARGS, "`timeout_ms:` must be an integer (milliseconds)")
    })?;
    if !TIMEOUT_RANGE_MS.contains(&ms) {
        return Err(BuiltinFailure::new(
            C_ARGS,
            format!("`timeout_ms: {ms}` is out of range — 1000..=600000 (1s to 10min)"),
        ));
    }
    Ok(Duration::from_millis(ms))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: serde_json::Value) -> Args {
        match v {
            serde_json::Value::Object(map) => map,
            other => panic!("test args must be an object, got {other}"),
        }
    }

    fn base(v: serde_json::Value) -> Args {
        let mut map = args(serde_json::json!({
            "provider": "mock", "prompt": "a butterfly", "output_dir": "./out"
        }));
        for (k, val) in args(v) {
            map.insert(k, val);
        }
        map
    }

    #[test]
    fn minimal_call_gets_the_documented_defaults() {
        let parsed = parse(&base(serde_json::json!({}))).expect("valid");
        assert_eq!(parsed.provider, Provider::Mock);
        assert_eq!(parsed.model, "mock-image-1");
        assert_eq!(parsed.n, 1);
        assert_eq!(parsed.size, SizeSpec::Auto);
        assert_eq!(parsed.quality, Quality::Auto);
        assert_eq!(parsed.format, ImageFormat::Png);
        assert_eq!(parsed.background, Background::Auto);
        assert!(
            parsed.manifest,
            "manifest defaults true (assets, not blobs)"
        );
        assert!(!parsed.debug);
        assert_eq!(parsed.timeout, Duration::from_millis(180_000));
        assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
    }

    #[test]
    fn empty_prompt_and_missing_output_dir_are_rejected() {
        for (patch, needle) in [
            (serde_json::json!({ "prompt": "" }), "prompt"),
            (serde_json::json!({ "prompt": "   " }), "prompt"),
            (serde_json::json!({ "output_dir": "" }), "output_dir"),
        ] {
            let mut map = base(patch.clone());
            if patch.get("output_dir").is_some() {
                // keep prompt valid for the output_dir case
                map.insert("prompt".into(), serde_json::json!("x"));
            }
            let err = parse(&map).expect_err("rejected");
            assert_eq!(err.code, C_ARGS);
            assert!(err.message.contains(needle), "{}", err.message);
        }
        let mut missing = base(serde_json::json!({}));
        missing.remove("prompt");
        assert!(parse(&missing).is_err(), "absent prompt rejected");
    }

    #[test]
    fn provider_resolution_explicit_inferred_and_failing() {
        // explicit wins
        let p = parse(&base(serde_json::json!({ "provider": "openai" }))).expect("valid");
        assert_eq!(p.provider, Provider::Openai);
        assert_eq!(p.model, "gpt-image-2", "openai default model");
        // inferred from model
        let mut map = base(serde_json::json!({ "model": "gemini-3-pro-image" }));
        map.remove("provider");
        let p = parse(&map).expect("valid");
        assert_eq!(p.provider, Provider::Gemini);
        assert_eq!(p.model, "gemini-3-pro-image");
        // unknown provider
        let err =
            parse(&base(serde_json::json!({ "provider": "midjourney" }))).expect_err("rejected");
        assert!(err.message.contains("openai"), "hint lists the set");
        // neither provider nor model
        let mut neither = base(serde_json::json!({}));
        neither.remove("provider");
        let err = parse(&neither).expect_err("rejected");
        assert!(err.message.contains("provider"), "{}", err.message);
        // dead model families get a pointed hint
        let mut dalle = base(serde_json::json!({ "model": "dall-e-3" }));
        dalle.remove("provider");
        let err = parse(&dalle).expect_err("rejected");
        assert!(err.message.contains("gpt-image-2"), "{}", err.message);
        let mut imagen = base(serde_json::json!({ "model": "imagen-4.0-generate-001" }));
        imagen.remove("provider");
        let err = parse(&imagen).expect_err("rejected");
        assert!(
            err.message.contains("gemini-3.1-flash-image"),
            "{}",
            err.message
        );
    }

    #[test]
    fn v1_unsupported_options_are_loud_never_silent() {
        let edit = parse(&base(serde_json::json!({ "mode": "edit" }))).expect_err("edit");
        assert!(edit.message.contains("roadmap"), "{}", edit.message);
        let refs = parse(&base(serde_json::json!({ "reference_images": ["a.png"] })))
            .expect_err("reference_images");
        assert!(refs.message.contains("roadmap"), "{}", refs.message);
        let save = parse(&base(serde_json::json!({ "save": false }))).expect_err("save false");
        assert!(save.message.contains("base64"), "{}", save.message);
        // …while the supported spellings pass.
        assert!(
            parse(&base(
                serde_json::json!({ "mode": "generate", "save": true })
            ))
            .is_ok()
        );
    }

    #[test]
    fn n_bounds_are_enforced() {
        assert!(parse(&base(serde_json::json!({ "n": 10 }))).is_ok());
        for bad in [
            serde_json::json!(0),
            serde_json::json!(11),
            serde_json::json!(2.5),
        ] {
            let err = parse(&base(serde_json::json!({ "n": bad }))).expect_err("rejected");
            assert_eq!(err.code, C_ARGS);
        }
    }

    #[test]
    fn format_quality_background_enums_are_closed() {
        assert!(parse(&base(serde_json::json!({ "format": "webp" }))).is_ok());
        let p = parse(&base(serde_json::json!({ "format": "jpg" }))).expect("jpg alias");
        assert_eq!(p.format, ImageFormat::Jpeg);
        assert!(parse(&base(serde_json::json!({ "format": "gif" }))).is_err());
        assert!(parse(&base(serde_json::json!({ "quality": "ultra" }))).is_ok());
        assert!(parse(&base(serde_json::json!({ "quality": "hd" }))).is_err());
        assert!(parse(&base(serde_json::json!({ "background": "opaque" }))).is_ok());
        assert!(parse(&base(serde_json::json!({ "background": "clear" }))).is_err());
    }

    #[test]
    fn transparent_background_validation_is_provider_aware() {
        // jpeg can't carry alpha — rejected regardless of provider.
        let err = parse(&base(
            serde_json::json!({ "background": "transparent", "format": "jpeg" }),
        ))
        .expect_err("jpeg alpha");
        assert!(err.message.contains("png or webp"), "{}", err.message);
        // gpt-image-2 documents NO transparent support — fail before spending.
        let err = parse(&base(serde_json::json!({
            "provider": "openai", "background": "transparent"
        })))
        .expect_err("gpt-image-2 transparent");
        assert!(err.message.contains("gpt-image-1.5"), "{}", err.message);
        // gpt-image-1.5 supports it (png default) — passes.
        assert!(
            parse(&base(serde_json::json!({
                "provider": "openai", "model": "gpt-image-1.5",
                "background": "transparent"
            })))
            .is_ok()
        );
        // gemini: not documented — clear error.
        assert!(
            parse(&base(serde_json::json!({
                "provider": "gemini", "background": "transparent"
            })))
            .is_err()
        );
        // mock accepts (plumbing tests).
        assert!(parse(&base(serde_json::json!({ "background": "transparent" }))).is_ok());
    }

    #[test]
    fn size_and_aspect_ratio_resolve_with_size_winning() {
        let p = parse(&base(serde_json::json!({ "size": "1536x800" }))).expect("valid");
        assert_eq!(
            p.size,
            SizeSpec::Exact {
                width: 1536,
                height: 800
            }
        );
        let p = parse(&base(serde_json::json!({ "aspect_ratio": "16:9" }))).expect("valid");
        assert_eq!(p.size, SizeSpec::Aspect(AspectRatio::Wide));
        let p = parse(&base(
            serde_json::json!({ "size": "1024x1024", "aspect_ratio": "16:9" }),
        ))
        .expect("valid");
        assert_eq!(
            p.size,
            SizeSpec::Exact {
                width: 1024,
                height: 1024
            },
            "size wins"
        );
        assert!(
            p.warnings.iter().any(|w| w.starts_with("size_conflict:")),
            "{:?}",
            p.warnings
        );
        // `size: auto` defers to the ratio.
        let p = parse(&base(
            serde_json::json!({ "size": "auto", "aspect_ratio": "9:16" }),
        ))
        .expect("valid");
        assert_eq!(p.size, SizeSpec::Aspect(AspectRatio::Tall));
        for bad in ["1024", "0x100", "10000x10000", "axb", "1024x-5"] {
            assert!(
                parse(&base(serde_json::json!({ "size": bad }))).is_err(),
                "{bad} rejected"
            );
        }
        assert!(parse(&base(serde_json::json!({ "aspect_ratio": "5:4" }))).is_err());
    }

    #[test]
    fn compression_range_and_png_warning() {
        let p = parse(&base(
            serde_json::json!({ "format": "webp", "compression": 80 }),
        ))
        .expect("valid");
        assert_eq!(p.compression, Some(80));
        assert!(parse(&base(serde_json::json!({ "compression": 101 }))).is_err());
        assert!(parse(&base(serde_json::json!({ "compression": -1 }))).is_err());
        // png + compression → warning, value dropped.
        let p = parse(&base(serde_json::json!({ "compression": 50 }))).expect("valid");
        assert_eq!(p.compression, None);
        assert!(
            p.warnings
                .iter()
                .any(|w| w.starts_with("compression_ignored:")),
            "{:?}",
            p.warnings
        );
    }

    #[test]
    fn seed_warns_per_provider_capability() {
        let p = parse(&base(
            serde_json::json!({ "provider": "openai", "seed": 42 }),
        ))
        .expect("valid");
        assert_eq!(p.seed, None, "openai has no seed — dropped");
        assert!(
            p.warnings
                .iter()
                .any(|w| w.starts_with("seed_unsupported:"))
        );
        let p = parse(&base(
            serde_json::json!({ "provider": "gemini", "seed": 42 }),
        ))
        .expect("valid");
        assert_eq!(p.seed, Some(42), "gemini passes the seed");
        assert!(
            p.warnings
                .iter()
                .any(|w| w.starts_with("seed_best_effort:"))
        );
        let p = parse(&base(serde_json::json!({ "seed": 42 }))).expect("valid");
        assert_eq!(p.seed, Some(42), "mock is deterministic by design");
        assert!(p.warnings.is_empty());
    }

    #[test]
    fn provider_options_known_keys_validated_unknown_warned() {
        let err = parse(&base(serde_json::json!({
            "provider": "openai", "provider_options": { "moderation": "off" }
        })))
        .expect_err("bad moderation");
        assert!(err.message.contains("auto"), "{}", err.message);
        let err = parse(&base(serde_json::json!({
            "provider": "gemini", "provider_options": { "image_size": "1k" }
        })))
        .expect_err("lowercase k");
        assert!(err.message.contains("uppercase"), "{}", err.message);
        let err = parse(&base(serde_json::json!({
            "provider": "gemini", "provider_options": { "thinking_level": "max" }
        })))
        .expect_err("bad level");
        assert!(err.message.contains("minimal"), "{}", err.message);
        let p = parse(&base(serde_json::json!({
            "provider": "gemini",
            "provider_options": { "thinking_level": "high", "future_knob": 3 }
        })))
        .expect("valid");
        assert!(
            p.warnings
                .iter()
                .any(|w| w.contains("future_knob") && w.starts_with("provider_option_unknown:")),
            "{:?}",
            p.warnings
        );
        assert!(parse(&base(serde_json::json!({ "provider_options": [] }))).is_err());
    }

    #[test]
    fn timeout_bounds_and_default() {
        let p = parse(&base(serde_json::json!({ "timeout_ms": 300_000 }))).expect("valid");
        assert_eq!(p.timeout, Duration::from_millis(300_000));
        assert!(parse(&base(serde_json::json!({ "timeout_ms": 999 }))).is_err());
        assert!(parse(&base(serde_json::json!({ "timeout_ms": 600_001 }))).is_err());
        assert!(parse(&base(serde_json::json!({ "timeout_ms": "3s" }))).is_err());
    }

    #[test]
    fn gemini_model_charset_is_confined() {
        // The model rides the URL path — path metacharacters are refused.
        for bad in ["a/b", "a:b", "a b", "a%2Fb", "../x"] {
            let err = parse(&base(serde_json::json!({
                "provider": "gemini", "model": bad
            })))
            .expect_err("charset");
            assert!(err.message.contains("charset"), "{}", err.message);
        }
        assert!(
            parse(&base(serde_json::json!({
                "provider": "gemini", "model": "gemini-3.1-flash-image"
            })))
            .is_ok()
        );
    }

    #[test]
    fn preview_gemini_model_warns() {
        let p = parse(&base(serde_json::json!({
            "provider": "gemini", "model": "gemini-4-image-preview"
        })))
        .expect("valid");
        assert!(p.warnings.iter().any(|w| w.starts_with("preview_model:")));
    }

    #[test]
    fn openai_prompt_cap_is_enforced() {
        let long = "x".repeat(32_001);
        let err = parse(&base(serde_json::json!({
            "provider": "openai", "prompt": long
        })))
        .expect_err("cap");
        assert!(err.message.contains("32000"), "{}", err.message);
        // …but the same prompt is fine on mock (no documented cap).
        let long = "x".repeat(32_001);
        assert!(parse(&base(serde_json::json!({ "prompt": long }))).is_ok());
    }

    #[test]
    fn metadata_rides_through_verbatim() {
        let p = parse(&base(serde_json::json!({
            "metadata": { "campaign": "spring", "page_slug": "qr-menu", "locale": "fr" }
        })))
        .expect("valid");
        assert_eq!(
            p.metadata.get("campaign"),
            Some(&serde_json::json!("spring"))
        );
        assert!(parse(&base(serde_json::json!({ "metadata": "x" }))).is_err());
    }
}
