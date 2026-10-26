// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Static per-call USD floors for priced builtins, and the price class of
//! every paid-media call.
//!
//! `nika check`'s cost envelope prices `infer:`/`agent:` only — `invoke:`
//! is skipped as "spend nothing on inference". Priced media builtins still
//! bill (xAI Imagine ticks → `cost_usd`) *after* HTTP, and the run ledger
//! then aborts NIKA-1704. This table is the admission number: a floor
//! already over `--max-cost-usd` refuses before the executor (B24 / issue
//! 1296 · NIKA-1709).
//!
//! A paid media call the table does not price (openai/gemini images,
//! openai/elevenlabs speech: token- or character-billed, with no catalog
//! rate) is [`MediaPrice::Unknown`]: real spend a cap cannot bound, never
//! a free call. Local servers, the mock and keyless voices are
//! [`MediaPrice::Unmetered`].

/// xAI `grok-imagine-image` workhorse — $0.02 per image (vendor tick:
/// 1 US cent = `1e8` ticks).
const IMAGE_GENERATE_XAI_USD: f64 = 0.02;

/// The paid-media builtins: each call reaches a provider that may bill.
const PAID_MEDIA_TOOLS: [&str; 2] = ["image_generate", "tts_generate"];

/// Providers of those builtins that never bill: a user-run local server,
/// the mock, and a keyless voice synthesised on this machine.
const UNMETERED_MEDIA_PROVIDERS: [&str; 3] = ["local", "mock", "system"];

/// What the catalog knows about the price of one media builtin call.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum MediaPrice {
    /// No provider bill: a local server, the mock, a keyless voice, or a
    /// builtin that calls no paid provider. Never metered, never counted
    /// as paid.
    Unmetered,
    /// A paid provider call the catalog prices: USD per call (one image),
    /// before the call's `n:`.
    PerCall(f64),
    /// A paid provider call the catalog has no price for: the spend is
    /// real and unknown, so a cap cannot bound it.
    Unknown,
}

/// The price class of one media builtin call on `provider`.
///
/// `tool` accepts the `nika:` prefix or the bare name. A builtin other
/// than `image_generate` / `tts_generate` is [`MediaPrice::Unmetered`];
/// on those two, every provider outside the unmetered set (`local` ·
/// `mock` · `system`) without a catalog rate is [`MediaPrice::Unknown`],
/// so a new paid provider is unknown spend until the catalog prices it.
#[must_use]
pub fn builtin_media_price(tool: &str, provider: &str) -> MediaPrice {
    let name = tool.strip_prefix("nika:").unwrap_or(tool);
    if !PAID_MEDIA_TOOLS.contains(&name) || UNMETERED_MEDIA_PROVIDERS.contains(&provider) {
        return MediaPrice::Unmetered;
    }
    match (name, provider) {
        ("image_generate", "xai") => MediaPrice::PerCall(IMAGE_GENERATE_XAI_USD),
        _ => MediaPrice::Unknown,
    }
}

/// The provider a media builtin call bills, resolved as the builtin
/// resolves it: the explicit `provider:` wins, else the one its `model:`
/// prefix names (`gpt-image*` · `gemini*` · `grok*` · `mock*` for images;
/// `eleven*` · `gpt-*` · `tts-*` · `mock*` for speech). `None` for any
/// other builtin, or when neither key names a provider (the builtin then
/// refuses its arguments before any request).
#[must_use]
pub fn builtin_media_provider<'a>(
    tool: &str,
    provider: Option<&'a str>,
    model: Option<&str>,
) -> Option<&'a str> {
    let name = tool.strip_prefix("nika:").unwrap_or(tool);
    if !PAID_MEDIA_TOOLS.contains(&name) {
        return None;
    }
    if let Some(provider) = provider {
        return Some(provider);
    }
    let model = model?;
    let inferred = match name {
        "image_generate" if model.starts_with("gpt-image") => "openai",
        "image_generate" if model.starts_with("gemini") => "gemini",
        "image_generate" if model.starts_with("grok") => "xai",
        "tts_generate" if model.starts_with("eleven") => "elevenlabs",
        "tts_generate" if model.starts_with("gpt-") || model.starts_with("tts-") => "openai",
        _ if model.starts_with("mock") => "mock",
        _ => return None,
    };
    Some(inferred)
}

