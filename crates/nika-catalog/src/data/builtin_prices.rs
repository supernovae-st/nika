// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Static per-call USD floors for priced builtins.
//!
//! `nika check`'s cost envelope prices `infer:`/`agent:` only — `invoke:`
//! is skipped as "spend nothing on inference". Priced media builtins still
//! bill (xAI Imagine ticks → `cost_usd`) *after* HTTP, and the run ledger
//! then aborts NIKA-1704. This table is the admission number: a floor
//! already over `--max-cost-usd` refuses before the executor (B24 / issue
//! 1296 · NIKA-1709). Unpriced providers (`mock` · `local` · openai/gemini
//! image, whose spend is token-metered and not a static per-call floor)
//! return `None`.

/// xAI `grok-imagine-image` workhorse — $0.02 per image (vendor tick:
/// 1 US cent = `1e8` ticks).
const IMAGE_GENERATE_XAI_USD: f64 = 0.02;

/// Per-call USD floor for a priced builtin + provider pair.
///
/// `tool` accepts `nika:image_generate` or `image_generate`. `None` when
/// the pair is unpriced (mock/local/unknown) or the tool has no static
/// floor. The caller multiplies by a known `n:` and a known `for_each`
/// count; templated args stay `None` here (the mid-run ledger owns them).
#[must_use]
pub fn builtin_provider_floor_usd(tool: &str, provider: &str) -> Option<f64> {
    let name = tool.strip_prefix("nika:").unwrap_or(tool);
    match (name, provider) {
        ("image_generate", "xai") => Some(IMAGE_GENERATE_XAI_USD),
        _ => None,
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
}
