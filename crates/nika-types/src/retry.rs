// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Retry configuration and error classification.
//!
//! `ErrorCategory` drives retry/alerting WITHOUT parsing NIKA-XXX string prefixes.
//! Pattern: Tonic `Status::code()`.

use serde::{Deserialize, Serialize};

/// Retry configuration for operations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct RetryConfig {
    /// Maximum number of attempts (including the initial one).
    pub max_attempts: u32,
    /// Base delay for exponential backoff in milliseconds.
    pub backoff_base_ms: u64,
    /// Maximum backoff delay in milliseconds.
    pub backoff_max_ms: u64,
    /// Jitter ratio (0.0 = no jitter, 1.0 = full jitter).
    pub jitter_ratio: f32,
}

impl RetryConfig {
    /// Create a new retry config — capped exponential backoff with **full
    /// jitter** (`jitter_ratio = 1.0`), the strategy the simulation
    /// evidence ranks top for collision avoidance under contention
    /// (Brooker · AWS Architecture Blog 2015 · adopted by the AWS SDKs;
    /// flipped from 0.25 on 2026-06-12 while the field had ZERO delay
    /// consumers — the cheapest moment to land the canonical default).
    ///
    /// Jitter alone does not prevent retry storms: the metastable-failures
    /// line (Bronson 2021 `HotOS` DOI 10.1145/3458336.3465286 · Huang 2022
    /// OSDI · arXiv 2309.16181 · 2510.03551 · 2511.23278 `RetryGuard`)
    /// shows retry amplification converts transient spikes into permanent
    /// outages. Consumers MUST pair this config with (1) bounded attempts
    /// (`max_attempts` · already here), (2) a retry budget / circuit
    /// breaker at the call site, (3) honoring explicit overload signals
    /// (429/503 `Retry-After` BEATS the computed delay) — and SHOULD
    /// jitter the first attempt of cron-shaped fan-outs.
    #[must_use]
    pub fn new(max_attempts: u32) -> Self {
        Self {
            max_attempts,
            backoff_base_ms: 1000,
            backoff_max_ms: 30_000,
            jitter_ratio: 1.0,
        }
    }

    /// No retries (single attempt).
    #[must_use]
    pub fn none() -> Self {
        Self {
            max_attempts: 1,
            backoff_base_ms: 0,
            backoff_max_ms: 0,
            jitter_ratio: 0.0,
        }
    }

    /// The backoff delay before retry number `attempt` (1-based: `1` =
    /// the delay before the FIRST retry), in milliseconds.
    ///
    /// THE single shared implementation of truncated exponential backoff
    /// with proportional jitter — every retry loop (http · providers ·
    /// exec-runner · agent) consumes this instead of hand-rolling one
    /// (2026-06-12 · the config carried `jitter_ratio` with no delay
    /// semantics anywhere in the workspace).
    ///
    /// ```text
    /// exp(attempt) = min(backoff_max_ms, backoff_base_ms · 2^(attempt-1))
    /// delay        = exp·(1 − jitter_ratio) + rand_unit · exp·jitter_ratio
    /// ```
    ///
    /// `rand_unit` is a caller-supplied uniform sample in `[0, 1)` — this
    /// crate is L0 (no entropy source · same discipline as caller-supplied
    /// timestamps in `nika-event`), so randomness is injected and the
    /// function stays PURE (deterministic replay · property-testable).
    ///
    /// The `jitter_ratio` spectrum recovers the canonical strategies
    /// (Brooker · AWS Architecture Blog 2015 · *Exponential Backoff and
    /// Jitter*): `1.0` = **full jitter** `[0, exp)` — the default · best
    /// collision avoidance for thundering herds · `0.0` = deterministic
    /// truncated exponential. The cap is load-bearing: uncapped binary
    /// exponential backoff loses constant throughput under bursty
    /// arrivals (Bender et al. · *How to Scale Exponential Backoff* ·
    /// arXiv 1402.5207 · JACM 2019 DOI 10.1145/3276769) — saturating at
    /// `backoff_max_ms` is the windowed/saturating family that restores
    /// robustness. Decorrelated jitter (`min(cap, U(base, 3·prev))`) is
    /// stateful and intentionally NOT offered here — this function stays
    /// pure · a consumer wanting it derives it from this primitive.
    ///
    /// Out-of-range inputs degrade safely: `rand_unit` and `jitter_ratio`
    /// are clamped to `[0, 1]` · `attempt == 0` is treated as `1` ·
    /// arithmetic saturates (no overflow at any `attempt`).
    #[must_use]
    pub fn delay_for_ms(&self, attempt: u32, rand_unit: f64) -> u64 {
        if self.backoff_base_ms == 0 {
            return 0;
        }
        let exponent = attempt.max(1) - 1;
        // u128 intermediate · 2^63+ shifts and huge bases saturate at max.
        let exp_u128 = if exponent >= 64 {
            u128::from(self.backoff_max_ms)
        } else {
            (u128::from(self.backoff_base_ms) << exponent).min(u128::from(self.backoff_max_ms))
        };
        #[allow(clippy::cast_possible_truncation)]
        let exp = exp_u128 as u64; // ≤ backoff_max_ms · fits
        let jr = f64::from(self.jitter_ratio).clamp(0.0, 1.0);
        let r = rand_unit.clamp(0.0, 1.0);
        #[allow(clippy::cast_precision_loss)]
        let exp_f = exp as f64;
        let delay = exp_f * (1.0 - jr) + r * exp_f * jr;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let out = delay as u64;
        out.min(self.backoff_max_ms)
    }
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self::new(3)
    }
}

