// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika:tts_generate` argument parsing — closed enums · loud refusals ·
//! the image family's honesty rules (never silently drop a knob).

use std::time::Duration;

use crate::{Args, BuiltinFailure};

use super::types::{AudioFormat, C_ARGS, Provider};

/// Cloud synthesis is seconds-class; local CPU synthesis can take longer.
const DEFAULT_TIMEOUT_MS: u64 = 120_000;
const DEFAULT_LOCAL_TIMEOUT_MS: u64 = 300_000;
const MAX_TIMEOUT_MS: u64 = 600_000;
/// The strictest documented per-request input cap (`OpenAI` 4096 chars) —
/// one cap for every provider keeps the contract portable.
const MAX_TEXT_CHARS: usize = 4_096;

#[derive(Debug)]
pub(crate) struct TtsArgs {
    pub(crate) provider: Provider,
    pub(crate) model: String,
    pub(crate) text: String,
    pub(crate) voice: String,
    /// `None` = provider-native container (openai/elevenlabs/local mp3 ·
    /// mock wav) — an explicit ask is honored where the wire allows and
    /// warned as `format_mismatch:` where the bytes say otherwise.
    pub(crate) format: Option<AudioFormat>,
    /// 0.25–4.0 · stored ×1000 (`speed_permille`) so `TtsArgs` stays Eq-able.
    speed_permille: u32,
    pub(crate) speed_explicit: bool,
    pub(crate) output_dir: String,
    pub(crate) filename_prefix: Option<String>,
    pub(crate) manifest: bool,
    pub(crate) metadata: serde_json::Map<String, serde_json::Value>,
    pub(crate) timeout: Duration,
    pub(crate) warnings: Vec<String>,
}

impl TtsArgs {
    pub(crate) const fn speed_permille(&self) -> u32 {
        self.speed_permille
    }

    /// The wire value (1.0-centred float) when explicitly set.
    pub(crate) fn speed_f64(&self) -> f64 {
        f64::from(self.speed_permille) / 1000.0
    }
}

pub(crate) fn parse(args: &Args) -> Result<TtsArgs, BuiltinFailure> {
    let mut warnings = Vec::new();

    let text = args
        .get("text")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .ok_or_else(|| BuiltinFailure::new(C_ARGS, "`text` (non-empty string) is required"))?
        .to_owned();
    if text.chars().count() > MAX_TEXT_CHARS {
        return Err(BuiltinFailure::new(
            C_ARGS,
            format!(
                "`text` is {} chars — the portable per-request cap is {MAX_TEXT_CHARS} \
                 (split long scripts into tasks; a `for_each` over paragraphs fans out cleanly)",
                text.chars().count()
            ),
        ));
    }

    let provider = parse_provider(args)?;
    let model = opt_str(args, "model")?.unwrap_or_else(|| provider.default_model().to_owned());
    let voice = opt_str(args, "voice")?.unwrap_or_else(|| provider.default_voice().to_owned());

    let format = match opt_str(args, "format")?.as_deref() {
        None | Some("auto") => None,
        Some("mp3") => Some(AudioFormat::Mp3),
        Some("wav") => Some(AudioFormat::Wav),
        Some(other) => {
            return Err(BuiltinFailure::new(
                C_ARGS,
                format!("unknown `format:` `{other}` — one of mp3 · wav · auto"),
            ));
        }
    };
    if format == Some(AudioFormat::Mp3) && provider == Provider::Mock {
        warnings.push(
            "format_mismatch: the mock renders WAV only — the request rides, the bytes decide"
                .to_owned(),
        );
    }

    let (speed_permille, speed_explicit) = match args.get("speed") {
        None => (1000, false),
        Some(v) => {
            let s = v.as_f64().ok_or_else(|| {
                BuiltinFailure::new(C_ARGS, "`speed` must be a number (0.25–4.0)")
            })?;
            if !(0.25..=4.0).contains(&s) {
                return Err(BuiltinFailure::new(
                    C_ARGS,
                    format!("`speed` {s} out of range — 0.25–4.0"),
                ));
            }
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // 250..=4000 by the range check
            ((s * 1000.0).round() as u32, true)
        }
    };
    if speed_explicit && provider == Provider::Elevenlabs {
        warnings.push(
            "speed_unsupported: elevenlabs has no speed knob on this wire — dropped (voice \
             settings govern pace)"
                .to_owned(),
        );
    }

    let output_dir = opt_str(args, "output_dir")?
        .ok_or_else(|| BuiltinFailure::new(C_ARGS, "`output_dir` is required"))?;

    let timeout_ms = parse_timeout(args, provider)?;
    let (metadata, manifest) = parse_provenance_knobs(args)?;

    Ok(TtsArgs {
        provider,
        model,
        text,
        voice,
        format,
        speed_permille,
        speed_explicit,
        output_dir,
        filename_prefix: opt_str(args, "filename_prefix")?,
        manifest,
        metadata,
        timeout: Duration::from_millis(timeout_ms),
        warnings,
    })
}

