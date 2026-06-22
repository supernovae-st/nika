// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Retry configuration and error classification.
//!
//! `ErrorCategory` drives retry/alerting WITHOUT parsing NIKA-XXX string prefixes.
//! Pattern: Tonic `Status::code()`.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Retry configuration for operations.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
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

    /// The delay to honor before retry number `attempt`, when the server
    /// may have sent an explicit overload signal.
    ///
    /// Per the 2026-06-12 primary-source pass (the metastable-failures
    /// line · Bronson 2021 `HotOS` DOI 10.1145/3458336.3465286 · Huang
    /// 2022 OSDI): **an explicit `Retry-After` (429/503) BEATS the
    /// computed backoff** — the server is telling clients exactly when
    /// capacity returns, and retrying earlier is the amplification
    /// behavior that sustains metastable outages. The hint is honored
    /// VERBATIM (not capped at `backoff_max_ms`: a 60 s `Retry-After`
    /// with a 30 s cap must wait 60 s — coming back early hammers the
    /// recovering server). Callers whose overall deadline is shorter
    /// than the hint should give up instead (that decision belongs to
    /// the deadline owner, not the delay primitive).
    #[must_use]
    pub fn delay_with_hint_ms(
        &self,
        attempt: u32,
        rand_unit: f64,
        retry_after_ms: Option<u64>,
    ) -> u64 {
        retry_after_ms.unwrap_or_else(|| self.delay_for_ms(attempt, rand_unit))
    }
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self::new(3)
    }
}

/// Client-side retry budget — the token bucket that prevents retry storms.
///
/// The 2026-06-12 primary-source pass is unambiguous: **jitter alone does
/// not prevent retry storms**. In the studied production incidents
/// (Huang et al. · *Metastable Failures in the Wild* · OSDI 2022) retries
/// are the dominant sustaining effect — the load amplification that keeps
/// a system down after the trigger is gone (arXiv 2309.16181 ·
/// 2510.03551 · 2511.23278 `RetryGuard`). The mitigation every source
/// converges on: cap retries to a small fraction of request volume.
///
/// This is the **count-based** token bucket (`gRFC` A6 client-retry
/// semantics · the `gRPC` production design): no clock, no window — pure
/// state, which keeps it L0 (deterministic replay · property-testable).
/// The time-windowed alternative (Finagle's `RetryBudget` · 10 s sliding
/// window) needs a clock and belongs to an L1+ wrapper if ever needed.
///
/// Mechanics (integer millitokens · zero float drift):
/// - starts FULL at `max_tokens`
/// - [`Self::record_failure`] withdraws 1 token (floor 0)
/// - [`Self::record_success`] deposits `token_ratio` tokens (cap `max_tokens`)
/// - [`Self::retry_allowed`] ⇔ balance is strictly above `max_tokens / 2`
///
/// Under sustained failure the bucket drains in ~`max_tokens / 2`
/// failures and stays empty until real successes refill it — retry
/// amplification during an outage converges to `token_ratio` (the
/// recommended 10–20 % of request volume) instead of `max_attempts ×`.
///
/// One budget per upstream (per provider · per host), shared across that
/// upstream's calls. Wrap in the caller's lock at L1+ (this type is
/// deliberately `&mut self` · single-writer).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub struct RetryBudget {
    /// Bucket capacity in millitokens (`max_tokens × 1000`).
    max_millitokens: u64,
    /// Current balance in millitokens.
    millitokens: u64,
    /// Deposit per success in millitokens (`token_ratio × 1000`).
    deposit_millitokens: u64,
}

impl RetryBudget {
    /// Millitokens per token (integer arithmetic · no float drift).
    const MILLI: u64 = 1000;

