//! Extended-Wilkinson tick labeling · Talbot, Lin & Hanrahan, `InfoVis` 2010
//! (DOI 10.1109/TVCG.2010.130) · CRAFT-implemented from the paper + the R
//! `labeling::extended` reference. Density term uses the `2 − max(ρ/ρt, ρt/ρ)`
//! form (the paper prints `1 −`, inconsistent with its own bounds — the R
//! reference confirms `2 −` · master plan CHT §3bis erratum).
//!
//! Weight order (simplicity, coverage, density, legibility) = R reference
//! `w = c(0.25, 0.2, 0.5, 0.05)`. All arithmetic is +,-,*,/ on f64 — the
//! IEEE-deterministic subset · no transcendentals (POW10 table + scans).

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::many_single_char_names
)] // TLH search (paper notation: q/j/k/z/m) · index/power casts
/// Exact powers of ten for z ∈ [-18, 22] (1e22 is the last exact f64 power).
/// Const table ⇒ identical bits everywhere.
const POW10: [f64; 41] = [
    1e-18, 1e-17, 1e-16, 1e-15, 1e-14, 1e-13, 1e-12, 1e-11, 1e-10, 1e-9, 1e-8, 1e-7, 1e-6, 1e-5,
    1e-4, 1e-3, 1e-2, 1e-1, 1e0, 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8, 1e9, 1e10, 1e11, 1e12,
    1e13, 1e14, 1e15, 1e16, 1e17, 1e18, 1e19, 1e20, 1e21, 1e22,
];
const POW10_MIN: i32 = -18;
const POW10_MAX: i32 = 22;

/// 10^z via the const table (clamped to table bounds — chart data beyond
/// 1e±18 is rejected upstream by the domain guards).
#[must_use]
pub fn pow10(z: i32) -> f64 {
    let idx = (z - POW10_MIN).clamp(0, (POW10.len() as i32) - 1) as usize;
    POW10.get(idx).copied().unwrap_or(1.0)
}

/// floor(log10(x)) for finite x > 0, via table scan (zero transcendentals).
#[must_use]
pub fn ilog10_floor(x: f64) -> i32 {
    let mut z = POW10_MIN;
    // Largest z with 10^z <= x.
    for (i, p) in POW10.iter().enumerate() {
        if *p <= x {
            z = POW10_MIN + i as i32;
        } else {
            break;
        }
    }
    z
}

/// ceil(log10(x)) for finite x > 0.
#[must_use]
pub fn ilog10_ceil(x: f64) -> i32 {
    let f = ilog10_floor(x);
    #[allow(clippy::float_cmp)] // POW10 entries are EXACT f64 values — equality is the contract
    if pow10(f) == x {
        f
    } else {
        (f + 1).min(POW10_MAX)
    }
}

/// The preferred step mantissas, in preference order (TLH §5).
const Q: [f64; 6] = [1.0, 5.0, 2.0, 2.5, 4.0, 3.0];
/// Score weights · (simplicity, coverage, density, legibility) · R reference.
const W: [f64; 4] = [0.25, 0.2, 0.5, 0.05];
const EPS: f64 = 1e-10;

/// A labeling: ticks at `lmin + i·lstep` for i in 0..k.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TickSpec {
    pub lmin: f64,
    pub lstep: f64,
    pub k: usize,
}

impl TickSpec {
    #[must_use]
    pub fn lmax(&self) -> f64 {
        self.lmin + self.lstep * ((self.k - 1) as f64)
    }

    /// Materialize tick values (index arithmetic · no accumulation drift).
    #[must_use]
    pub fn values(&self) -> Vec<f64> {
        (0..self.k)
            .map(|i| self.lmin + self.lstep * (i as f64))
            .collect()
    }
}

fn simplicity(qi: usize, j: f64, lmin: f64, lmax: f64, lstep: f64) -> f64 {
    let n = Q.len() as f64;
    let i = qi as f64 + 1.0;
    let rem = ((lmin % lstep) + lstep) % lstep;
    let v = if (rem < EPS || lstep - rem < EPS) && lmin <= 0.0 && lmax >= 0.0 {
        1.0
    } else {
        0.0
    };
    1.0 - (i - 1.0) / (n - 1.0) - j + v
}

