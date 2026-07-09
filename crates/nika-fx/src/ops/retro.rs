//! The retro pack — grain (AV1-shaped) · vignette (rational cos⁴ · zero
//! trig) · chromatic aberration (per-channel radial) · scanlines.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]
// Cast law: every `as` in this file is bounded by construction (pixel coords
// <= 16384 · channel values <= 8192 Q13 · fixed-point <= 2^32) — the pixel-math
// cast class documented in the crate docs. Truncation/sign paths are unreachable.
use crate::det::{gamma_luma, pixel_hash};
use crate::pix::PixBuf;

/// Luminance-shaped mono grain: stateless per-pixel hash noise scaled by a
/// midtone-humped strength curve (the AV1 §7.18 shape, one-LUT edition).
#[must_use]
pub fn grain(img: &PixBuf, intensity: u32, seed: u64) -> PixBuf {
    let mut out = PixBuf::new(img.w, img.h);
    let k = intensity as i32;
    for y in 0..img.h {
        for x in 0..img.w {
            let p = img.get(x, y);
            let luma = i32::from(gamma_luma(p));
            let hump = luma * (255 - luma) / 255; // 0..=63 peak at midtones
            let n = (pixel_hash(x as u32, y as u32, seed) & 0xFF) as i32 - 128;
            let delta = (n * hump * k) >> 13; // /(128·64)
            let px = [
                (i32::from(p[0]) + delta).clamp(0, 255) as u8,
                (i32::from(p[1]) + delta).clamp(0, 255) as u8,
                (i32::from(p[2]) + delta).clamp(0, 255) as u8,
            ];
            out.set(x, y, px);
        }
    }
    out
}

/// Natural cos⁴ falloff — cos⁴θ = 1/(1+ρ²)², a pure rational in Q16.
#[must_use]
pub fn vignette(img: &PixBuf, strength: u32) -> PixBuf {
    let mut out = PixBuf::new(img.w, img.h);
    let (w, h) = (img.w as i64, img.h as i64);
    let diag2 = w * w + h * h;
    for y in 0..img.h {
        for x in 0..img.w {
            // Offsets in half-pixel units from the exact center.
            let dx = 2 * x as i64 + 1 - w;
            let dy = 2 * y as i64 + 1 - h;
            let rho2_q16 = ((dx * dx + dy * dy) << 16) / diag2; // 0..~2^16
            let den_q16 = ((65536 + rho2_q16) * (65536 + rho2_q16)) >> 16;
            let gain_q16 = (1i64 << 32) / den_q16;
            let eff = 65536 - (i64::from(strength) * (65536 - gain_q16)) / 255;
            let p = img.get(x, y);
            let px = [
                ((i64::from(p[0]) * eff) >> 16) as u8,
                ((i64::from(p[1]) * eff) >> 16) as u8,
                ((i64::from(p[2]) * eff) >> 16) as u8,
            ];
            out.set(x, y, px);
        }
    }
    out
}

/// Lateral chromatic aberration: R sampled magnified, B minified, G kept.
/// `shift` = pixel offset at the frame edge. Nearest taps (stylistic op).
#[must_use]
pub fn chromatic_aberration(img: &PixBuf, shift: u32) -> PixBuf {
    let mut out = PixBuf::new(img.w, img.h);
    let max_dim = img.w.max(img.h) as i64;
    let k_q16 = (i64::from(shift) << 17) / max_dim; // shift px at edge
    let (cx, cy) = (img.w as i64 - 1, img.h as i64 - 1); // ×2 center form
    for y in 0..img.h {
        for x in 0..img.w {
            let dx2 = 2 * x as i64 - cx; // ×2 offsets keep integers exact
            let dy2 = 2 * y as i64 - cy;
            let sample = |scale_q16: i64, chan: usize| -> u8 {
                let sx = i64::midpoint(cx, (dx2 * scale_q16) >> 16);
                let sy = i64::midpoint(cy, (dy2 * scale_q16) >> 16);
                img.get_clamped(sx, sy)[chan]
            };
            let r = sample(65536 - k_q16, 0);
            let g = img.get(x, y)[1];
            let b = sample(65536 + k_q16, 2);
            out.set(x, y, [r, g, b]);
        }
    }
    out
}

/// Dark line every `period` rows, smooth integer falloff over half a band.
#[must_use]
pub fn scanlines(img: &PixBuf, strength: u32, period: u32) -> PixBuf {
    let mut out = PixBuf::new(img.w, img.h);
    let p = period as usize;
    for y in 0..img.h {
        let pos = y % p;
        let t = (pos * 512 / p) as i32; // 0..512 across the band
        let darkness = (256 - t).max(0) as u32; // line at band start, fades
        let dim = (strength * darkness) >> 8; // 0..=255
        let mul = 255 - dim.min(255);
        for x in 0..img.w {
            let px = img.get(x, y);
            out.set(
                x,
                y,
                [
                    ((u32::from(px[0]) * mul) / 255) as u8,
                    ((u32::from(px[1]) * mul) / 255) as u8,
                    ((u32::from(px[2]) * mul) / 255) as u8,
                ],
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(v: u8, w: usize, h: usize) -> PixBuf {
        let mut img = PixBuf::new(w, h);
        for p in img.pixels_mut() {
            *p = [v, v, v];
        }
        img
    }

    #[test]
    fn grain_is_seed_deterministic_and_midtone_shaped() {
        let img = flat(128, 32, 32);
        let a = grain(&img, 100, 42);
        let b = grain(&img, 100, 42);
        assert_eq!(a.pixels(), b.pixels());
        let c = grain(&img, 100, 43);
        assert_ne!(a.pixels(), c.pixels());
        // Blacks stay untouched (hump = 0).
        let dark = grain(&flat(0, 8, 8), 128, 42);
        assert!(dark.pixels().iter().all(|p| *p == [0, 0, 0]));
    }

    #[test]
    fn vignette_darkens_corners_not_center() {
        let img = flat(200, 64, 64);
        let v = vignette(&img, 255);
        let center = v.get(32, 32)[0];
        let corner = v.get(0, 0)[0];
        assert!(center > corner, "center {center} corner {corner}");
        assert!(center >= 195, "center barely touched, got {center}");
    }

    #[test]
    fn scanlines_dim_first_row_of_each_band() {
        let img = flat(200, 8, 8);
        let s = scanlines(&img, 255, 4);
        assert!(s.get(0, 0)[0] < 60);
        assert_eq!(s.get(0, 3)[0], 200); // far from the line
    }

    #[test]
    fn chromatic_aberration_center_stable_edges_split() {
        // Edge OFF-center — radial CA is zero at the exact center by
        // construction, so the step lives at x=16 where displacement is real.
        let mut img = PixBuf::new(65, 9);
        for y in 0..9 {
            for x in 0..65 {
                let v = if x >= 16 { 255 } else { 0 };
                img.set(x, y, [v, v, v]);
            }
        }
        let out = chromatic_aberration(&img, 8);
        let mut split = false;
        for x in 6..28 {
            let p = out.get(x, 4);
            if p[0] != p[2] {
                split = true;
                break;
            }
        }
        assert!(split, "no channel split found near the off-center edge");
    }
}