    /// Create a budget with `max_tokens` capacity and `token_ratio`
    /// deposit per success.
    ///
    /// `token_ratio` is clamped to `0.0..=1.0` (per `gRFC` A6 the ratio is
    /// a fraction of request volume · values above 1 would let a single
    /// success fund more than one retry). `max_tokens == 0` produces a
    /// budget that never allows retries (fail-closed).
    #[must_use]
    pub fn new(max_tokens: u32, token_ratio: f32) -> Self {
        let max_millitokens = u64::from(max_tokens) * Self::MILLI;
        let ratio = f64::from(token_ratio).clamp(0.0, 1.0);
        // ratio ∈ [0,1] · MILLI = 1000 → product ∈ [0,1000] · exact in f64.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        #[allow(clippy::cast_precision_loss)]
        let deposit_millitokens = (ratio * Self::MILLI as f64).round() as u64;
        Self {
            max_millitokens,
            millitokens: max_millitokens,
            deposit_millitokens,
        }
    }

    /// The `gRFC` A6 defaults — `max_tokens = 10` · `token_ratio = 0.1`.
    #[must_use]
    pub fn grpc_default() -> Self {
        Self::new(10, 0.1)
    }

    /// Record a failed attempt (original OR retry) — withdraws one token.
    pub fn record_failure(&mut self) {
        self.millitokens = self.millitokens.saturating_sub(Self::MILLI);
    }

    /// Record a successful attempt — deposits `token_ratio` tokens.
    pub fn record_success(&mut self) {
        self.millitokens = self
            .millitokens
            .saturating_add(self.deposit_millitokens)
            .min(self.max_millitokens);
    }

    /// Whether a retry may proceed right now (balance strictly above half
    /// capacity · `gRFC` A6 gate). A `max_tokens == 0` budget always
    /// refuses.
    #[must_use]
    pub fn retry_allowed(&self) -> bool {
        self.millitokens > self.max_millitokens / 2
    }

    /// Current balance in whole tokens (rounded down · observability).
    #[must_use]
    pub fn tokens(&self) -> u32 {
        u32::try_from(self.millitokens / Self::MILLI).unwrap_or(u32::MAX)
    }
}

impl Default for RetryBudget {
    fn default() -> Self {
        Self::grpc_default()
    }
}

/// Classification of errors for retry decisions.
///
/// Drives retry/alerting WITHOUT parsing NIKA-XXX string prefixes.
/// Pattern: Tonic `Status::code()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
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

#[cfg(test)]
mod budget_tests {
    use super::*;

    #[test]
    fn fresh_budget_allows_and_hint_beats_computed() {
        let b = RetryBudget::grpc_default();
        assert!(b.retry_allowed());
        assert_eq!(b.tokens(), 10);
        // Retry-After honored VERBATIM — above the cap included
        let c = RetryConfig::default();
        assert_eq!(c.delay_with_hint_ms(2, 0.5, Some(60_000)), 60_000);
        assert_eq!(c.delay_with_hint_ms(2, 0.5, Some(0)), 0);
        let mut no_jitter = RetryConfig::new(3);
        no_jitter.jitter_ratio = 0.0;
        assert_eq!(no_jitter.delay_with_hint_ms(2, 0.0, None), 2000);
    }

    #[test]
    fn drains_to_blocked_at_half_and_recovers_on_success() {
        let mut b = RetryBudget::grpc_default(); // 10 tokens · gate at >5
        for _ in 0..4 {
            b.record_failure();
        }
        assert!(b.retry_allowed(), "6 > 5 still allowed");
        b.record_failure(); // balance 5 == half → blocked (strict >)
        assert!(!b.retry_allowed(), "5 is NOT > 5");
        // recovery · 0.1/success → 10 successes = +1 token
        for _ in 0..10 {
            b.record_success();
        }
        assert!(b.retry_allowed(), "6 > 5 after refill");
    }

    #[test]
    fn floor_zero_cap_max_and_zero_capacity_fail_closed() {
        let mut b = RetryBudget::new(2, 1.0);
        for _ in 0..50 {
            b.record_failure();
        }
        assert_eq!(b.tokens(), 0, "floored");
        for _ in 0..50 {
            b.record_success();
        }
        assert_eq!(b.tokens(), 2, "capped at max");
        let z = RetryBudget::new(0, 0.5);
        assert!(!z.retry_allowed(), "zero-capacity refuses forever");
    }