fn simplicity_max(qi: usize, j: f64) -> f64 {
    let n = Q.len() as f64;
    let i = qi as f64 + 1.0;
    1.0 - (i - 1.0) / (n - 1.0) - j + 1.0
}

fn coverage(dmin: f64, dmax: f64, lmin: f64, lmax: f64) -> f64 {
    let r = 0.1 * (dmax - dmin);
    1.0 - 0.5 * ((dmax - lmax) * (dmax - lmax) + (dmin - lmin) * (dmin - lmin)) / (r * r)
}

fn coverage_max(dmin: f64, dmax: f64, span: f64) -> f64 {
    let range = dmax - dmin;
    if span > range {
        let half = (span - range) / 2.0;
        let r = 0.1 * range;
        1.0 - 0.5 * (half * half + half * half) / (r * r)
    } else {
        1.0
    }
}

/// Erratum form: `2 − max(ρ/ρt, ρt/ρ)`.
fn density(k: usize, m: f64, dmin: f64, dmax: f64, lmin: f64, lmax: f64) -> f64 {
    let r = (k as f64 - 1.0) / (lmax - lmin);
    let rt = (m - 1.0) / (dmax.max(lmax) - dmin.min(lmin));
    2.0 - (r / rt).max(rt / r)
}

fn density_max(k: usize, m: f64) -> f64 {
    if k as f64 >= m {
        2.0 - (k as f64 - 1.0) / (m - 1.0)
    } else {
        1.0
    }
}

/// Search state carried through the candidate loops.
struct Best {
    score: f64,
    spec: Option<TickSpec>,
}

/// Extended-Wilkinson search. `m` = target label count (e.g. 5.0).
/// Returns None when the domain is degenerate (caller pads first).
#[must_use]
pub fn extended_wilkinson(dmin: f64, dmax: f64, m: f64) -> Option<TickSpec> {
    extended_wilkinson_opts(dmin, dmax, m, false)
}

/// Search with an INTEGER-DOMAIN constraint (Count semantic: run numbers ·
/// item counts). When `integer_only`, candidate labelings with fractional
/// step or start are excluded from the SEARCH (not post-filtered — spacing
/// stays optimal under the constraint).
#[must_use]
pub fn extended_wilkinson_opts(
    dmin: f64,
    dmax: f64,
    m: f64,
    integer_only: bool,
) -> Option<TickSpec> {
    if !(dmax - dmin).is_finite() || dmax - dmin < EPS || m < 2.0 {
        return None;
    }
    let mut best = Best {
        score: -2.0,
        spec: None,
    };
    // j = label skip amount · j=1 dominates; pruning breaks early anyway.
    for j_int in 1..=3_i32 {
        let j = f64::from(j_int);
        let mut j_prunes_all = true;
        for (qi, q) in Q.iter().enumerate() {
            let sm = simplicity_max(qi, j);
            if W[0] * sm + W[1] + W[2] + W[3] < best.score {
                continue; // this (q, j) can never win
            }
            j_prunes_all = false;
            search_k(dmin, dmax, m, qi, *q, j, sm, integer_only, &mut best);
        }
        if j_prunes_all {
            break;
        }
    }
    best.spec
}

