// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Pure logits / probability transforms — the sampling-time algorithms.
//!
//! These are the SOTA decode-time algorithms implemented natively on
//! `&mut [f32]`, the exact shape candle's `LogitsProcessor::sample_f`
//! closure receives (post-softmax probabilities) — and, for the
//! pre-softmax case, the logits vector before `sample`. Keeping them
//! here (candle-free) means they are unit/property-tested in the default
//! build and reusable by ANY backend.
//!
//! Insertion points (per the candle generation contract):
//!
//! - [`apply_repeat_penalty`] — PRE-softmax, on raw logits (the
//!   llama.cpp convention candle's `utils::apply_repeat_penalty` follows).
//! - [`apply_top_n_sigma`] — PRE-softmax (arXiv:2411.07641; verified vs the
//!   llama.cpp reference) — temperature-invariant noise truncation.
//! - [`apply_min_p`] — POST-softmax, on probabilities (arXiv:2407.01082;
//!   not in candle's `Sampling` enum — applied via `sample_f`).
//! - [`apply_token_mask`] — POST-softmax; the constrained-decoding
//!   primitive (an `llguidance`-computed mask zeroes disallowed tokens —
//!   the v2 guaranteed-structured-output hook).

/// min-p sampling (arXiv:2407.01082): keep only tokens whose probability
/// is at least `p_base × max_prob`; zero the rest. Dynamic truncation —
/// a better quality/diversity frontier than top-p, especially at higher
/// temperatures and on small/quantized models (noisier tails).
///
/// The max-probability token ALWAYS survives (`max ≥ p_base·max` for
/// `p_base ≤ 1`), so the distribution can never become empty. Survivors
/// are NOT renormalized — the weighted draw downstream normalizes
/// implicitly. `p_base ≤ 0` is a no-op; `p_base ≥ 1` keeps only the
/// argmax (and exact ties). NaN entries are treated as non-survivors.
///
/// Returns the number of surviving (non-zero) entries.
pub fn apply_min_p(probs: &mut [f32], p_base: f32) -> usize {
    if probs.is_empty() {
        return 0;
    }
    // ALWAYS zero NaN first — a surviving NaN poisons the downstream weighted
    // draw (undefined index). This runs on every path, including the early
    // returns below, so a NaN can never reach the sampler regardless of p_base.
    for p in probs.iter_mut() {
        if p.is_nan() {
            *p = 0.0;
        }
    }
    if p_base <= 0.0 {
        return probs.iter().filter(|p| **p > 0.0).count();
    }
    let max = probs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    if !max.is_finite() || max <= 0.0 {
        // Degenerate distribution (all zero) — nothing meaningful to
        // truncate; leave untouched and report what is there.
        return probs.iter().filter(|p| **p > 0.0).count();
    }
    let floor = p_base * max;
    let mut survivors = 0usize;
    for p in probs.iter_mut() {
        // `>=` keeps the max itself and exact ties; NaN fails the compare
        // (NaN >= x is false) and is zeroed — fail-closed on bad input.
        if *p >= floor {
            survivors += 1;
        } else {
            *p = 0.0;
        }
    }
    survivors
}

/// Repeat penalty (llama.cpp convention): for each token id seen in the
/// recent window, divide its logit by `penalty` when the logit is ≥ 0,
/// else multiply by it — both push the value DOWN, discouraging the
/// repeat. Operates on RAW LOGITS pre-softmax (mirrors candle's
/// `utils::apply_repeat_penalty`, reimplemented here so the crate's
/// algorithms live in one candle-free place).
///
/// `penalty <= 1.0` is a no-op (1.0 = off; a sub-1 value would AMPLIFY
/// repeats, never wanted). Out-of-range ids in `recent` are ignored
/// (defensive — a corrupt context can never index out of bounds).
pub fn apply_repeat_penalty(logits: &mut [f32], penalty: f32, recent: &[u32]) {
    // `penalty <= 1.0` is a no-op: 1.0 = off, and a value below 1 would
    // AMPLIFY repeats (never wanted) — so anything ≤1 (incl. ≤0) does nothing.
    if penalty <= 1.0 {
        return;
    }
    for &tok in recent {
        let Some(l) = logits.get_mut(tok as usize) else {
            continue;
        };
        *l = if *l >= 0.0 {
            *l / penalty
        } else {
            *l * penalty
        };
    }
}