    #[test]
    fn ratio_clamped_and_serde_roundtrip() {
        let mut hot = RetryBudget::new(4, 9.5); // clamped to 1.0
        hot.record_failure();
        hot.record_success();
        assert_eq!(hot.tokens(), 4, "one success refunds at most one token");
        let b = RetryBudget::new(3, -2.0); // clamped to 0.0 · deposits nothing
        let json = serde_json::to_string(&b).expect("serialize");
        let back: RetryBudget = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(b, back);
    }

    /// E2E STORM SIMULATION — the anti-metastable property itself.
    ///
    /// Scenario from the research (Huang OSDI 2022 · arXiv 2511.23278):
    /// a sustained outage where EVERY attempt fails. Naive policy retries
    /// every failure (`max_attempts` = 3 → 3× load amplification — the
    /// sustaining effect that keeps the upstream down). The budget must
    /// collapse amplification to ~1× after the bucket drains.
    #[test]
    fn storm_simulation_budget_caps_amplification() {
        const REQUESTS: u64 = 10_000;
        let attempts_per_request = 3u64; // max_attempts naive ceiling

        // WITHOUT budget · every failure retries up to the ceiling.
        let naive_attempts = REQUESTS * attempts_per_request;

        // WITH budget · same storm · retry only when the bucket allows.
        let mut b = RetryBudget::grpc_default();
        let mut budget_attempts = 0u64;
        for _ in 0..REQUESTS {
            budget_attempts += 1; // the original attempt (always sent)
            b.record_failure();
            for _ in 1..attempts_per_request {
                if !b.retry_allowed() {
                    break;
                }
                budget_attempts += 1; // the retry
                b.record_failure();
            }
        }
        // The bucket (10 tokens · gate at >5) funds a handful of retries
        // then pins amplification at 1× for the rest of the outage.
        let max_tolerated = REQUESTS + 10;
        assert!(
            budget_attempts <= max_tolerated,
            "budget failed to cap the storm: {budget_attempts} attempts \
             (naive would be {naive_attempts})"
        );
        assert!(!b.retry_allowed(), "bucket drained during sustained outage");

        // RECOVERY · upstream heals · successes refill · retries resume.
        for _ in 0..60 {
            b.record_success();
        }
        assert!(b.retry_allowed(), "budget recovers with real successes");
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(512))]

        /// Balance NEVER leaves [0, max] and never panics, for ANY op
        /// sequence and ANY construction parameters.
        #[test]
        fn prop_budget_bounded(
            max in 0u32..10_000,
            ratio in -5.0f32..5.0,
            ops in proptest::collection::vec(proptest::bool::ANY, 0..200),
        ) {
            let mut b = RetryBudget::new(max, ratio);
            for op in ops {
                if op { b.record_success() } else { b.record_failure() }
                proptest::prop_assert!(b.tokens() <= max);
            }
        }

        /// The gate is EXACTLY « balance > half capacity » — verified
        /// against an independent integer model replaying the same ops.
        #[test]
        fn prop_gate_matches_integer_model(
            max in 1u32..1_000,
            ops in proptest::collection::vec(proptest::bool::ANY, 0..300),
        ) {
            let mut b = RetryBudget::new(max, 1.0); // deposit = 1 token exact
            let max_milli = u64::from(max) * 1000;
            let mut model: u64 = max_milli;
            for op in ops {
                if op {
                    b.record_success();
                    model = (model + 1000).min(max_milli);
                } else {
                    b.record_failure();
                    model = model.saturating_sub(1000);
                }
                proptest::prop_assert_eq!(b.retry_allowed(), model > max_milli / 2);
            }
        }

        /// Sustained-failure amplification is bounded by capacity: from a
        /// FULL bucket, at most ⌈max/2⌉ retries are ever granted before
        /// the gate closes (no deposits during a pure outage).
        #[test]
        fn prop_outage_retries_bounded_by_half_capacity(max in 1u32..1_000) {
            let mut b = RetryBudget::new(max, 0.1);
            let mut granted = 0u64;
            for _ in 0..(u64::from(max) * 3) {
                if b.retry_allowed() {
                    granted += 1;
                    b.record_failure();
                } else {
                    b.record_failure();
                }
            }
            proptest::prop_assert!(
                granted <= u64::from(max).div_ceil(2),
                "granted {granted} > half capacity {}",
                u64::from(max).div_ceil(2)
            );
        }
    }
}
