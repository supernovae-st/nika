// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Forensic statistics — the fourth shared seam: the honesty
//! ladder every learned-truth reader speaks. one exact quantile (Hyndman & Fan type-7, the
//! numpy/R default) and the honesty ladder as a TYPE: a p50 under
//! `BANDS_MIN_N` samples cannot be constructed, not merely not
//! rendered. Learned truth stays learned — bands, never points; the
//! sample count rides every rung.

/// The C3 honesty ladder — the rung IS the type. The JSON twin
/// inherits the same honesty through the `kind` tag (internally
/// tagged · consumers tolerate unknown kinds, the law `EventKind`
/// teaches).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Prior {
    /// n = 0 — no numbers exist. « never run », never an invention.
    NeverRan,
    /// n = 1 — the one observation, named as such (no percentile words).
    LastRun {
        /// The single observed value.
        value: f64,
    },
    /// 2 ≤ n < [`BANDS_MIN_N`] — honest spread only.
    Range {
        /// Sample count.
        n: usize,
        /// Smallest observation.
        min: f64,
        /// Largest observation.
        max: f64,
    },
    /// n ≥ [`BANDS_MIN_N`] — H&F-7 bands (C1).
    Bands {
        /// Sample count.
        n: usize,
        /// Smallest observation.
        min: f64,
        /// Median (H&F-7).
        p50: f64,
        /// 90th percentile (H&F-7).
        p90: f64,
        /// Largest observation.
        max: f64,
    },
}

/// C3: percentile vocabulary is earned at n ≥ 5 (p90 ≈ max until
/// n > 1/(1−p) — the 1/(1-p) rule).
pub const BANDS_MIN_N: usize = 5;

impl Prior {
    /// Build the rung the sample size earns. Input must already be
    /// finite-filtered (the caller counts the drops — C2 accounting).
    #[must_use]
    pub fn from_finite(values: &[f64]) -> Self {
        let mut xs = values.to_vec();
        xs.sort_unstable_by(f64::total_cmp);
        match xs.as_slice() {
            [] => Self::NeverRan,
            [one] => Self::LastRun { value: *one },
            [min, .., max] if xs.len() < BANDS_MIN_N => Self::Range {
                n: xs.len(),
                min: *min,
                max: *max,
            },
            [min, .., max] => match (quantile_h7(&xs, 0.5), quantile_h7(&xs, 0.9)) {
                (Some(p50), Some(p90)) => Self::Bands {
                    n: xs.len(),
                    min: *min,
                    p50,
                    p90,
                    max: *max,
                },
                _ => Self::Range {
                    n: xs.len(),
                    min: *min,
                    max: *max,
                },
            },
        }
    }

    /// Sample count on any rung (render's « based on last N runs »).
    #[must_use]
    pub const fn n(&self) -> usize {
        match self {
            Self::NeverRan => 0,
            Self::LastRun { .. } => 1,
            Self::Range { n, .. } | Self::Bands { n, .. } => *n,
        }
    }
}

/// A finite-sample upper prediction bound — split-conformal in its
/// order-statistic form. For exchangeable samples the NEXT observation
/// falls at or below `bound` with probability **at least** `k/(n+1)`,
/// and **exactly** `k/(n+1)` when the distribution is continuous (ties
/// only push true coverage higher — conservative, never invalid). A
/// distribution-free THEOREM about the next run, not an estimate
/// (Angelopoulos–Barber–Bates, CUP 2026 · arXiv:2411.11824 Theorem 3.2).
///
/// Scope of the promise: « next » means the next observation drawn
/// from the SAME population the sample was gathered from — the
/// gather's filters define it (same workflow content · same model ·
/// completed runs). The guarantee says nothing about a run the
/// filters would have excluded.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
#[non_exhaustive]
pub struct ConformalUpper {
    /// The bound — the k-th order statistic, a RAW sample value:
    /// interpolating between order statistics would break the theorem.
    pub bound: f64,
    /// Coverage numerator: P(next ≤ bound) ≥ k/(n+1).
    pub k: usize,
    /// Sample count (the coverage denominator is n+1).
    pub n: usize,
}

impl ConformalUpper {
    /// Assemble a bound (invariant #19: `#[non_exhaustive]` structs
    /// construct through `new`, never a literal).
    #[must_use]
    pub fn new(bound: f64, k: usize, n: usize) -> Self {
        Self { bound, k, n }
    }
}

