// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Generation-budget arithmetic — the pure, model-free bound on how many
//! tokens one request may generate.
//!
//! Kept OUT of `candle_backend` (which is `local-infer`-gated and only
//! exercised by the real-GGUF e2e) so the safety bound lives on the pure,
//! unit- + mutation-tested surface: a malformed wire `max_tokens` is proven
//! finite *here*, with no model in the loop. The candle backend calls
//! [`effective_max_tokens`] at the decode site; this module is the SSOT for
//! "how long may a generation run".

/// The number of tokens to actually generate for one request.
///
/// `requested` is the wire `max_tokens` (`None` ⇒ `default`). The result is
/// clamped to the slots LEFT in the context window
/// (`context_window - prompt_len`, saturating) — so generation can neither
///
/// - run away on a hostile/huge `max_tokens` (a wire `u32::MAX` would
///   otherwise become a ~4-billion-iteration loop → OOM + a core-pinning
///   spin that, under the serialized model `Mutex`, blocks every other
///   request including `/health`), nor
/// - exceed the window (generating past `context_window` desyncs the rotary
///   positions / KV-cache, the exact failure the prompt-length check guards).
///
/// `default` is the budget when the request is silent — NOT a ceiling on its
/// own (the ceiling is the context window, applied here unconditionally).
pub(crate) fn effective_max_tokens(
    requested: Option<u32>,
    default: u32,
    context_window: usize,
    prompt_len: usize,
) -> usize {
    let asked = requested.unwrap_or(default) as usize;
    let remaining = context_window.saturating_sub(prompt_len);
    asked.min(remaining)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_uses_the_default_when_context_allows() {
        assert_eq!(effective_max_tokens(None, 1024, 32_768, 100), 1024);
    }

    #[test]
    fn honors_a_modest_explicit_request() {
        assert_eq!(effective_max_tokens(Some(256), 1024, 32_768, 100), 256);
    }

    #[test]
    fn clamps_a_runaway_request_to_remaining_context() {
        // The P2-1 case: a wire `max_tokens: u32::MAX` must NOT become a
        // 4-billion-iteration loop — the context window bounds it.
        assert_eq!(
            effective_max_tokens(Some(u32::MAX), 1024, 32_768, 100),
            32_768 - 100
        );
    }

    #[test]
    fn clamps_even_the_default_when_context_is_nearly_full() {
        // Only 10 slots left → even the 1024 default is clamped down.
        assert_eq!(effective_max_tokens(None, 1024, 4_096, 4_086), 10);
    }

    #[test]
    fn saturates_to_zero_without_panic_when_prompt_fills_context() {
        // The caller guards this with a ContextOverflow error, but the helper
        // stays total: prompt_len >= context_window must not underflow-panic.
        assert_eq!(effective_max_tokens(Some(50), 1024, 4_096, 4_096), 0);
        assert_eq!(effective_max_tokens(Some(50), 1024, 4_096, 9_999), 0);
    }
}