/// `timeout_ms` with the provider-aware default (local CPU synthesis gets
/// the wide one).
fn parse_timeout(args: &Args, provider: Provider) -> Result<u64, BuiltinFailure> {
    let default = if provider == Provider::Local {
        DEFAULT_LOCAL_TIMEOUT_MS
    } else {
        DEFAULT_TIMEOUT_MS
    };
    match args.get("timeout_ms") {
        None => Ok(default),
        Some(v) => v
            .as_u64()
            .filter(|ms| (1_000..=MAX_TIMEOUT_MS).contains(ms))
            .ok_or_else(|| {
                BuiltinFailure::new(
                    C_ARGS,
                    format!(
                        "`timeout_ms` must be 1000..={MAX_TIMEOUT_MS} — under 1s is a typo'd \
                         unit (the image family's floor, applied uniformly)"
                    ),
                )
            }),
    }
}

/// `metadata` + `manifest` (the provenance knobs).
fn parse_provenance_knobs(
    args: &Args,
) -> Result<(serde_json::Map<String, serde_json::Value>, bool), BuiltinFailure> {
    let metadata = match args.get("metadata") {
        None => serde_json::Map::new(),
        Some(serde_json::Value::Object(map)) => map.clone(),
        Some(_) => {
            return Err(BuiltinFailure::new(C_ARGS, "`metadata` must be an object"));
        }
    };
    let manifest = crate::strict_bool(args, "manifest", true, C_ARGS)?;
    Ok((metadata, manifest))
}

fn parse_provider(args: &Args) -> Result<Provider, BuiltinFailure> {
    match args.get("provider").and_then(serde_json::Value::as_str) {
        Some("local") => Ok(Provider::Local),
        Some("openai") => Ok(Provider::Openai),
        Some("elevenlabs") => Ok(Provider::Elevenlabs),
        Some("mock") => Ok(Provider::Mock),
        Some(other) => Err(BuiltinFailure::new(
            C_ARGS,
            format!(
                "unknown `provider:` `{other}` — one of local · openai · elevenlabs · mock \
                 (local is never inferred: model names are server-specific)"
            ),
        )),
        // Model-prefix inference mirrors the image family; `local` stays
        // explicit-only.
        None => match args.get("model").and_then(serde_json::Value::as_str) {
            Some(m) if m.starts_with("eleven") => Ok(Provider::Elevenlabs),
            Some(m) if m.starts_with("gpt-") || m.starts_with("tts-") => Ok(Provider::Openai),
            Some(m) if m.starts_with("mock") => Ok(Provider::Mock),
            Some(other) => Err(BuiltinFailure::new(
                C_ARGS,
                format!(
                    "cannot infer a provider from model `{other}` — set `provider:` \
                     (local · openai · elevenlabs · mock)"
                ),
            )),
            None => Err(BuiltinFailure::new(
                C_ARGS,
                "`provider:` is required (local · openai · elevenlabs · mock)",
            )),
        },
    }
}

fn opt_str(args: &Args, key: &str) -> Result<Option<String>, BuiltinFailure> {
    match args.get(key) {
        None => Ok(None),
        Some(serde_json::Value::String(s)) if !s.trim().is_empty() => Ok(Some(s.trim().to_owned())),
        Some(_) => Err(BuiltinFailure::new(
            C_ARGS,
            format!("`{key}` must be a non-empty string"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base(extra: serde_json::Value) -> Args {
        let serde_json::Value::Object(mut map) = serde_json::json!({
            "provider": "mock", "text": "bonjour", "output_dir": "./out"
        }) else {
            unreachable!()
        };
        if let serde_json::Value::Object(e) = extra {
            map.extend(e);
        }
        map
    }

    #[test]
    fn parse_covers_the_contract_edges() {
        let parsed_mock = parse(&base(serde_json::json!({}))).expect("valid");
        assert_eq!(parsed_mock.provider, Provider::Mock);
        assert_eq!(parsed_mock.voice, "sine");
        assert_eq!(parsed_mock.speed_permille(), 1000);
        assert!(parsed_mock.manifest);
        let mut m = base(serde_json::json!({ "model": "eleven_flash_v2_5" }));
        m.remove("provider");
        assert_eq!(parse(&m).expect("inferred").provider, Provider::Elevenlabs);
        let mut m = base(serde_json::json!({ "model": "gpt-4o-mini-tts" }));
        m.remove("provider");
        assert_eq!(parse(&m).expect("inferred").provider, Provider::Openai);
        let mut m = base(serde_json::json!({ "model": "kokoro" }));
        m.remove("provider");
        assert!(parse(&m).is_err(), "local never inferred");

        // caps + ranges.
        assert!(parse(&base(serde_json::json!({ "text": "x".repeat(5000) }))).is_err());
        assert!(parse(&base(serde_json::json!({ "speed": 9.0 }))).is_err());
        assert!(parse(&base(serde_json::json!({ "format": "flac" }))).is_err());
        assert!(
            parse(&base(serde_json::json!({ "timeout_ms": 5 }))).is_err(),
            "sub-second timeouts are typo'd units (image-family floor parity)"
        );
        let s = parse(&base(serde_json::json!({ "speed": 1.5 }))).expect("speed");
        assert_eq!(s.speed_permille(), 1500);
        assert!((s.speed_f64() - 1.5).abs() < 1e-9);

        // elevenlabs speed warn-drop.
        let e = parse(&base(serde_json::json!({
            "provider": "elevenlabs", "speed": 2.0
        })))
        .expect("valid");
        assert!(
            e.warnings
                .iter()
                .any(|w| w.starts_with("speed_unsupported:"))
        );

        // local default timeout is the CPU-wide one.
        let l = parse(&base(serde_json::json!({ "provider": "local" }))).expect("valid");
        assert_eq!(l.timeout.as_millis(), 300_000);
    }
}
