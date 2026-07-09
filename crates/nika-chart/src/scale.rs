//! Scales · pure affine maps (log scale arrives with det-ln · v0.2).

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)] // band index arithmetic
/// Linear domain→range map. Range may be inverted (SVG y grows down).
#[derive(Debug, Clone, Copy)]
pub struct Linear {
    d0: f64,
    d1: f64,
    r0: f64,
    r1: f64,
}

impl Linear {
    /// Caller guarantees d1 > d0 (compile guards the domain).
    #[must_use]
    pub fn new(d0: f64, d1: f64, r0: f64, r1: f64) -> Self {
        Self { d0, d1, r0, r1 }
    }

    #[must_use]
    pub fn map(&self, v: f64) -> f64 {
        let t = (v - self.d0) / (self.d1 - self.d0);
        self.r0 + t * (self.r1 - self.r0)
    }
}

/// Band scale for categories · inner padding fixed at 0.28 (band) / 0.14 (edge).
#[derive(Debug, Clone, Copy)]
pub struct Band {
    n: usize,
    r0: f64,
    r1: f64,
}

impl Band {
    #[must_use]
    pub fn new(n: usize, r0: f64, r1: f64) -> Self {
        Self {
            n: n.max(1),
            r0,
            r1,
        }
    }

    fn step(&self) -> f64 {
        (self.r1 - self.r0) / (self.n as f64)
    }

    /// (x, width) of band `i`'s bar slot.
    #[must_use]
    pub fn slot(&self, i: usize) -> (f64, f64) {
        let step = self.step();
        let pad = step * 0.14;
        let x = self.r0 + step * (i as f64) + pad;
        (x, step - 2.0 * pad)
    }

    /// Center x of band `i` (labels · line-over-categories).
    #[must_use]
    pub fn center(&self, i: usize) -> f64 {
        self.r0 + self.step() * (i as f64 + 0.5)
    }

    /// Full step width (label truncation budget).
    #[must_use]
    pub fn step_width(&self) -> f64 {
        self.step()
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
    fn linear_maps_and_inverts_range() {
        let s = Linear::new(0.0, 10.0, 200.0, 0.0);
        assert_eq!(s.map(0.0), 200.0);
        assert_eq!(s.map(10.0), 0.0);
        assert_eq!(s.map(5.0), 100.0);
    }

    #[test]
    fn band_slots_cover_range() {
        let b = Band::new(4, 0.0, 400.0);
        let (x0, w) = b.slot(0);
        let (x3, _) = b.slot(3);
        assert!(x0 > 0.0 && w > 0.0);
        assert!(x3 + w < 400.0 + 1e-9);
        assert_eq!(b.center(0), 50.0);
    }
}
