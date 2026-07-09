//! Resize (linear-light law · §2bis-8) + crop.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]
// Cast law: every `as` in this file is bounded by construction (pixel coords
// <= 16384 · channel values <= 8192 Q13 · fixed-point <= 2^32) — the pixel-math
// cast class documented in the crate docs. Truncation/sign paths are unreachable.
use crate::args::ResizeFilter;
use crate::det::{linear_to_srgb, srgb_to_linear};
use crate::error::FxError;
use crate::pix::PixBuf;

fn target_dims(
    w: usize,
    h: usize,
    width: Option<u32>,
    height: Option<u32>,
) -> Result<(usize, usize), FxError> {
    let (tw, th) = match (width, height) {
        (Some(tw), Some(th)) => (tw as usize, th as usize),
        (Some(tw), None) => {
            let th = ((h as u64 * u64::from(tw) + (w as u64 / 2)) / w as u64).max(1) as usize;
            (tw as usize, th)
        }
        (None, Some(th)) => {
            let tw = ((w as u64 * u64::from(th) + (h as u64 / 2)) / h as u64).max(1) as usize;
            (tw, th as usize)
        }
        (None, None) => return Err(FxError::Args("resize needs a dimension".into())),
    };
    if tw as u64 * th as u64 > crate::codec::png::MAX_PIXELS {
        return Err(FxError::Budget {
            pixels: tw as u64 * th as u64,
            max: crate::codec::png::MAX_PIXELS,
        });
    }
    Ok((tw, th))
}

/// # Errors
/// `FxError::Args` when no dimension is given · `FxError::Budget` when the
/// target exceeds the pixel budget.
pub fn resize(
    img: &PixBuf,
    width: Option<u32>,
    height: Option<u32>,
    filter: ResizeFilter,
) -> Result<PixBuf, FxError> {
    let (tw, th) = target_dims(img.w, img.h, width, height)?;
    if tw == img.w && th == img.h {
        return Ok(img.clone());
    }
    match filter {
        ResizeFilter::Nearest => Ok(nearest(img, tw, th)),
        ResizeFilter::Bilinear => Ok(bilinear(img, tw, th)),
        ResizeFilter::Auto => {
            // Iterated 2× box halving in linear light while ≥2× above target,
            // then one bilinear pass — the deterministic anti-alias ladder.
            let mut cur = img.clone();
            while cur.w >= tw * 2 && cur.h >= th * 2 {
                cur = halve(&cur);
            }
            if cur.w == tw && cur.h == th {
                Ok(cur)
            } else {
                Ok(bilinear(&cur, tw, th))
            }
        }
    }
}

fn nearest(img: &PixBuf, tw: usize, th: usize) -> PixBuf {
    let mut out = PixBuf::new(tw, th);
    for y in 0..th {
        let sy = (y as u64 * img.h as u64 / th as u64) as usize;
        for x in 0..tw {
            let sx = (x as u64 * img.w as u64 / tw as u64) as usize;
            out.set(x, y, img.get(sx, sy));
        }
    }
    out
}

/// 2×2 box average in linear light — one halving step of the Auto ladder.
fn halve(img: &PixBuf) -> PixBuf {
    let (tw, th) = (img.w / 2, img.h / 2);
    let mut out = PixBuf::new(tw, th);
    for y in 0..th {
        for x in 0..tw {
            let mut acc = [0u32; 3];
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let p = img.get(2 * x + dx, 2 * y + dy);
                for c in 0..3 {
                    acc[c] += u32::from(srgb_to_linear(p[c]));
                }
            }
            let px = [
                linear_to_srgb(((acc[0] + 2) / 4) as u16),
                linear_to_srgb(((acc[1] + 2) / 4) as u16),
                linear_to_srgb(((acc[2] + 2) / 4) as u16),
            ];
            out.set(x, y, px);
        }
    }
    out
}