/// Classification of errors for retry decisions.
///
/// Drives retry/alerting WITHOUT parsing NIKA-XXX string prefixes.
/// Pattern: Tonic `Status::code()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ErrorCategory {
    /// Transient — may succeed on retry (network timeout, 503).
    Transient,
    /// Permanent — will never succeed (400, schema error).
    Permanent,
    /// User fault — bad input, fix the request.
    UserFault,
    /// Provider fault — provider-side issue.
    ProviderFault,
    /// Internal — engine bug.
    Internal,
    /// Rate limited — retry after delay.
    RateLimit,
    /// Budget exceeded — stop, don't retry.
    Budget,
    /// Policy violation — blocked by rules.
    Policy,
}

impl ErrorCategory {
    /// Whether this category is retryable.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Transient | Self::RateLimit | Self::ProviderFault
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_retry_config() {
        let config = RetryConfig::default();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.backoff_base_ms, 1000);
        assert_eq!(config.backoff_max_ms, 30_000);
        // full jitter IS the canonical default (Brooker 2015 · flipped
        // 2026-06-12 pre-consumer · a regression to partial jitter here
        // re-correlates concurrent retriers silently)
        assert!((config.jitter_ratio - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn no_retry_config() {
        let config = RetryConfig::none();
        assert_eq!(config.max_attempts, 1);
    }

    #[test]
    fn retryable_categories() {
        assert!(ErrorCategory::Transient.is_retryable());
        assert!(ErrorCategory::RateLimit.is_retryable());
        assert!(ErrorCategory::ProviderFault.is_retryable());
        assert!(!ErrorCategory::Permanent.is_retryable());
        assert!(!ErrorCategory::UserFault.is_retryable());
        assert!(!ErrorCategory::Budget.is_retryable());
        assert!(!ErrorCategory::Policy.is_retryable());
        assert!(!ErrorCategory::Internal.is_retryable());
    }

    #[test]
    fn retry_config_serde_roundtrip() {
        let config = RetryConfig::new(5);
        let json = serde_json::to_string(&config).expect("serialize");
        let back: RetryConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(config, back);
    }

    #[test]
    fn error_category_serde_roundtrip() {
        let cat = ErrorCategory::RateLimit;
        let json = serde_json::to_string(&cat).expect("serialize");
        assert_eq!(json, "\"rate_limit\"");
        let back: ErrorCategory = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(cat, back);
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn retry_types_are_send_sync() {
        _assert_send_sync::<RetryConfig>();
        _assert_send_sync::<ErrorCategory>();
    }

    // ── delay_for_ms · the shared backoff semantics ──

    #[test]
    fn delay_deterministic_when_no_jitter() {
        let mut c = RetryConfig::new(5);
        c.jitter_ratio = 0.0;
        // base 1000 · doubling · capped at 30_000
        assert_eq!(c.delay_for_ms(1, 0.5), 1000);
        assert_eq!(c.delay_for_ms(2, 0.5), 2000);
        assert_eq!(c.delay_for_ms(3, 0.5), 4000);
        assert_eq!(c.delay_for_ms(6, 0.5), 30_000); // 32_000 capped
    }

    #[test]
    fn delay_full_jitter_spans_zero_to_exp() {
        let mut c = RetryConfig::new(5);
        c.jitter_ratio = 1.0;
        assert_eq!(c.delay_for_ms(2, 0.0), 0); // full jitter floor
        assert_eq!(c.delay_for_ms(2, 0.999_999), 1999); // just under exp
    }

    #[test]
    fn delay_zero_base_is_zero_everywhere() {
        let c = RetryConfig::none();
        assert_eq!(c.delay_for_ms(1, 0.9), 0);
        assert_eq!(c.delay_for_ms(40, 0.9), 0);
    }

    #[test]
    fn delay_attempt_zero_treated_as_one() {
        let mut c = RetryConfig::new(3);
        c.jitter_ratio = 0.0;
        assert_eq!(c.delay_for_ms(0, 0.5), c.delay_for_ms(1, 0.5));
    }

    #[test]
    fn delay_out_of_range_inputs_clamped() {
        let mut c = RetryConfig::new(3);
        c.jitter_ratio = 7.5; // clamped to 1.0
        let d = c.delay_for_ms(2, 9.9); // rand clamped to 1.0
        assert!(d <= 2000);
        let d2 = c.delay_for_ms(2, -3.0); // clamped to 0.0 → full-jitter floor
        assert_eq!(d2, 0);
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(512))]

        /// The delay is ALWAYS within [exp·(1−jr), exp] and never exceeds
        /// the cap — for every attempt, base, cap, ratio and sample. No
        /// overflow panic at any attempt (saturating shift path).
        #[test]
        fn prop_delay_bounded(
            attempt in 0u32..200,
            base in 0u64..10_000_000,
            max in 0u64..100_000_000,
            jr in 0.0f32..=1.0,
            r in 0.0f64..1.0,
        ) {
            let c = RetryConfig { max_attempts: 3, backoff_base_ms: base, backoff_max_ms: max, jitter_ratio: jr };
            let d = c.delay_for_ms(attempt, r);
            proptest::prop_assert!(d <= max, "delay {d} exceeds cap {max}");
            if base > 0 {
                let exponent = attempt.max(1) - 1;
                let exp = if exponent >= 64 { u128::from(max) } else { (u128::from(base) << exponent).min(u128::from(max)) };
                #[allow(clippy::cast_possible_truncation)]
                let exp = exp as u64;
                #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let floor = (exp as f64 * f64::from(1.0 - jr.clamp(0.0, 1.0))).floor() as u64;
                proptest::prop_assert!(d <= exp, "delay {d} above exp {exp}");
                proptest::prop_assert!(d + 1 >= floor.min(exp), "delay {d} below jitter floor {floor}");
            }
        }

        /// Monotone in attempt when jitter is off: a later retry never
        /// waits LESS (until the cap flattens it).
        #[test]
        fn prop_delay_monotone_no_jitter(
            attempt in 1u32..60,
            base in 1u64..1_000_000,
            max in 1u64..1_000_000_000,
        ) {
            let c = RetryConfig { max_attempts: 3, backoff_base_ms: base, backoff_max_ms: max, jitter_ratio: 0.0 };
            let d1 = c.delay_for_ms(attempt, 0.0);
            let d2 = c.delay_for_ms(attempt + 1, 0.0);
            proptest::prop_assert!(d2 >= d1, "delay shrank: {d1} -> {d2}");
        }
    }
}
