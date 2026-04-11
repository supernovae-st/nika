// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Pure retry/backoff helpers for the `fetch:` verb.
//!
//! Extracted from `nika-engine/src/runtime/executor/fetch.rs` in S14-β,
//! refactored in S15-A0 to satisfy invariant #23 (kernel-adjacent helpers
//! use std / primitive / `bytes::Bytes` types only).
//!
//! These helpers have zero production coupling to `reqwest`. The engine
//! bridge re-imports them via `use nika_verb_fetch::retry::*` and passes
//! `Option<&str>` extracted from `response.headers().get(RETRY_AFTER)`.
//!
//! The retry loop _orchestration_ (which consumes these helpers) still
//! lives in the engine bridge. Extracting it is S15-A6 territory.

/// Maximum backoff delay: 5 minutes (300,000 ms).
///
/// Both `safe_backoff_delay` and `parse_retry_after` cap at this value to
/// prevent a server-side `Retry-After: 99999999` or an exponential blow-up
/// from stalling a workflow indefinitely.
pub const MAX_BACKOFF_MS: u64 = 300_000;

/// Check if a content-type string indicates HTML.
///
/// Used by the `llm_txt` extract mode to detect soft-404 pages where the
/// server returns `200 OK` with an HTML error page instead of the requested
/// `/llms.txt` file.
pub fn is_html_content_type(ct: &str) -> bool {
    ct.to_ascii_lowercase().contains("text/html")
}

/// Safe exponential backoff that handles Infinity/NaN/overflow.
///
/// Returns a delay in milliseconds, guaranteed to be in `[1, MAX_BACKOFF_MS]`.
///
/// - Negative or zero multiplier → use `1.0` (no backoff growth, but safe).
/// - Exponent clamped to 30 before `powi()` to avoid `f64::INFINITY` on
///   pathological inputs.
/// - `NaN` / `Infinity` factors collapse to `MAX_BACKOFF_MS`.
/// - `base_ms == 0` still returns at least `1` (minimum guaranteed delay).
pub fn safe_backoff_delay(base_ms: u64, multiplier: f64, exp: u32) -> u64 {
    let safe_mult = if multiplier <= 0.0 { 1.0 } else { multiplier };
    let factor = safe_mult.powi(exp.min(30) as i32);
    if factor.is_infinite() || factor.is_nan() || factor > MAX_BACKOFF_MS as f64 {
        return MAX_BACKOFF_MS;
    }
    let raw = base_ms.saturating_mul(factor as u64);
    raw.clamp(1, MAX_BACKOFF_MS)
}