/// The level-`num/den` conformal upper bound over an ASCENDING-sorted,
/// all-finite slice (the [`quantile_h7`] contract — and finiteness is
/// load-bearing HERE: `total_cmp` sorts NaN above `+∞`, which would
/// poison exactly this order statistic).
///
/// `k = ⌈(num/den)·(n+1)⌉` is computed in INTEGER arithmetic. The f64
/// route fails at the exact boundary: `0.9` is not representable,
/// `0.9_f64 * 10.0 == 9.000000000000002`, and its ceil declares n = 9
/// infeasible when the mathematics says k = 9 works.
///
/// `None` = honestly infeasible — no distribution-free level-`num/den`
/// bound exists at this n (feasibility law: `n ≥ num/(den−num)` · nine
/// samples for 90%, nineteen for 95%). Never a number without its
/// theorem. Degenerate levels refuse the same way: `num == 0` and
/// `den == 0` are guarded (k would underflow · division by zero);
/// `num ≥ den` needs NO guard — it forces `k ≥ n+1`, and the
/// feasibility law already refuses it (every clause load-bearing).
#[must_use]
pub fn conformal_upper(sorted: &[f64], num: u32, den: u32) -> Option<ConformalUpper> {
    // Unsorted input would not degrade into a wrong VALUE here — it
    // degrades into a wrong GUARANTEE (the worst failure class this
    // crate can produce), so the contract is checked where debug
    // builds can see it.
    debug_assert!(
        sorted.is_sorted(),
        "conformal_upper needs ascending-sorted input — the guarantee fails silently otherwise"
    );
    if num == 0 || den == 0 {
        return None;
    }
    let n = sorted.len();
    let np1 = u64::try_from(n).ok()?.checked_add(1)?;
    // k = ⌈num·(n+1)/den⌉ — exact, total, no transcendentals.
    let k_exact = u64::from(num)
        .checked_mul(np1)?
        .checked_add(u64::from(den) - 1)?
        / u64::from(den);
    let k = usize::try_from(k_exact).ok()?;
    if k > n {
        return None;
    }
    Some(ConformalUpper::new(sorted.get(k - 1).copied()?, k, n))
}