/// Per-call USD floor for a priced builtin + provider pair.
///
/// `tool` accepts `nika:image_generate` or `image_generate`. `None` when
/// the pair is unpriced (mock/local/unknown) or the tool has no static
/// floor. The caller multiplies by a known `n:` and a known `for_each`
/// count; templated args stay `None` here (the mid-run ledger owns them).
#[must_use]
pub fn builtin_provider_floor_usd(tool: &str, provider: &str) -> Option<f64> {
    match builtin_media_price(tool, provider) {
        MediaPrice::PerCall(usd) => Some(usd),
        MediaPrice::Unmetered | MediaPrice::Unknown => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xai_image_generate_is_two_cents() {
        for tool in ["image_generate", "nika:image_generate"] {
            let floor = builtin_provider_floor_usd(tool, "xai").expect("priced");
            assert!(
                (floor - 0.02).abs() < 1e-12,
                "xAI workhorse is $0.02/image, got {floor} for {tool}"
            );
        }
    }

    #[test]
    fn unpriced_image_providers_have_no_static_floor() {
        for provider in ["mock", "local", "openai", "gemini", ""] {
            assert_eq!(
                builtin_provider_floor_usd("image_generate", provider),
                None,
                "{provider} image is not a static per-call floor"
            );
        }
    }

    #[test]
    fn other_builtins_are_unpriced() {
        for tool in ["tts_generate", "fetch", "read", "jq"] {
            assert_eq!(
                builtin_provider_floor_usd(tool, "xai"),
                None,
                "{tool} has no static builtin floor"
            );
        }
    }

    #[test]
    fn paid_media_without_a_catalog_rate_is_unknown_spend() {
        for (tool, provider) in [
            ("nika:image_generate", "openai"),
            ("image_generate", "gemini"),
            ("nika:tts_generate", "openai"),
            ("tts_generate", "elevenlabs"),
            // A provider the catalog has never seen is unknown, never free.
            ("nika:image_generate", "newcloud"),
            ("nika:tts_generate", "xai"),
        ] {
            assert_eq!(
                builtin_media_price(tool, provider),
                MediaPrice::Unknown,
                "{tool} on {provider} bills with no catalog rate"
            );
        }
    }

    #[test]
    fn xai_images_are_priced_per_call() {
        assert_eq!(
            builtin_media_price("nika:image_generate", "xai"),
            MediaPrice::PerCall(0.02)
        );
    }

    #[test]
    fn local_mock_keyless_and_other_builtins_are_unmetered() {
        for tool in ["nika:image_generate", "nika:tts_generate"] {
            for provider in ["local", "mock", "system"] {
                assert_eq!(
                    builtin_media_price(tool, provider),
                    MediaPrice::Unmetered,
                    "{tool} on {provider} never bills"
                );
            }
        }
        for tool in [
            "nika:image_fx",
            "nika:chart",
            "nika:image_compose",
            "nika:fetch",
        ] {
            assert_eq!(
                builtin_media_price(tool, "openai"),
                MediaPrice::Unmetered,
                "{tool} calls no paid media provider"
            );
        }
    }

    #[test]
    fn the_media_provider_is_the_explicit_one_else_the_model_prefix() {
        let image = "nika:image_generate";
        let speech = "nika:tts_generate";
        assert_eq!(
            builtin_media_provider(image, Some("gemini"), Some("gpt-image-1")),
            Some("gemini"),
            "an explicit provider wins"
        );
        for (tool, model, provider) in [
            (image, "gpt-image-1-mini", Some("openai")),
            (image, "gemini-3.1-flash-image", Some("gemini")),
            (image, "grok-imagine-image", Some("xai")),
            (image, "mock-image", Some("mock")),
            (image, "dall-e-3", None),
            (speech, "eleven_multilingual_v2", Some("elevenlabs")),
            (speech, "gpt-4o-mini-tts", Some("openai")),
            (speech, "tts-1", Some("openai")),
            (speech, "mock-tts-1", Some("mock")),
            (speech, "kokoro", None),
        ] {
            assert_eq!(
                builtin_media_provider(tool, None, Some(model)),
                provider,
                "{tool} with model {model}"
            );
        }
        assert_eq!(builtin_media_provider(image, None, None), None);
        assert_eq!(
            builtin_media_provider("nika:image_fx", Some("openai"), None),
            None,
            "not a paid-media builtin"
        );
    }
}