/// Top-nσ sampling (arXiv:2411.07641 · "Not All Logits Are You Need"):
/// keep only tokens whose logit is within `n` standard deviations of the
/// maximum; the rest go to `-∞`. PRE-softmax, on raw logits (the llama.cpp
/// reference implementation's insertion point — PR #11223, verified).
///
/// The insight: logits split into a Gaussian noise floor and a distinct
/// informative region; `max − n·σ` separates them statistically. The killer
/// property vs top-p/min-p: the survivor SET is **temperature-invariant**
/// (dividing logits by T scales `max − x` and `σ` identically) — exact in
/// real arithmetic, so high-T sampling never re-admits noise tokens. In
/// f32 a token sitting AT the threshold can flip across scalings (a
/// rounding tie, not a semantic leak); the property test pins the law and
/// names that one legal tie band (seed of 2026-08-01).
///
/// `n <= 0` is a no-op (disabled · llama.cpp convention). The max token
/// always survives (`max ≥ max − n·σ`). A degenerate distribution (all
/// equal · σ = 0) keeps everything. NaN logits are sent to `-∞` first
/// (fail-closed — and they would otherwise poison mean/σ). Typical n: 1–5
/// (paper); returns the survivor count.
pub fn apply_top_n_sigma(logits: &mut [f32], n: f32) -> usize {
    if logits.is_empty() {
        return 0;
    }
    // Fail-closed NaN scrub BEFORE the statistics (a single NaN poisons
    // mean/σ and would void the threshold entirely).
    for l in logits.iter_mut() {
        if l.is_nan() {
            *l = f32::NEG_INFINITY;
        }
    }
    if n <= 0.0 {
        return logits.iter().filter(|l| l.is_finite()).count();
    }
    // Statistics over the FINITE entries only — earlier-masked (-∞) tokens
    // are out of the distribution, not data points.
    let mut max = f32::NEG_INFINITY;
    let mut sum = 0f64;
    let mut count = 0usize;
    for &l in logits.iter().filter(|l| l.is_finite()) {
        max = max.max(l);
        sum += f64::from(l);
        count += 1;
    }
    if count == 0 {
        return 0;
    }
    #[allow(clippy::cast_precision_loss)]
    let mean = sum / count as f64;
    let mut acc = 0f64;
    for &l in logits.iter().filter(|l| l.is_finite()) {
        let d = f64::from(l) - mean;
        acc += d * d;
    }
    #[allow(clippy::cast_precision_loss)]
    let std = (acc / count as f64).sqrt();
    #[allow(clippy::cast_possible_truncation)]
    let threshold = max - n * std as f32;

    let mut survivors = 0usize;
    for l in logits.iter_mut() {
        if *l >= threshold {
            survivors += 1;
        } else {
            *l = f32::NEG_INFINITY;
        }
    }
    survivors
}

