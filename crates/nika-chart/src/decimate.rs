//! LTTB downsampling · Largest-Triangle-Three-Buckets.
//!
//! Steinarsson, *Downsampling Time Series for Visual Representation*, `MSc`
//! thesis, University of Iceland, 2013 · hdl 1946/15343 · Algorithm 4.2
//! (CRAFT-implemented from the thesis · R2/R3 pins). Deterministic given
//! (data, threshold): first/last anchored · `threshold−2` equal buckets ·
//! per bucket keep the point maximizing the triangle area with the
//! previously KEPT point and the NEXT bucket's average.
//!
//! Tie-break (unspecified in the thesis prose · documented HERE as contract):
//! first strictly-greater area wins ⇒ the EARLIEST point of equal area is
//! kept — matches the author's reference implementation.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)] // LTTB bucket arithmetic (usize↔f64 per the thesis)
/// Downsample `pts` to ≤ `threshold` points. Identity when already small.
/// Pure f64 arithmetic (+,−,×) — the IEEE-deterministic subset.
#[must_use]
pub fn lttb(pts: &[(f64, f64)], threshold: usize) -> Vec<(f64, f64)> {
    let n = pts.len();
    if threshold >= n || threshold < 3 {
        return pts.to_vec();
    }
    let mut out = Vec::with_capacity(threshold);
    let first = match pts.first() {
        Some(p) => *p,
        None => return Vec::new(),
    };
    out.push(first);

    let every = ((n - 2) as f64) / ((threshold - 2) as f64);
    let mut a = 0usize; // index of the previously kept point

    for i in 0..(threshold - 2) {
        // Current bucket bounds.
        let start = ((i as f64) * every).floor() as usize + 1;
        let end = (((i + 1) as f64) * every).floor() as usize + 1;
        let end = end.min(n - 1);
        // Next bucket average (the thesis' point C).
        let nstart = end;
        let nend = ((((i + 2) as f64) * every).floor() as usize + 1).min(n);
        let span = (nend - nstart).max(1) as f64;
        let (mut cx, mut cy) = (0.0, 0.0);
        for p in pts.get(nstart..nend).unwrap_or(&[]) {
            cx += p.0;
            cy += p.1;
        }
        cx /= span;
        cy /= span;

        let pa = pts.get(a).copied().unwrap_or(first);
        let mut best_idx = start;
        let mut best_area = -1.0_f64;
        for (off, p) in pts.get(start..end).unwrap_or(&[]).iter().enumerate() {
            // 2×triangle area (constant factor irrelevant to argmax).
            let area = ((pa.0 - cx) * (p.1 - pa.1) - (pa.0 - p.0) * (cy - pa.1)).abs();
            if area > best_area {
                best_area = area;
                best_idx = start + off;
            }
        }
        out.push(pts.get(best_idx).copied().unwrap_or(pa));
        a = best_idx;
    }

    if let Some(last) = pts.last() {
        out.push(*last);
    }
    out
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

    fn wave(n: usize) -> Vec<(f64, f64)> {
        // Deterministic zigzag with one big spike — no transcendentals.
        (0..n)
            .map(|i| {
                let x = i as f64;
                let base = ((i * 37) % 100) as f64 / 10.0;
                let spike = if i == n / 2 { 80.0 } else { 0.0 };
                (x, base + spike)
            })
            .collect()
    }

    #[test]
    fn anchors_first_and_last() {
        let pts = wave(1000);
        let d = lttb(&pts, 100);
        assert_eq!(d.len(), 100);
        assert_eq!(d[0], pts[0]);
        assert_eq!(d[99], pts[999]);
    }

    #[test]
    fn keeps_the_spike() {
        let pts = wave(1000);
        let d = lttb(&pts, 50);
        let spike = pts[500];
        assert!(d.contains(&spike), "LTTB must keep the extreme point");
    }

    #[test]
    fn identity_when_small() {
        let pts = wave(10);
        assert_eq!(lttb(&pts, 100), pts);
        assert_eq!(lttb(&pts, 2), pts); // threshold < 3 → identity
    }

    #[test]
    fn deterministic() {
        let pts = wave(5000);
        assert_eq!(lttb(&pts, 200), lttb(&pts, 200));
    }

    #[test]
    fn monotone_x_preserved() {
        let pts = wave(2000);
        let d = lttb(&pts, 120);
        for w in d.windows(2) {
            assert!(w[0].0 < w[1].0, "x must stay strictly increasing");
        }
    }
}
