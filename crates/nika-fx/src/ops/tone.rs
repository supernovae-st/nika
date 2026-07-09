//! Grayscale (linear-light) · levels (brightness/contrast) · pixelate.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]
// Cast law: every `as` in this file is bounded by construction (pixel coords
// <= 16384 · channel values <= 8192 Q13 · fixed-point <= 2^32) — the pixel-math
// cast class documented in the crate docs. Truncation/sign paths are unreachable.
use crate::det::{linear_luma_q13, linear_to_srgb, srgb_to_linear};
use crate::pix::PixBuf;

#[must_use]
pub fn grayscale(img: &PixBuf) -> PixBuf {
    let mut out = PixBuf::new(img.w, img.h);
    for y in 0..img.h {
        for x in 0..img.w {
            let g = linear_to_srgb(linear_luma_q13(img.get(x, y)) as u16);
            out.set(x, y, [g, g, g]);
        }
    }
    out
}

/// Classic gamma-space levels: brightness add, then contrast around 128
/// with the 259-formula in Q8 integer form.
#[must_use]
pub fn levels(img: &PixBuf, brightness: i32, contrast: i32) -> PixBuf {
    let f_q8 = (259 * (contrast + 255) * 256) / (255 * (259 - contrast));
    let mut lut = [0u8; 256];
    for (v, e) in lut.iter_mut().enumerate() {
        let b = v as i32 + brightness;
        let c = ((b - 128) * f_q8) / 256 + 128;
        *e = c.clamp(0, 255) as u8;
    }
    let mut out = PixBuf::new(img.w, img.h);
    for y in 0..img.h {
        for x in 0..img.w {
            let p = img.get(x, y);
            out.set(
                x,
                y,
                [lut[p[0] as usize], lut[p[1] as usize], lut[p[2] as usize]],
            );
        }
    }
    out
}

/// Block-average mosaic in linear light (size preserved — the mosaic look).
#[must_use]
pub fn pixelate(img: &PixBuf, block: u32) -> PixBuf {
    let b = block as usize;
    let mut out = PixBuf::new(img.w, img.h);
    let mut by = 0;
    while by < img.h {
        let bh = b.min(img.h - by);
        let mut bx = 0;
        while bx < img.w {
            let bw = b.min(img.w - bx);
            let mut acc = [0u64; 3];
            for y in by..by + bh {
                for x in bx..bx + bw {
                    let p = img.get(x, y);
                    for c in 0..3 {
                        acc[c] += u64::from(srgb_to_linear(p[c]));
                    }
                }
            }
            let n = (bw * bh) as u64;
            let avg = [
                linear_to_srgb(((acc[0] + n / 2) / n) as u16),
                linear_to_srgb(((acc[1] + n / 2) / n) as u16),
                linear_to_srgb(((acc[2] + n / 2) / n) as u16),
            ];
            for y in by..by + bh {
                for x in bx..bx + bw {
                    out.set(x, y, avg);
                }
            }
            bx += b;
        }
        by += b;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grayscale_neutralizes_channels() {
        let mut img = PixBuf::new(2, 1);
        img.set(0, 0, [255, 0, 0]);
        img.set(1, 0, [128, 128, 128]);
        let g = grayscale(&img);
        let p = g.get(0, 0);
        assert_eq!(p[0], p[1]);
        assert_eq!(p[1], p[2]);
        assert_eq!(g.get(1, 0), [128, 128, 128]);
    }

    #[test]
    fn levels_identity_at_zero() {
        let mut img = PixBuf::new(1, 1);
        img.set(0, 0, [7, 130, 250]);
        assert_eq!(levels(&img, 0, 0).get(0, 0), [7, 130, 250]);
    }

    #[test]
    fn pixelate_uniform_blocks() {
        let mut img = PixBuf::new(4, 4);
        for y in 0..4 {
            for x in 0..4 {
                img.set(x, y, [(x * 60) as u8, (y * 60) as u8, 0]);
            }
        }
        let p = pixelate(&img, 2);
        assert_eq!(p.get(0, 0), p.get(1, 1));
        assert_eq!(p.get(2, 2), p.get(3, 3));
    }
}
