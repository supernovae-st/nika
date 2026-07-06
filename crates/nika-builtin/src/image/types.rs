// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Shared vocabulary of the `nika:image_generate` module family — the
//! closed enums the arg parser produces and every provider adapter
//! consumes, plus the spec error codes (`NIKA-BUILTIN-IMAGE_GENERATE-*`,
//! stdlib §Media · taxonomy-owned by `spec/05-errors.md`).

/// 001 — invalid arguments (bad enum value · out-of-range · V1-unsupported
/// option like `mode: edit` / `save: false` / `reference_images`).
pub(crate) const C_ARGS: &str = "NIKA-BUILTIN-IMAGE_GENERATE-001";
/// 002 — provider unavailable (missing API key · image plane not wired).
pub(crate) const C_PROVIDER_UNAVAILABLE: &str = "NIKA-BUILTIN-IMAGE_GENERATE-002";
/// 003 — provider request failed (transport · HTTP status · rate limit).
/// Transient per the spec status table (5xx/408/429 + timeout/connection).
pub(crate) const C_REQUEST: &str = "NIKA-BUILTIN-IMAGE_GENERATE-003";
/// 004 — the provider returned no image / a malformed image payload.
pub(crate) const C_NO_IMAGE: &str = "NIKA-BUILTIN-IMAGE_GENERATE-004";
/// 005 — content policy block (moderation · safety finish reasons).
pub(crate) const C_POLICY: &str = "NIKA-BUILTIN-IMAGE_GENERATE-005";
/// 006 — saving an image or the manifest failed.
pub(crate) const C_SAVE: &str = "NIKA-BUILTIN-IMAGE_GENERATE-006";
/// 007 — decoded image failed validation (magic bytes · dimensions · size).
pub(crate) const C_VALIDATE: &str = "NIKA-BUILTIN-IMAGE_GENERATE-007";

/// The generation mode — `generate` (text→image · default) or `edit`
/// (input image(s) + instruction → image · reads are permit-gated, the
/// mirror of the save boundary).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    Generate,
    Edit,
}

impl Mode {
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Generate => "generate",
            Self::Edit => "edit",
        }
    }
}

/// One input image read from disk for `mode: edit` — the bytes + the
/// sniffed format + the path/sha for the provenance chain.
///
/// M-A stores the identity (path · sha · format) that the mock edit and
/// the manifest provenance chain need; the wire adapters (M-A.2) will add
/// the byte payload they data-URL-encode. Growing the struct then is
/// additive and behavior-free.
#[derive(Debug, Clone)]
pub(crate) struct InputImage {
    pub(crate) path: String,
    pub(crate) format: ImageFormat,
    pub(crate) sha256: String,
}

/// The v1.1 image providers — a closed set (local + xai joined 2026-07-05
/// per the Rule-3 sovereignty review · mock for offline CI/docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Provider {
    /// A user-run LOCAL `OpenAI`-images-compatible server (`LocalAI` · Ollama
    /// · sd.cpp server · SGLang/vLLM-Omni — one wire covers them all). The
    /// sovereign path: the base URL is ENGINE CONFIG, never workflow data.
    Local,
    /// `OpenAI` Images API (`POST /v1/images/generations`).
    Openai,
    /// Google Gemini image output (`POST …/models/{model}:generateContent`).
    Gemini,
    /// xAI Imagine API (`POST /v1/images/generations` · `aspect_ratio` +
    /// `resolution` classes instead of exact sizes).
    Xai,
    /// Deterministic in-process mock — zero network, zero key.
    Mock,
}

impl Provider {
    /// The canonical lowercase id (output + filenames + manifest).
    pub(crate) const fn id(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Openai => "openai",
            Self::Gemini => "gemini",
            Self::Xai => "xai",
            Self::Mock => "mock",
        }
    }

    /// The provider's default model (research-verified 2026-07 — the only
    /// ids with no scheduled shutdown; overridable via `model:`).
    pub(crate) const fn default_model(self) -> &'static str {
        match self {
            // The LocalAI custom-YAML convention (the default base URL is
            // LocalAI's :8080 — a zero-config pair). Other compat servers
            // (Ollama · sd.cpp · SGLang) name models differently: set
            // `model:` explicitly alongside your base URL.
            Self::Local => "stablediffusion",
            Self::Openai => "gpt-image-2",
            Self::Gemini => "gemini-3.1-flash-image",
            // The workhorse tier ($0.02/image) — the `-quality` tier is
            // the `model:` knob (grok-imagine-image-quality).
            Self::Xai => "grok-imagine-image",
            Self::Mock => "mock-image-1",
        }
    }
}

