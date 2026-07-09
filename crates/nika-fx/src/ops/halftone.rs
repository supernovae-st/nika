//! Clustered-dot rotated-screen halftone — black ink on white, print class.
//! Rotation via full per-pixel i64 products (never incremental — the §12-B
//! zero-drift law). Angles are a closed enum with Q16 constants re-derived
//! in tests (runtime trig is banned).

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]
// Cast law: every `as` in this file is bounded by construction (pixel coords
// <= 16384 · channel values <= 8192 Q13 · fixed-point <= 2^32) — the pixel-math
// cast class documented in the crate docs. Truncation/sign paths are unreachable.
use crate::args::ScreenAngle;
use crate::det::gamma_luma;
use crate::pix::PixBuf;

/// (cos·2^16, sin·2^16) — re-derived in tests against std trig.
fn angle_q16(a: ScreenAngle) -> (i64, i64) {
    match a {
        ScreenAngle::A0 => (65536, 0),
        ScreenAngle::A15 => (63303, 16962),
        ScreenAngle::A45 => (46341, 46341),
        ScreenAngle::A75 => (16962, 63303),
    }
}

#[must_use]
pub fn halftone(img: &PixBuf, cell: u32, angle: ScreenAngle) -> PixBuf {
    let (cos_q, sin_q) = angle_q16(angle);
    // Screen frequency: one period per `cell` pixels (Q16/cell per pixel).
    let fa = cos_q / i64::from(cell);
    let fb = sin_q / i64::from(cell);
    let mut out = PixBuf::new(img.w, img.h);
    for y in 0..img.h {
        for x in 0..img.w {
            let (xi, yi) = (x as i64, y as i64);
            // Full products per pixel — exact at every (x, y).
            let u = xi * fa + yi * fb;
            let v = -xi * fb + yi * fa;
            let du = (u & 0xFFFF) - 32768; // −32768..32767 (cell-centered)
            let dv = (v & 0xFFFF) - 32768;
            let r2_q16 = (du * du + dv * dv) >> 16; // 0..=32768
            let z_q16 = 65536 - 2 * r2_q16; // round-dot spot: 1 center → 0 corner
            let t = ((z_q16 * 255) >> 16) as i32; // threshold byte
            let luma = i32::from(gamma_luma(img.get(x, y)));
            let ink = luma < t;
            out.set(x, y, if ink { [0, 0, 0] } else { [255, 255, 255] });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::disallowed_methods)]
    fn angle_constants_rederive() {
        for (a, deg) in [
            (ScreenAngle::A0, 0.0f64),
            (ScreenAngle::A15, 15.0),
            (ScreenAngle::A45, 45.0),
            (ScreenAngle::A75, 75.0),
        ] {
            let (c, s) = angle_q16(a);
            let rad = deg.to_radians();
            assert_eq!(c, (rad.cos() * 65536.0).round() as i64, "cos {deg}");
            assert_eq!(s, (rad.sin() * 65536.0).round() as i64, "sin {deg}");
        }
    }

    #[test]
    fn halftone_is_binary_and_tone_tracking() {
        let mut img = PixBuf::new(64, 64);
        for y in 0..64 {
            for x in 0..64 {
                let g = (x * 4) as u8; // dark → light gradient
                img.set(x, y, [g, g, g]);
            }
        }
        let out = halftone(&img, 8, ScreenAngle::A45);
        let mut ink_left = 0;
        let mut ink_right = 0;
        for y in 0..64 {
            for x in 0..64 {
                let is_ink = out.get(x, y) == [0, 0, 0];
                assert!(is_ink || out.get(x, y) == [255, 255, 255]);
                if is_ink && x < 32 {
                    ink_left += 1;
                }
                if is_ink && x >= 32 {
                    ink_right += 1;
                }
            }
        }
        assert!(ink_left > ink_right, "dark side must carry more ink");
    }
}