/// Inner loops over label count k, power z and start offset (TLH structure).
#[allow(clippy::too_many_arguments)]
fn search_k(
    dmin: f64,
    dmax: f64,
    m: f64,
    qi: usize,
    q: f64,
    j: f64,
    sm: f64,
    integer_only: bool,
    best: &mut Best,
) {
    for k in 2..=13_usize {
        let dm = density_max(k, m);
        if W[0] * sm + W[1] + W[2] * dm + W[3] < best.score {
            break;
        }
        let delta = (dmax - dmin) / ((k + 1) as f64) / (j * q);
        if delta <= 0.0 {
            continue;
        }
        let z_start = ilog10_ceil(delta);
        for z in z_start..=POW10_MAX {
            let step = j * q * pow10(z);
            if integer_only && step.fract() != 0.0 {
                continue;
            }
            let span = step * ((k - 1) as f64);
            let cm = coverage_max(dmin, dmax, span);
            if W[0] * sm + W[1] * cm + W[2] * dm + W[3] < best.score {
                break;
            }
            let min_start = ((dmax / step).floor() * j - ((k - 1) as f64) * j) as i64;
            let max_start = ((dmin / step).ceil() * j) as i64;
            if min_start > max_start {
                continue;
            }
            for start in min_start..=max_start.min(min_start + 1000) {
                let lmin = (start as f64) * step / j;
                if integer_only && lmin.fract() != 0.0 {
                    continue;
                }
                let lmax = lmin + span;
                let s = simplicity(qi, j, lmin, lmax, step);
                let c = coverage(dmin, dmax, lmin, lmax);
                let d = density(k, m, dmin, dmax, lmin, lmax);
                let score = W[0] * s + W[1] * c + W[2] * d + W[3] * 1.0;
                if score > best.score && lmin <= dmin && lmax >= dmax {
                    best.score = score;
                    best.spec = Some(TickSpec {
                        lmin,
                        lstep: step,
                        k,
                    });
                }
            }
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::unreadable_literal
)]
mod tests {
    use super::*;

    #[test]
    fn pow10_matches_literals() {
        assert_eq!(pow10(0), 1.0);
        assert_eq!(pow10(3), 1000.0);
        assert_eq!(pow10(-3), 0.001);
        assert_eq!(pow10(22), 1e22);
    }

    #[test]
    fn ilog10_bounds() {
        assert_eq!(ilog10_floor(1.0), 0);
        assert_eq!(ilog10_floor(999.0), 2);
        assert_eq!(ilog10_floor(1000.0), 3);
        assert_eq!(ilog10_ceil(1000.0), 3);
        assert_eq!(ilog10_ceil(1001.0), 4);
        assert_eq!(ilog10_floor(0.02), -2);
    }

    #[test]
    fn hundred_gets_quarter_steps() {
        let t = extended_wilkinson(0.0, 100.0, 5.0).expect("ticks");
        assert_eq!(t.lmin, 0.0);
        assert_eq!(t.lmax(), 100.0);
        assert_eq!(t.k, 5);
        assert_eq!(t.lstep, 25.0);
    }

    #[test]
    fn loose_covers_domain() {
        for (lo, hi) in [(0.0, 3.0), (0.013, 3.7), (2.0, 2400.0), (1e-5, 3.2e-4)] {
            let t = extended_wilkinson(lo, hi, 5.0).expect("ticks");
            assert!(t.lmin <= lo, "lmin {} > lo {}", t.lmin, lo);
            assert!(t.lmax() >= hi, "lmax {} < hi {}", t.lmax(), hi);
            assert!(t.k >= 2 && t.k <= 13);
        }
    }

    #[test]
    fn deterministic_repeat() {
        let a = extended_wilkinson(0.013, 3.7, 5.0);
        let b = extended_wilkinson(0.013, 3.7, 5.0);
        assert_eq!(a, b);
    }

    #[test]
    fn degenerate_domain_is_none() {
        assert!(extended_wilkinson(2.0, 2.0, 5.0).is_none());
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::unreadable_literal
)]
mod integer_tests {
    use super::*;

    #[test]
    fn integer_mode_yields_integer_steps_and_starts() {
        for (lo, hi) in [(1.0, 10.0), (1.0, 3.0), (1.0, 1000.0), (0.0, 7.0)] {
            let t = extended_wilkinson_opts(lo, hi, 6.0, true).expect("ticks");
            assert_eq!(t.lstep.fract(), 0.0, "step must be integer for ({lo},{hi})");
            assert_eq!(t.lmin.fract(), 0.0, "start must be integer for ({lo},{hi})");
            assert!(t.lstep >= 1.0);
            for v in t.values() {
                assert_eq!(v.fract(), 0.0);
            }
        }
    }

    #[test]
    fn two_runs_get_unit_steps() {
        let t = extended_wilkinson_opts(1.0, 2.0, 6.0, true).expect("ticks");
        assert_eq!(t.lstep, 1.0);
    }

    #[test]
    fn integer_mode_still_covers_domain() {
        let t = extended_wilkinson_opts(1.0, 12.0, 6.0, true).expect("ticks");
        assert!(t.lmin <= 1.0 && t.lmax() >= 12.0);
    }
}