/// Output image formats (closed · `format:` arg · default png).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImageFormat {
    /// Lossless · supports transparency.
    Png,
    /// Lossy · `compression:` applies.
    Jpeg,
    /// Lossy/lossless · `compression:` applies · supports transparency.
    Webp,
}

impl ImageFormat {
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpeg",
            Self::Webp => "webp",
        }
    }

    pub(crate) const fn mime(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Webp => "image/webp",
        }
    }

    /// The file extension (jpeg → the conventional `.jpg`).
    pub(crate) const fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Webp => "webp",
        }
    }
}

/// `quality:` — the common ladder; each adapter maps it (openai native
/// `auto|low|medium|high` · `ultra` folds to `high` with a warning ·
/// gemini has no quality knob → model-tier warning).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Quality {
    Auto,
    Low,
    Medium,
    High,
    Ultra,
}

impl Quality {
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Ultra => "ultra",
        }
    }
}

/// `background:` — transparent requires png/webp (validated at parse) and
/// a provider/model that supports it (gpt-image-2 rejects it · research-
/// verified · caught at parse so no request is spent).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Background {
    Auto,
    Transparent,
    Opaque,
}

impl Background {
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Transparent => "transparent",
            Self::Opaque => "opaque",
        }
    }
}

/// `aspect_ratio:` — the closed common set (all 8 are native gemini
/// ratios; openai maps each to an exact gpt-image-2 `WxH`, /16-aligned).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AspectRatio {
    Square,       // 1:1
    Wide,         // 16:9
    Tall,         // 9:16
    Standard,     // 4:3
    StandardFlip, // 3:4
    Photo,        // 3:2
    PhotoFlip,    // 2:3
    Cinematic,    // 21:9
}

impl AspectRatio {
    /// The canonical `W:H` string (gemini-native form).
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Square => "1:1",
            Self::Wide => "16:9",
            Self::Tall => "9:16",
            Self::Standard => "4:3",
            Self::StandardFlip => "3:4",
            Self::Photo => "3:2",
            Self::PhotoFlip => "2:3",
            Self::Cinematic => "21:9",
        }
    }

    /// Every accepted `aspect_ratio:` value (parse + hint surfaces).
    pub(crate) const ALL: [Self; 8] = [
        Self::Square,
        Self::Wide,
        Self::Tall,
        Self::Standard,
        Self::StandardFlip,
        Self::Photo,
        Self::PhotoFlip,
        Self::Cinematic,
    ];

    /// The exact gpt-image-2 `WxH` this ratio maps to — every pair is
    /// /16-aligned, within the 1:3–3:1 ratio window and the documented
    /// pixel budget (655,360 ≤ w·h ≤ 8,294,400 · edge ≤ 3840).
    pub(crate) const fn openai_size(self) -> (u32, u32) {
        match self {
            Self::Square => (1024, 1024),
            Self::Wide => (1536, 864),
            Self::Tall => (864, 1536),
            Self::Standard => (1152, 864),
            Self::StandardFlip => (864, 1152),
            Self::Photo => (1536, 1024),
            Self::PhotoFlip => (1024, 1536),
            Self::Cinematic => (2016, 864),
        }
    }
}

/// The resolved size request (post `size:`-vs-`aspect_ratio:` conflict
/// resolution — `size:` wins, with a warning).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SizeSpec {
    /// Neither given — the provider default (openai `auto` · gemini 1:1).
    Auto,
    /// An exact `WxH` from `size:`.
    Exact { width: u32, height: u32 },
    /// A ratio from `aspect_ratio:` (no exact size requested).
    Aspect(AspectRatio),
}

/// Normalized token usage across providers (absent axes stay `None` —
/// never zero-that-looks-real).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[allow(clippy::struct_field_names)] // the wire vocabulary — spec-normative axis names
pub(crate) struct Usage {
    pub(crate) input_tokens: Option<u64>,
    pub(crate) output_tokens: Option<u64>,
    pub(crate) total_tokens: Option<u64>,
    /// Gemini thinking tokens (billed) — openai has no equivalent.
    pub(crate) thoughts_tokens: Option<u64>,
}