/// Hyndman & Fan type-7 (numpy/R default · C1) over an ASCENDING-sorted,
/// all-finite slice. `None` on empty input; `q` outside `[0, 1]` clamps; a
/// non-finite `q` refuses (never NaN out). Plain lerp — golden parity
/// over cleverness (no `mul_add`).
#[must_use]
pub fn quantile_h7(sorted: &[f64], q: f64) -> Option<f64> {
    if !q.is_finite() {
        return None;
    }
    let last = sorted.last().copied()?;
    let q = q.clamp(0.0, 1.0);
    #[allow(clippy::cast_precision_loss)] // n ≤ SCAN_CAP (200) ≪ 2^52
    let h = (sorted.len() - 1) as f64 * q;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 0 ≤ h ≤ n−1
    let j = h.floor() as usize;
    let g = h - h.floor();
    let lo = sorted.get(j).copied()?;
    let hi = sorted.get(j + 1).copied().unwrap_or(last);
    Some(lo + g * (hi - lo))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// Relative approx-eq (≤1e-9) — `q*(n-1)` carries ulp noise, golden
    /// vectors NEVER assert bit-equality (§2.3).
    fn close(a: f64, b: f64) {
        let scale = a.abs().max(b.abs()).max(1.0);
        assert!((a - b).abs() <= 1e-9 * scale, "{a} !~ {b}");
    }

    #[test]
    fn golden_vectors_match_numpy_linear_default() {
        // §2.3 pins · verified against numpy 2.4.0 method='linear'.
        close(
            quantile_h7(&[1.0, 2.0, 3.0, 4.0], 0.9).expect("test value"),
            3.7,
        );
        for q in [0.0, 0.5, 0.9, 1.0] {
            close(quantile_h7(&[10.0], q).expect("test value"), 10.0);
        }
        close(quantile_h7(&[1.0, 2.0], 0.5).expect("test value"), 1.5);
        let five = [120.0, 150.0, 180.0, 200.0, 240.0];
        close(quantile_h7(&five, 0.5).expect("test value"), 180.0); // exact-hit g=0
        close(quantile_h7(&five, 0.9).expect("test value"), 224.0); // interpolation g=0.6
    }

    #[test]
    fn conformal_bound_is_a_raw_order_statistic() {
        let five = [120.0, 150.0, 180.0, 200.0, 240.0];
        // τ=1/2, n=5: k=⌈6/2⌉=3 → the third value, coverage ≥ 3/6.
        let c = conformal_upper(&five, 1, 2).expect("feasible");
        assert_eq!((c.bound, c.k, c.n), (180.0, 3, 5));
        // τ=4/5, n=5: k=⌈24/5⌉=5 → the max, coverage 5/6 — never an
        // interpolated value (the theorem lives on raw samples).
        let c = conformal_upper(&five, 4, 5).expect("feasible");
        assert_eq!((c.bound, c.k), (240.0, 5));
    }

    #[test]
    fn the_feasibility_frontier_is_integer_exact_not_floating() {
        // τ=9/10 needs n ≥ 9 — and AT n=9, k=⌈9·10/10⌉=9 EXACTLY. The
        // f64 route (0.9_f64 * 10.0 = 9.000000000000002 → ceil = 10)
        // declares this infeasible; the integer route keeps the
        // theorem's frontier where the mathematics puts it.
        let nine: Vec<f64> = (1..=9).map(f64::from).collect();
        let c = conformal_upper(&nine, 9, 10).expect("n=9 IS feasible at 90%");
        assert_eq!((c.k, c.bound), (9, 9.0));
        let eight: Vec<f64> = (1..=8).map(f64::from).collect();
        assert_eq!(conformal_upper(&eight, 9, 10), None, "n=8 is not");
        // τ=19/20 needs n ≥ 19.
        let nineteen: Vec<f64> = (1..=19).map(f64::from).collect();
        assert!(conformal_upper(&nineteen, 19, 20).is_some());
        assert!(conformal_upper(&nineteen[..18], 19, 20).is_none());
        // Degenerate levels refuse: zero parts · τ ≥ 1 · empty sample.
        assert_eq!(conformal_upper(&nine, 0, 10), None);
        assert_eq!(conformal_upper(&nine, 10, 0), None);
        assert_eq!(conformal_upper(&nine, 10, 10), None);
        assert_eq!(conformal_upper(&nine, 11, 10), None);
        assert_eq!(conformal_upper(&[], 1, 2), None);
    }

    proptest! {
        /// The coverage THEOREM as a deterministic property — counted,
        /// never sampled. For n+1 DISTINCT exchangeable values, each
        /// leave-one-out split is equally likely, and the k-th order
        /// statistic of the kept n covers the held-out value in
        /// EXACTLY k of the n+1 splits (continuous case ·
        /// arXiv:2411.11824 Theorem 3.2): hold out y_(j) — the kept k-th
        /// smallest is y_(k+1) when j ≤ k (covers) and y_(k) when
        /// j > k (does not). No Monte-Carlo, no seed sensitivity.
        #[test]
        fn leave_one_out_coverage_is_exactly_k_over_n_plus_1(
            values in proptest::collection::btree_set(0u32..1_000_000, 2..40),
            num in 1u32..20,
            den in 2u32..21,
        ) {
            prop_assume!(num < den);
            let all: Vec<f64> = values.iter().copied().map(f64::from).collect();
            let n = all.len() - 1; // n kept per split · n+1 total
            if conformal_upper(&all[..n], num, den).is_none() {
                return Ok(()); // infeasible (num, den, n) — nothing to count
            }
            let mut covered = 0usize;
            let mut k_seen = 0usize;
            for holdout in 0..all.len() {
                let kept: Vec<f64> = all
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i != holdout)
                    .map(|(_, v)| *v)
                    .collect();
                let c = conformal_upper(&kept, num, den).expect("same n, same k");
                k_seen = c.k;
                if all[holdout] <= c.bound {
                    covered += 1;
                }
            }
            prop_assert_eq!(covered, k_seen);
        }
    }

    /// The sortedness contract is ENFORCED in debug builds (and this
    /// test pins the assert against deletion — a mutant that drops it
    /// stops panicking and fails the expectation).
    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "ascending-sorted")]
    fn unsorted_input_is_refused_loudly_in_debug() {
        let _ = conformal_upper(&[3.0, 1.0, 2.0], 1, 2);
    }

    #[test]
    fn quantile_edges_refuse_honestly() {
        assert_eq!(quantile_h7(&[], 0.5), None);
        assert_eq!(quantile_h7(&[1.0, 2.0], f64::NAN), None);
        close(
            quantile_h7(&[1.0, 2.0, 3.0], -1.0).expect("test value"),
            1.0,
        ); // clamp q=0
        close(quantile_h7(&[1.0, 2.0, 3.0], 2.0).expect("test value"), 3.0); // clamp q=1
    }

    #[test]
    fn ladder_rungs_are_earned_by_n() {
        assert_eq!(Prior::from_finite(&[]), Prior::NeverRan);
        assert_eq!(Prior::from_finite(&[7.0]), Prior::LastRun { value: 7.0 });
        assert_eq!(
            Prior::from_finite(&[3.0, 1.0, 2.0]),
            Prior::Range {
                n: 3,
                min: 1.0,
                max: 3.0
            }
        );
        let bands = Prior::from_finite(&[240.0, 120.0, 180.0, 200.0, 150.0]);
        let Prior::Bands {
            n,
            min,
            p50,
            p90,
            max,
        } = bands
        else {
            unreachable!("n = 5 earns bands, got {bands:?}")
        };
        assert_eq!(n, 5);
        close(min, 120.0);
        close(p50, 180.0);
        close(p90, 224.0);
        close(max, 240.0);
    }

    #[test]
    fn json_twin_is_internally_tagged_per_rung() {
        // One golden per rung — the discriminant is the contract (§4.4).
        let never = serde_json::to_value(Prior::NeverRan).expect("test value");
        assert_eq!(never["kind"], "never_ran");
        let last = serde_json::to_value(Prior::LastRun { value: 3.0 }).expect("test value");
        assert_eq!(last["kind"], "last_run");
        assert!((last["value"].as_f64().expect("test value") - 3.0).abs() < 1e-12);
        let range = serde_json::to_value(Prior::from_finite(&[1.0, 2.0])).expect("test value");
        assert_eq!(range["kind"], "range");
        assert_eq!(range["n"], 2);
        let bands = serde_json::to_value(Prior::from_finite(&[1.0, 2.0, 3.0, 4.0, 5.0]))
            .expect("test value");
        assert_eq!(bands["kind"], "bands");
        assert_eq!(bands["n"], 5);
    }

    proptest! {
        #[test]
        fn permutation_invariance(mut xs in proptest::collection::vec(0.0f64..1e9, 0..40)) {
            let a = Prior::from_finite(&xs);
            xs.reverse();
            let b = Prior::from_finite(&xs);
            prop_assert_eq!(a, b);
        }

        #[test]
        fn bands_are_ordered(xs in proptest::collection::vec(0.0f64..1e9, 5..40)) {
            if let Prior::Bands { min, p50, p90, max, .. } = Prior::from_finite(&xs) {
                prop_assert!(min <= p50 && p50 <= p90 && p90 <= max);
            } else {
                prop_assert!(false, "n >= 5 must earn bands");
            }
        }

        #[test]
        fn quantile_monotone_in_q(xs in proptest::collection::vec(0.0f64..1e9, 1..40),
                                  qa in 0.0f64..1.0, qb in 0.0f64..1.0) {
            let mut s = xs;
            s.sort_unstable_by(f64::total_cmp);
            let (lo, hi) = if qa <= qb { (qa, qb) } else { (qb, qa) };
            let a = quantile_h7(&s, lo).expect("test value");
            let b = quantile_h7(&s, hi).expect("test value");
            prop_assert!(a <= b + 1e-9 * b.abs().max(1.0));
        }

        #[test]
        fn degenerate_n1_all_quantiles_equal(x in 0.0f64..1e9, q in 0.0f64..1.0) {
            let v = quantile_h7(&[x], q).expect("test value");
            prop_assert!((v - x).abs() <= f64::EPSILON * x.abs().max(1.0));
        }

        #[test]
        fn rung_totality(n in 0usize..12) {
            #[allow(clippy::cast_precision_loss)] // proptest n < 12
            let xs: Vec<f64> = (0..n).map(|i| i as f64).collect();
            let rung = Prior::from_finite(&xs);
            let expect_n = rung.n();
            prop_assert_eq!(expect_n, n);
            match (n, rung) {
                (0, Prior::NeverRan)
                | (1, Prior::LastRun { .. })
                | (2..=4, Prior::Range { .. })
                | (5.., Prior::Bands { .. }) => {}
                (n, rung) => prop_assert!(false, "n={n} earned the wrong rung {rung:?}"),
            }
        }
    }
}
