// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The forecast math — one exact quantile (Hyndman & Fan type-7, the
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
pub(crate) enum Prior {
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
pub(crate) const BANDS_MIN_N: usize = 5;

impl Prior {
    /// Build the rung the sample size earns. Input must already be
    /// finite-filtered (the caller counts the drops — C2 accounting).
    #[must_use]
    pub(crate) fn from_finite(values: &[f64]) -> Self {
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
    pub(crate) const fn n(&self) -> usize {
        match self {
            Self::NeverRan => 0,
            Self::LastRun { .. } => 1,
            Self::Range { n, .. } | Self::Bands { n, .. } => *n,
        }
    }
}

/// Hyndman & Fan type-7 (numpy/R default · C1) over an ASCENDING-sorted,
/// all-finite slice. `None` on empty input; `q` outside `[0, 1]` clamps; a
/// non-finite `q` refuses (never NaN out). Plain lerp — golden parity
/// over cleverness (no `mul_add`).
#[must_use]
pub(crate) fn quantile_h7(sorted: &[f64], q: f64) -> Option<f64> {
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