impl Usage {
    /// Merge another call's usage into this one (gemini `n:` = n calls).
    /// Saturating: the counters are provider-controlled numbers — a
    /// hostile/broken body carrying `u64::MAX` must corrupt a statistic,
    /// never panic (debug) or wrap (release).
    pub(crate) fn accumulate(&mut self, other: Self) {
        for (into, from) in [
            (&mut self.input_tokens, other.input_tokens),
            (&mut self.output_tokens, other.output_tokens),
            (&mut self.total_tokens, other.total_tokens),
            (&mut self.thoughts_tokens, other.thoughts_tokens),
        ] {
            if let Some(value) = from {
                *into = Some(into.unwrap_or(0).saturating_add(value));
            }
        }
    }

    /// The output JSON object (`null` for axes the provider never reported).
    pub(crate) fn to_json(self) -> serde_json::Value {
        serde_json::json!({
            "input_tokens": self.input_tokens,
            "output_tokens": self.output_tokens,
            "total_tokens": self.total_tokens,
            "thoughts_tokens": self.thoughts_tokens,
        })
    }
}

/// One decoded provider image, pre-save (bytes + provenance).
#[derive(Debug)]
pub(crate) struct RawImage {
    /// The decoded (post-base64) image bytes.
    pub(crate) bytes: Vec<u8>,
    /// The provider's revised prompt for THIS image, when reported.
    pub(crate) revised_prompt: Option<String>,
    /// The seed that produced it, when the provider echoes/accepts one.
    pub(crate) seed: Option<i64>,
}

/// What a provider adapter returns for one `image_generate` call.
#[derive(Debug, Default)]
pub(crate) struct ProviderBatch {
    pub(crate) images: Vec<RawImage>,
    pub(crate) usage: Usage,
    /// REAL spend in USD when the provider reports it exactly (xai bills
    /// `cost_in_usd_ticks` in the response body) — `None` = unpriced,
    /// honest (never an estimate that looks real).
    pub(crate) cost_usd: Option<f64>,
    /// The host the render actually came from (provenance — matters once
    /// the `local` provider makes the endpoint engine-configurable).
    /// `None` for the in-process mock.
    pub(crate) endpoint_host: Option<String>,
    /// Interleaved text parts (gemini) — surfaced as `provider_text`.
    pub(crate) provider_text: Option<String>,
    /// Adapter-level warnings (e.g. « gemini returned 2 images for 1 »).
    pub(crate) warnings: Vec<String>,
    /// The sanitized raw provider response (`debug: true` only — base64
    /// payloads replaced by `<N base64 chars>` placeholders).
    pub(crate) raw_debug: Option<serde_json::Value>,
}

impl ProviderBatch {
    /// A batch must carry at least one image (the adapters enforce this
    /// with their own 004/005 mapping before returning).
    pub(crate) fn is_empty(&self) -> bool {
        self.images.is_empty()
    }
}