/// Parse the `Retry-After` header value from a 429 or 503 response.
///
/// Takes `Option<&str>` — the caller extracts the raw header value from
/// `response.headers().get(RETRY_AFTER).and_then(|v| v.to_str().ok())`
/// and passes it in. Keeps this module reqwest-free (invariant #23).
///
/// Supports delay-seconds format per RFC 7231 §7.1.3:
/// - `"120"` → `Some(120_000)` (ms)
///
/// HTTP-date format is intentionally not supported (uncommon for LLM APIs
/// and adds a `chrono` dependency that is not worth the surface).
///
/// Returns `None` if the value is missing, unparseable, or zero.
/// Caps at `MAX_BACKOFF_MS` to prevent servers from stalling a workflow
/// indefinitely.
pub fn parse_retry_after(header_value: Option<&str>) -> Option<u64> {
    let value = header_value?;
    let secs = value.trim().parse::<u64>().ok()?;
    if secs == 0 {
        return None;
    }
    Some(secs.saturating_mul(1000).min(MAX_BACKOFF_MS))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_retry_after ───────────────────────────────────────────────
    //
    // Tests exercise the primitive `Option<&str>` surface AND build a
    // real `reqwest::HeaderMap` fixture to verify the caller's extraction
    // pattern (`headers.get(RETRY_AFTER).and_then(|v| v.to_str().ok())`)
    // produces the correct input. reqwest is `[dev-dependencies]` only.

    fn retry_after_from_headers(
        headers: &reqwest::header::HeaderMap,
    ) -> Option<&str> {
        headers
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
    }

    #[test]
    fn parse_retry_after_integer_seconds() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(reqwest::header::RETRY_AFTER, "30".parse().unwrap());
        assert_eq!(parse_retry_after(retry_after_from_headers(&headers)), Some(30_000));
    }

    #[test]
    fn parse_retry_after_missing_header() {
        let headers = reqwest::header::HeaderMap::new();
        assert_eq!(parse_retry_after(retry_after_from_headers(&headers)), None);
    }

    #[test]
    fn parse_retry_after_missing_none_input() {
        // Direct Option<&str>::None path — no HeaderMap needed
        assert_eq!(parse_retry_after(None), None);
    }

    #[test]
    fn parse_retry_after_zero_returns_none() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(reqwest::header::RETRY_AFTER, "0".parse().unwrap());
        assert_eq!(parse_retry_after(retry_after_from_headers(&headers)), None);
    }

    #[test]
    fn parse_retry_after_caps_at_5_minutes() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(reqwest::header::RETRY_AFTER, "600".parse().unwrap());
        assert_eq!(parse_retry_after(retry_after_from_headers(&headers)), Some(300_000));
    }

    #[test]
    fn parse_retry_after_non_numeric_returns_none() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::RETRY_AFTER,
            "Fri, 31 Dec 2099 23:59:59 GMT".parse().unwrap(),
        );
        assert_eq!(parse_retry_after(retry_after_from_headers(&headers)), None);
    }

    #[test]
    fn parse_retry_after_whitespace_trimmed() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(reqwest::header::RETRY_AFTER, " 5 ".parse().unwrap());
        assert_eq!(parse_retry_after(retry_after_from_headers(&headers)), Some(5_000));
    }

    #[test]
    fn parse_retry_after_primitive_string() {
        // Direct primitive path — no reqwest involvement
        assert_eq!(parse_retry_after(Some("42")), Some(42_000));
        assert_eq!(parse_retry_after(Some("0")), None);
        assert_eq!(parse_retry_after(Some(" 7 ")), Some(7_000));
        assert_eq!(parse_retry_after(Some("not-a-number")), None);
        assert_eq!(parse_retry_after(Some("")), None);
    }

    // ── safe_backoff_delay ──────────────────────────────────────────────

    #[test]
    fn backoff_large_exponent_does_not_produce_zero() {
        for exp in 0..=30u32 {
            let delay = safe_backoff_delay(100, 2.5, exp);
            assert!(delay > 0, "delay must never be 0 at exp={exp}");
            assert!(
                delay <= MAX_BACKOFF_MS,
                "delay must be capped at {MAX_BACKOFF_MS}, got {delay} at exp={exp}"
            );
        }
    }

    #[test]
    fn backoff_infinity_capped_at_max() {
        // 10^30 overflows f64 → Infinity → (as u64) == 0 in Rust
        let delay = safe_backoff_delay(100, 10.0, 30);
        assert_eq!(delay, MAX_BACKOFF_MS, "Infinity should be capped");
    }

    #[test]
    fn backoff_normal_case() {
        // 100 * 2.0^2 = 400
        let delay = safe_backoff_delay(100, 2.0, 2);
        assert_eq!(delay, 400);
    }

    #[test]
    fn backoff_zero_exponent() {
        // 100 * 2.0^0 = 100
        let delay = safe_backoff_delay(100, 2.0, 0);
        assert_eq!(delay, 100);
    }

    #[test]
    fn backoff_negative_multiplier_uses_one() {
        // Negative multiplier should not produce 0ms or negative delays
        let delay = safe_backoff_delay(100, -1.0, 3);
        assert_eq!(delay, 100, "negative multiplier should fall back to 1.0");

        let delay = safe_backoff_delay(100, 0.0, 3);
        assert_eq!(delay, 100, "zero multiplier should fall back to 1.0");

        let delay = safe_backoff_delay(100, -2.5, 5);
        assert_eq!(
            delay, 100,
            "negative multiplier should always produce base_ms"
        );
    }

    #[test]
    fn backoff_base_zero_returns_one() {
        // base_ms=0: 0 * factor = 0, clamped to 1 (minimum guarantee)
        let delay = safe_backoff_delay(0, 2.0, 3);
        assert_eq!(delay, 1, "base_ms=0 should clamp result to 1");

        let delay = safe_backoff_delay(0, 1.0, 0);
        assert_eq!(delay, 1, "base_ms=0 with identity factor should still be 1");

        // When factor exceeds MAX_BACKOFF_MS (100^10 > 300_000), the function
        // returns MAX_BACKOFF_MS before the base_ms multiplication happens.
        let delay = safe_backoff_delay(0, 100.0, 10);
        assert_eq!(
            delay, MAX_BACKOFF_MS,
            "huge factor triggers early MAX_BACKOFF_MS return regardless of base_ms"
        );

        // With a normal factor (2^3=8), base_ms=0 produces 0*8=0, clamped to 1
        let delay = safe_backoff_delay(0, 2.0, 0);
        assert_eq!(delay, 1, "base_ms=0 with factor=1 gives 0, clamped to 1");
    }

    #[test]
    fn backoff_nan_multiplier_returns_max() {
        // NaN comparison: NaN <= 0.0 is false, so safe_mult = NaN
        // NaN.powi(n) = NaN, which triggers the is_nan() guard → MAX_BACKOFF_MS
        let delay = safe_backoff_delay(100, f64::NAN, 1);
        assert_eq!(
            delay, MAX_BACKOFF_MS,
            "NaN multiplier should return MAX_BACKOFF_MS"
        );

        let delay = safe_backoff_delay(100, f64::NAN, 0);
        // IEEE 754: NaN^0 = 1.0 (special case)
        let expected = if f64::NAN.powi(0).is_nan() {
            MAX_BACKOFF_MS
        } else {
            100
        };
        assert_eq!(delay, expected, "NaN^0 follows IEEE 754 rules");
    }

    #[test]
    fn backoff_very_small_multiplier_returns_at_least_one() {
        // 0.1^30 ≈ 1e-30, which truncates to 0 as u64
        let delay = safe_backoff_delay(100, 0.1, 30);
        assert_eq!(delay, 1, "tiny factor should clamp to 1");

        let delay = safe_backoff_delay(1, 0.01, 20);
        assert_eq!(
            delay, 1,
            "very small multiplier with high exp should return 1"
        );

        // 0.5^10 = 0.000976..., as u64 = 0, 100 * 0 = 0, clamped to 1
        let delay = safe_backoff_delay(100, 0.5, 10);
        assert_eq!(delay, 1, "0.5^10 truncates to 0, result clamped to 1");
    }

    // ── is_html_content_type ────────────────────────────────────────────

    #[test]
    fn is_html_content_type_detects_html() {
        assert!(is_html_content_type("text/html"));
        assert!(is_html_content_type("text/html; charset=utf-8"));
        assert!(is_html_content_type("TEXT/HTML"));
    }

    #[test]
    fn is_html_content_type_allows_plaintext() {
        assert!(!is_html_content_type("text/plain"));
        assert!(!is_html_content_type("text/markdown"));
        assert!(!is_html_content_type("application/json"));
    }
}
