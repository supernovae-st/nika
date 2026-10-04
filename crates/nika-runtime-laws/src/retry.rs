// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Retry backoff arithmetic (spec `05-errors.md` §retry) — the three
//! strategy ramps (`fixed` · `linear` · `exponential`) + **full jitter**
//! (Brooker · AWS Architecture Blog 2015 · *Exponential Backoff and
//! Jitter*) with a saturating cap (Bender et al. · JACM 2019 ·
//! DOI 10.1145/3276769 — uncapped backoff loses throughput under
//! bursty arrivals).
//!
//! The blend/clamp discipline mirrors `nika_types::retry::delay_for_ms`
//! (THE shared backoff semantics) extended with the two spec ramps the
//! schema's `BackoffStrategy` adds — this adapter graduates into
//! nika-types on a second consumer (stress-to-ratchet).
//!
//! Randomness is **derived, not sampled**: [`rand_unit`] hashes
//! `(seed, task, attempt)` through splitmix64 (Steele et al. ·
//! *Fast Splittable Pseudorandom Number Generators* · OOPSLA 2014 —
//! the canonical 64-bit finalizer family). Pure · `Sync` ·
//! replay-stable by construction (no RNG state · no logged-sleep
//! requirement) — the determinism contract of the event stream
//! extends to the retry delays for free.
//!
//! A law, not a decision: whether to retry, how many times, and the sleep
//! itself stay with the runtime (`nika-runtime`'s `retry` module), which
//! asks this one how long.

use nika_schema::types::{BackoffStrategy, RetryConfig};

/// The backoff delay before retry number `attempt` (1-based · `1` =
/// the delay before the FIRST retry), in milliseconds.
///
/// ```text
/// ramp(fixed)       = backoff_ms
/// ramp(linear)      = backoff_ms · attempt
/// ramp(exponential) = backoff_ms · 2^(attempt-1)
/// capped            = min(backoff_max_ms, ramp)
/// jitter: true      → full jitter · uniform [0, capped)   (Brooker 2015)
/// jitter: false     → capped (deterministic)
/// ```
///
/// Out-of-range inputs degrade safely: `attempt == 0` is treated as
/// `1` · arithmetic saturates (no overflow at any attempt) ·
/// `rand_unit` is clamped to `[0, 1]`.
#[must_use]
pub fn delay_ms(cfg: &RetryConfig, attempt: u32, rand_unit: f64) -> u64 {
    if cfg.backoff_ms == 0 {
        return 0;
    }
    let n = attempt.max(1);
    let ramp_u128 = match cfg.backoff_strategy {
        BackoffStrategy::Fixed => u128::from(cfg.backoff_ms),
        BackoffStrategy::Linear => u128::from(cfg.backoff_ms) * u128::from(n),
        BackoffStrategy::Exponential => {
            let exponent = n - 1;
            if exponent >= 64 {
                u128::from(cfg.backoff_max_ms)
            } else {
                u128::from(cfg.backoff_ms) << exponent
            }
        }
        // #[non_exhaustive] · a future strategy must land here loudly —
        // the saturating cap is the safe degradation (never a panic).
        _ => u128::from(cfg.backoff_max_ms),
    };
    let capped_u128 = ramp_u128.min(u128::from(cfg.backoff_max_ms));
    #[allow(clippy::cast_possible_truncation)] // ≤ backoff_max_ms · fits u64
    let capped = capped_u128 as u64;
    if !cfg.jitter {
        return capped;
    }
    let r = rand_unit.clamp(0.0, 1.0);
    #[allow(clippy::cast_precision_loss)] // ms scale · ≤2^53 in practice
    let capped_f = capped as f64;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let out = (r * capped_f) as u64; // full jitter · uniform [0, capped)
    out.min(capped)
}

/// splitmix64 finalizer (Steele et al. 2014) — the avalanche step.
const fn splitmix64(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// FNV-1a over the task id — a stable 64-bit stream selector.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// A uniform sample in `[0, 1)` derived from `(seed, task, attempt)` —
/// pure · replay-stable (see module docs).
#[must_use]
pub fn rand_unit(seed: u64, task_id: &str, attempt: u32) -> f64 {
    let mixed = splitmix64(seed ^ fnv1a(task_id.as_bytes()) ^ (u64::from(attempt) << 32));
    // 53 high bits → the f64 mantissa lattice · [0, 1).
    #[allow(clippy::cast_precision_loss)] // by construction ≤ 2^53 − 1
    let unit = (mixed >> 11) as f64 * (1.0 / 9_007_199_254_740_992.0);
    unit
}