/// Strip oversized payloads out of a raw provider response so the
/// `debug: true` echo stays log-safe: any LONG string under a known
/// payload key (`b64_json` · `data`) becomes `"<N base64 chars omitted>"`,
/// and any OTHER oversized string (a base64-shaped `text` part · a huge
/// revised prompt) is clamped to a readable prefix — the debug echo is a
/// diagnostic, never a payload carrier. Headers never enter this value
/// (only the parsed BODY is echoed), so credentials cannot appear by
/// construction.
pub(crate) fn sanitize_raw(value: &mut serde_json::Value) {
    const PAYLOAD_KEYS: [&str; 2] = ["b64_json", "data"];
    const KEEP_CHARS: usize = 256;
    /// Non-payload strings clamp at a debug-friendly width.
    const CLAMP_CHARS: usize = 4_096;
    match value {
        serde_json::Value::Object(map) => {
            for (key, entry) in map.iter_mut() {
                if let serde_json::Value::String(text) = entry {
                    if PAYLOAD_KEYS.contains(&key.as_str()) && text.len() > KEEP_CHARS {
                        *entry = serde_json::Value::String(format!(
                            "<{} base64 chars omitted>",
                            text.len()
                        ));
                    } else if text.chars().count() > CLAMP_CHARS {
                        use std::fmt::Write as _;
                        let total = text.chars().count();
                        let mut clamped: String = text.chars().take(KEEP_CHARS).collect();
                        let _ = write!(clamped, " … <{} more chars clamped>", total - KEEP_CHARS);
                        *entry = serde_json::Value::String(clamped);
                    }
                    continue;
                }
                sanitize_raw(entry);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                sanitize_raw(item);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_ids_and_defaults_are_the_locked_canon() {
        assert_eq!(Provider::Local.id(), "local");
        assert_eq!(Provider::Local.default_model(), "stablediffusion");
        assert_eq!(Provider::Openai.id(), "openai");
        assert_eq!(Provider::Openai.default_model(), "gpt-image-2");
        assert_eq!(Provider::Gemini.id(), "gemini");
        assert_eq!(Provider::Gemini.default_model(), "gemini-3.1-flash-image");
        assert_eq!(Provider::Xai.id(), "xai");
        assert_eq!(Provider::Xai.default_model(), "grok-imagine-image");
        assert_eq!(Provider::Mock.id(), "mock");
    }

    #[test]
    fn format_mime_and_extension_pair_up() {
        assert_eq!(ImageFormat::Png.mime(), "image/png");
        assert_eq!(ImageFormat::Png.extension(), "png");
        assert_eq!(ImageFormat::Jpeg.mime(), "image/jpeg");
        assert_eq!(ImageFormat::Jpeg.extension(), "jpg", "conventional .jpg");
        assert_eq!(ImageFormat::Webp.mime(), "image/webp");
        assert_eq!(ImageFormat::Webp.extension(), "webp");
    }

    #[test]
    fn every_aspect_ratio_maps_to_a_valid_gpt_image_2_size() {
        // The documented gpt-image-2 constraints: both edges /16, ratio
        // within 1:3–3:1, 655,360 ≤ pixels ≤ 8,294,400, edge ≤ 3840.
        for ratio in AspectRatio::ALL {
            let (w, h) = ratio.openai_size();
            assert_eq!(w % 16, 0, "{}: width /16", ratio.name());
            assert_eq!(h % 16, 0, "{}: height /16", ratio.name());
            let pixels = u64::from(w) * u64::from(h);
            assert!(
                (655_360..=8_294_400).contains(&pixels),
                "{}: pixel budget ({pixels})",
                ratio.name()
            );
            assert!(w <= 3840 && h <= 3840, "{}: edge cap", ratio.name());
            let ratio_f = f64::from(w) / f64::from(h);
            assert!(
                (1.0 / 3.0..=3.0).contains(&ratio_f),
                "{}: ratio window",
                ratio.name()
            );
        }
    }

    #[test]
    fn usage_accumulation_saturates_on_hostile_counters() {
        let mut total = Usage {
            input_tokens: Some(1),
            output_tokens: None,
            total_tokens: Some(u64::MAX),
            thoughts_tokens: None,
        };
        total.accumulate(Usage {
            input_tokens: Some(u64::MAX),
            output_tokens: Some(u64::MAX),
            total_tokens: Some(u64::MAX),
            thoughts_tokens: None,
        });
        assert_eq!(total.input_tokens, Some(u64::MAX), "saturates, never wraps");
        assert_eq!(total.output_tokens, Some(u64::MAX));
        assert_eq!(total.total_tokens, Some(u64::MAX));
    }

    #[test]
    fn sanitize_raw_clamps_oversized_non_payload_strings() {
        // Payload keys keep the full-omission marker; any OTHER oversized
        // string (a text part · a revised prompt) clamps to a prefix.
        let mut value = serde_json::json!({
            "data": "P".repeat(500),
            "text": "T".repeat(10_000),
            "note": "short stays verbatim",
        });
        sanitize_raw(&mut value);
        assert_eq!(value["data"], "<500 base64 chars omitted>");
        let text = value["text"].as_str().expect("string");
        assert!(text.len() < 1_000, "clamped ({} bytes)", text.len());
        assert!(text.contains("more chars clamped"));
        assert_eq!(value["note"], "short stays verbatim");
    }

    #[test]
    fn usage_accumulates_across_calls_and_keeps_honest_nulls() {
        let mut total = Usage::default();
        assert_eq!(total.to_json()["input_tokens"], serde_json::Value::Null);
        total.accumulate(Usage {
            input_tokens: Some(10),
            output_tokens: Some(1120),
            total_tokens: Some(1130),
            thoughts_tokens: None,
        });
        total.accumulate(Usage {
            input_tokens: Some(5),
            output_tokens: Some(1120),
            total_tokens: Some(1125),
            thoughts_tokens: Some(300),
        });
        assert_eq!(total.input_tokens, Some(15));
        assert_eq!(total.output_tokens, Some(2240));
        assert_eq!(total.total_tokens, Some(2255));
        assert_eq!(total.thoughts_tokens, Some(300), "late axis still lands");
    }
}
