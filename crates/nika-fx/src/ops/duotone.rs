//! Duotone / two-stop gradient map — the ramp is interpolated in Oklab
//! (perceptually even) and indexed by linear-light luma.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]
// Cast law: every `as` in this file is bounded by construction (pixel coords
// <= 16384 · channel values <= 8192 Q13 · fixed-point <= 2^32) — the pixel-math
// cast class documented in the crate docs. Truncation/sign paths are unreachable.
use crate::det::{linear_luma_q13, oklab_q12_to_srgb, srgb_to_oklab_q12};
use crate::pix::PixBuf;

#[must_use]
pub fn duotone(img: &PixBuf, dark: [u8; 3], light: [u8; 3]) -> PixBuf {
    let a = srgb_to_oklab_q12(dark);
    let b = srgb_to_oklab_q12(light);
    let mut ramp = [[0u8; 3]; 256];
    for (t, e) in ramp.iter_mut().enumerate() {
        let ti = t as i32;
        let lab = [
            a[0] + (b[0] - a[0]) * ti / 255,
            a[1] + (b[1] - a[1]) * ti / 255,
            a[2] + (b[2] - a[2]) * ti / 255,
        ];
        *e = oklab_q12_to_srgb(lab);
    }
    let mut out = PixBuf::new(img.w, img.h);
    for y in 0..img.h {
        for x in 0..img.w {
            let luma = linear_luma_q13(img.get(x, y));
            let idx = ((luma * 255 + 4096) / 8192).min(255) as usize;
            out.set(x, y, ramp[idx]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duotone_endpoints_map_to_stops() {
        let mut img = PixBuf::new(2, 1);
        img.set(0, 0, [0, 0, 0]);
        img.set(1, 0, [255, 255, 255]);
        let out = duotone(&img, [20, 10, 60], [240, 230, 200]);
        let d = out.get(0, 0);
        let l = out.get(1, 0);
        // Endpoints round-trip through Oklab within a few counts.
        for c in 0..3 {
            assert!((i32::from(d[c]) - [20, 10, 60][c]).abs() <= 4, "{d:?}");
            assert!((i32::from(l[c]) - [240, 230, 200][c]).abs() <= 4, "{l:?}");
        }
    }
}