/// Center-aligned 16.16 bilinear in linear light (half-pixel convention
/// pinned by the goldens).
fn bilinear(img: &PixBuf, tw: usize, th: usize) -> PixBuf {
    let mut out = PixBuf::new(tw, th);
    for y in 0..th {
        let (sy, fy) = src_coord(y, th, img.h);
        for x in 0..tw {
            let (sx, fx) = src_coord(x, tw, img.w);
            let p00 = img.get(sx, sy);
            let p10 = img.get((sx + 1).min(img.w - 1), sy);
            let p01 = img.get(sx, (sy + 1).min(img.h - 1));
            let p11 = img.get((sx + 1).min(img.w - 1), (sy + 1).min(img.h - 1));
            let mut px = [0u8; 3];
            for c in 0..3 {
                let a = lerp_q16(
                    u64::from(srgb_to_linear(p00[c])),
                    u64::from(srgb_to_linear(p10[c])),
                    fx,
                );
                let b = lerp_q16(
                    u64::from(srgb_to_linear(p01[c])),
                    u64::from(srgb_to_linear(p11[c])),
                    fx,
                );
                px[c] = linear_to_srgb(lerp_q16(a, b, fy) as u16);
            }
            out.set(x, y, px);
        }
    }
    out
}

#[inline]
fn lerp_q16(a: u64, b: u64, f: u64) -> u64 {
    (a * (65536 - f) + b * f + 32768) >> 16
}

/// Map dst index → (src floor index, Q16 fraction), center-aligned.
#[inline]
fn src_coord(d: usize, dn: usize, sn: usize) -> (usize, u64) {
    let q = (((2 * d as i64 + 1) * (sn as i64)) << 15) / dn as i64 - 32768;
    let q = q.max(0);
    let ix = (q >> 16) as usize;
    if ix >= sn - 1 {
        (sn - 1, 0)
    } else {
        (ix, (q & 0xFFFF) as u64)
    }
}

/// # Errors
/// `FxError::Args` when the rectangle exceeds the image bounds.
pub fn crop(img: &PixBuf, x: u32, y: u32, width: u32, height: u32) -> Result<PixBuf, FxError> {
    let (x, y, cw, ch) = (x as usize, y as usize, width as usize, height as usize);
    if x + cw > img.w || y + ch > img.h {
        return Err(FxError::Args(format!(
            "crop {cw}x{ch}+{x}+{y} exceeds image {}x{}",
            img.w, img.h
        )));
    }
    let mut out = PixBuf::new(cw, ch);
    for oy in 0..ch {
        for ox in 0..cw {
            out.set(ox, oy, img.get(x + ox, y + oy));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grad(w: usize, h: usize) -> PixBuf {
        let mut img = PixBuf::new(w, h);
        for y in 0..h {
            for x in 0..w {
                img.set(
                    x,
                    y,
                    [(x * 255 / (w - 1)) as u8, (y * 255 / (h - 1)) as u8, 99],
                );
            }
        }
        img
    }

    #[test]
    fn resize_dims_and_determinism() {
        let img = grad(100, 50);
        let a = resize(&img, Some(40), None, ResizeFilter::Auto).unwrap();
        assert_eq!((a.w, a.h), (40, 20));
        let b = resize(&img, Some(40), None, ResizeFilter::Auto).unwrap();
        assert_eq!(a.pixels(), b.pixels());
    }

    #[test]
    fn nearest_integer_upscale_is_exact_blocks() {
        let mut img = PixBuf::new(2, 1);
        img.set(0, 0, [10, 20, 30]);
        img.set(1, 0, [200, 210, 220]);
        let up = resize(&img, Some(4), Some(2), ResizeFilter::Nearest).unwrap();
        assert_eq!(up.get(0, 0), [10, 20, 30]);
        assert_eq!(up.get(1, 1), [10, 20, 30]);
        assert_eq!(up.get(2, 0), [200, 210, 220]);
        assert_eq!(up.get(3, 1), [200, 210, 220]);
    }

    #[test]
    fn crop_bounds_checked() {
        let img = grad(10, 10);
        assert!(crop(&img, 8, 8, 4, 4).is_err());
        let c = crop(&img, 2, 3, 4, 5).unwrap();
        assert_eq!((c.w, c.h), (4, 5));
        assert_eq!(c.get(0, 0), img.get(2, 3));
    }
}