/// Apply a token mask: zero the probability of every token whose mask bit
/// is `false` (disallowed). This is the constrained-decoding primitive —
/// an `llguidance`/`outlines`-computed grammar mask makes the next token
/// guaranteed-valid by construction (the v2 structured-output hook).
///
/// POST-softmax (operates on probabilities · applied in `sample_f`).
/// A `mask` shorter than `probs` zeroes the uncovered tail (fail-closed:
/// an under-specified mask forbids, never silently allows). Returns the
/// number of surviving (allowed AND non-zero) entries — `0` means the
/// grammar admits nothing here, which the caller MUST treat as an error
/// (forcing EOS or failing) rather than sampling from an empty set.
pub fn apply_token_mask(probs: &mut [f32], mask: &[bool]) -> usize {
    let mut survivors = 0usize;
    for (i, p) in probs.iter_mut().enumerate() {
        // Uncovered tail (i >= mask.len()) is treated as disallowed.
        let allowed = mask.get(i).copied().unwrap_or(false);
        if allowed && *p > 0.0 {
            survivors += 1;
        } else {
            *p = 0.0;
        }
    }
    survivors
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // exact, deterministic literals — not computed approximations
mod tests {
    use super::*;

    // ── min-p ────────────────────────────────────────────────────────

    #[test]
    fn min_p_keeps_max_and_zeroes_the_tail() {
        // max = 0.6, p_base 0.5 → floor 0.3; keep 0.6 and 0.3, drop 0.1.
        let mut p = [0.6, 0.3, 0.1];
        let n = apply_min_p(&mut p, 0.5);
        assert_eq!(n, 2);
        assert_eq!(p, [0.6, 0.3, 0.0]);
    }

    #[test]
    fn min_p_zero_or_negative_is_noop() {
        let mut p = [0.6, 0.3, 0.1];
        let n = apply_min_p(&mut p, 0.0);
        assert_eq!(n, 3);
        assert_eq!(p, [0.6, 0.3, 0.1], "p_base<=0 must not modify");
    }

    #[test]
    fn min_p_never_empties_the_distribution() {
        // Even p_base = 1.0 keeps the argmax (0.6 >= 1.0*0.6).
        let mut p = [0.6, 0.3, 0.1];
        let n = apply_min_p(&mut p, 1.0);
        assert!(n >= 1, "the max token must always survive");
        assert_eq!(p[0], 0.6);
    }

    #[test]
    fn min_p_handles_nan_without_poisoning_max() {
        let mut p = [f32::NAN, 0.5, 0.4];
        let n = apply_min_p(&mut p, 0.5); // max = 0.5, floor 0.25; keep 0.5, 0.4
        assert_eq!(n, 2);
        assert_eq!(p[0], 0.0, "NaN entry is zeroed (fail-closed)");
        assert_eq!(p[1], 0.5);
        assert_eq!(p[2], 0.4);
    }

    #[test]
    fn min_p_empty_slice_is_zero() {
        let mut p: [f32; 0] = [];
        assert_eq!(apply_min_p(&mut p, 0.5), 0);
    }

    #[test]
    fn min_p_survivor_counts_are_strictly_positive() {
        // Mutant killers: the `> 0.0` count predicates (both early paths) must
        // not count ZERO entries as survivors (`>=`/`==`/`<` all die here),
        // and the degenerate guard is `!finite OR max<=0` (the `||`→`&&`
        // mutant proceeds to the threshold loop and would report 2).
        let mut all_zero = [0.0f32, 0.0];
        assert_eq!(apply_min_p(&mut all_zero, 0.5), 0, "all-zero → 0 survivors");

        // A (defensive) negative entry still hits the degenerate path — and
        // pins the count predicate DIRECTION (`>` vs `<`): only strictly
        // positive entries are live, a negative one never is.
        let mut with_negative = [-1.0f32, 0.0];
        assert_eq!(
            apply_min_p(&mut with_negative, 0.5),
            0,
            "negative is not a survivor"
        );

        let mut disabled = [0.0f32, 1.0];
        assert_eq!(
            apply_min_p(&mut disabled, 0.0),
            1,
            "p_base=0 counts only >0"
        );
    }

    // ── repeat penalty ─────────────────────────────────────────────────

    #[test]
    fn repeat_penalty_lowers_seen_positive_logits() {
        let mut l = [2.0, 1.0, 0.5];
        apply_repeat_penalty(&mut l, 2.0, &[0]); // token 0 seen → 2.0/2 = 1.0
        assert_eq!(l, [1.0, 1.0, 0.5]);
    }

    #[test]
    fn repeat_penalty_pushes_negative_logits_more_negative() {
        let mut l = [-1.0, 0.0, 0.0];
        apply_repeat_penalty(&mut l, 2.0, &[0]); // -1.0 * 2 = -2.0 (further down)
        assert_eq!(l[0], -2.0);
    }

    #[test]
    fn repeat_penalty_one_is_noop() {
        let mut l = [2.0, 1.0];
        apply_repeat_penalty(&mut l, 1.0, &[0, 1]);
        assert_eq!(l, [2.0, 1.0]);
    }

    #[test]
    fn repeat_penalty_ignores_out_of_range_ids() {
        let mut l = [2.0, 1.0];
        apply_repeat_penalty(&mut l, 2.0, &[99]); // id 99 absent — no panic, no change
        assert_eq!(l, [2.0, 1.0]);
    }

    // ── top-nσ (arXiv:2411.07641) ───────────────────────────────────────

    #[test]
    fn top_n_sigma_keeps_the_informative_region() {
        // max=10, far-noise at 0; with n=1 the noise floor drops out.
        let mut l = [10.0, 9.5, 0.0, 0.1, 0.2];
        let n = apply_top_n_sigma(&mut l, 1.0);
        assert!(
            l[0].is_finite() && l[1].is_finite(),
            "informative region survives"
        );
        assert!(l[2] == f32::NEG_INFINITY, "noise floor masked");
        assert_eq!(n, 2);
    }

    #[test]
    fn top_n_sigma_disabled_is_noop() {
        let mut l = [3.0, 2.0, 1.0];
        let n = apply_top_n_sigma(&mut l, 0.0);
        assert_eq!(n, 3);
        assert_eq!(l, [3.0, 2.0, 1.0]);
    }

    #[test]
    fn top_n_sigma_degenerate_uniform_keeps_all() {
        // All equal → σ=0 → threshold = max → everything (== max) survives.
        let mut l = [1.5, 1.5, 1.5];
        assert_eq!(apply_top_n_sigma(&mut l, 1.0), 3);
    }

    #[test]
    fn top_n_sigma_pins_the_variance_arithmetic() {
        // Numeric pin for the mean/σ computation (kills the d=l−mean→l+mean
        // and sum +=→*=/−= mutants, which all shift the threshold):
        // [5,4,3,2] → mean 3.5 · σ = √1.25 ≈ 1.118 · n=1 → threshold ≈ 3.88
        // → exactly {5, 4} survive; 3 and 2 are masked.
        let mut l = [5.0, 4.0, 3.0, 2.0];
        let n = apply_top_n_sigma(&mut l, 1.0);
        assert_eq!(n, 2, "exactly the two informative logits survive");
        assert!(l[0].is_finite() && l[1].is_finite());
        assert_eq!(l[2], f32::NEG_INFINITY);
        assert_eq!(l[3], f32::NEG_INFINITY);
    }

    #[test]
    fn top_n_sigma_scrubs_nan_fail_closed() {
        let mut l = [f32::NAN, 5.0, 4.9];
        let n = apply_top_n_sigma(&mut l, 2.0);
        assert_eq!(l[0], f32::NEG_INFINITY, "NaN → -inf before stats");
        assert!(l[1].is_finite() && l[2].is_finite());
        assert_eq!(n, 2);
    }

    // ── token mask (constrained decoding) ──────────────────────────────

    #[test]
    fn token_mask_zeroes_disallowed() {
        let mut p = [0.5, 0.3, 0.2];
        let n = apply_token_mask(&mut p, &[true, false, true]);
        assert_eq!(n, 2);
        assert_eq!(p, [0.5, 0.0, 0.2]);
    }

    #[test]
    fn token_mask_shorter_than_probs_forbids_the_tail() {
        // Fail-closed: an under-specified mask must forbid, never allow.
        let mut p = [0.5, 0.3, 0.2];
        let n = apply_token_mask(&mut p, &[true]); // only index 0 covered
        assert_eq!(n, 1);
        assert_eq!(p, [0.5, 0.0, 0.0]);
    }

    #[test]
    fn token_mask_allowed_zero_prob_is_not_a_survivor() {
        // Mutant killer (`> 0.0` → `>= 0.0`): an ALLOWED token whose prob is
        // already zero is not a live choice — counting it would let a caller
        // believe the grammar admits something samplable when it does not.
        let mut p = [0.0f32, 0.4];
        let n = apply_token_mask(&mut p, &[true, true]);
        assert_eq!(n, 1, "zero-prob allowed entry must not count");
    }

    #[test]
    fn token_mask_all_disallowed_returns_zero() {
        // 0 survivors = the grammar admits nothing here; the caller MUST
        // treat this as an error, not sample from an empty distribution.
        let mut p = [0.5, 0.5];
        let n = apply_token_mask(&mut p, &[false, false]);
        assert_eq!(n, 0);
        assert_eq!(p, [0.0, 0.0]);
    }

    // ── properties (precision over the input space) ─────────────────────

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(512))]

        /// min-p NEVER empties a distribution that had a positive entry, and
        /// NEVER leaves a survivor below the floor. The two invariants that
        /// make dynamic truncation safe to sample from.
        #[test]
        fn prop_min_p_keeps_max_and_respects_floor(
            raw in proptest::collection::vec(0.0f32..1.0, 1..64),
            p_base in 0.01f32..1.0,
        ) {
            let mut probs = raw.clone();
            let had_positive = probs.iter().any(|p| *p > 0.0);
            let max = probs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            let survivors = apply_min_p(&mut probs, p_base);
            if had_positive && max > 0.0 {
                proptest::prop_assert!(survivors >= 1, "must keep ≥1 when a positive entry existed");
                let floor = p_base * max;
                for &p in &probs {
                    proptest::prop_assert!(
                        p == 0.0 || p + 1e-6 >= floor,
                        "every survivor must be ≥ floor (p={p}, floor={floor})"
                    );
                }
            }
        }

        /// A token mask output never has a non-zero prob at a disallowed index,
        /// and the survivor count equals the allowed-and-positive count.
        #[test]
        fn prop_token_mask_zeroes_exactly_the_disallowed(
            raw in proptest::collection::vec(0.0f32..1.0, 1..64),
            mask in proptest::collection::vec(proptest::bool::ANY, 1..64),
        ) {
            let mut probs = raw.clone();
            let survivors = apply_token_mask(&mut probs, &mask);
            let mut expected = 0usize;
            for (i, &p0) in raw.iter().enumerate() {
                let allowed = mask.get(i).copied().unwrap_or(false);
                if allowed && p0 > 0.0 {
                    expected += 1;
                    // exact bit-equality on purpose — the value is copied
                    // through untouched, so `to_bits` is the precise check
                    // (and dodges clippy::float_cmp, which can't prove it).
                    proptest::prop_assert_eq!(probs[i].to_bits(), p0.to_bits(),
                        "allowed survivor must be byte-unchanged");
                } else {
                    proptest::prop_assert_eq!(probs[i].to_bits(), 0.0f32.to_bits(),
                        "disallowed/zero must be zeroed");
                }
            }
            proptest::prop_assert_eq!(survivors, expected);
        }

        /// THE top-nσ paper claim, proven: the survivor SET is temperature-
        /// invariant. Dividing every logit by T scales (max − x) and σ by
        /// the same factor, so `x ≥ max − n·σ` holds before iff after —
        /// unlike top-p/min-p, high temperature can never re-admit noise.
        #[test]
        fn prop_top_n_sigma_is_temperature_invariant(
            raw in proptest::collection::vec(-15.0f32..15.0, 2..48),
            n in 0.5f32..4.0,
            temp in 0.25f32..4.0,
        ) {
            let mut base = raw.clone();
            let _ = apply_top_n_sigma(&mut base, n);

            let mut scaled: Vec<f32> = raw.iter().map(|l| l / temp).collect();
            let _ = apply_top_n_sigma(&mut scaled, n);

            // The law is exact in real arithmetic; in f32 the ONLY legal
            // disagreement is a rounding tie AT the threshold (found by
            // the 2026-08-01 seed: one token within float noise of
            // `max − n·σ` flips across scalings). Recompute the base
            // threshold the way the sampler does and demand every
            // disagreeing index sit inside that tie band — a flip AWAY
            // from the boundary stays a refutation.
            let finite: Vec<f32> = raw.iter().copied().filter(|l| l.is_finite()).collect();
            let denom = f64::from(u32::try_from(finite.len()).expect("gen caps at 48"));
            let max = finite.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            let mean = finite.iter().map(|&l| f64::from(l)).sum::<f64>() / denom;
            let var = finite
                .iter()
                .map(|&l| (f64::from(l) - mean).powi(2))
                .sum::<f64>()
                / denom;
            #[allow(clippy::cast_possible_truncation)]
            let threshold = max - n * var.sqrt() as f32;
            let tie_band = 1e-3f32 * threshold.abs().max(1.0);
            for i in 0..raw.len() {
                if base[i].is_finite() != scaled[i].is_finite() {
                    proptest::prop_assert!(
                        (raw[i] - threshold).abs() <= tie_band,
                        "survivor SET diverged OFF the tie band at T={} (index {} · logit {} · threshold {})",
                        temp, i, raw[i], threshold
                    );
                }
            }
        }

        /// The max logit always survives top-nσ, and every survivor is within
        /// n·σ of the max (the threshold semantic, over the input space).
        #[test]
        fn prop_top_n_sigma_keeps_max_and_respects_threshold(
            raw in proptest::collection::vec(-20.0f32..20.0, 1..64),
            n in 0.1f32..5.0,
        ) {
            let max_before = raw.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            let mut l = raw.clone();
            let survivors = apply_top_n_sigma(&mut l, n);
            proptest::prop_assert!(survivors >= 1, "the max token must always survive");
            let max_after = l.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            proptest::prop_assert_eq!(max_before.to_bits(), max_after.to_bits(),
                "the max logit must be untouched");
        }

        /// Repeat penalty is monotone-down on seen tokens (never raises a
        /// logit) and leaves unseen tokens untouched.
        #[test]
        fn prop_repeat_penalty_never_raises_seen(
            logits in proptest::collection::vec(-10.0f32..10.0, 1..64),
            penalty in 1.0f32..3.0,
        ) {
            let mut after = logits.clone();
            let seen: Vec<u32> = vec![0]; // penalize index 0
            apply_repeat_penalty(&mut after, penalty, &seen);
            proptest::prop_assert!(after[0] <= logits[0] + 1e-6, "penalty must not raise a seen logit");
            for i in 1..logits.len() {
                proptest::prop_assert_eq!(after[i], logits[i], "unseen logit must be unchanged");
            }
        }
    }
}
