//! Dithering — ordered (Bayer · blue-noise · IGN) + error diffusion
//! (Floyd-Steinberg · Atkinson · JJN, serpentine). All integer. Error
//! diffusion runs in gamma space (the classic look); ordered bias too.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]
// Cast law: every `as` in this file is bounded by construction (pixel coords
#![allow(clippy::many_single_char_names)] // r/g/b · x/y · a/b/c (Paeth) ARE the domain vocabulary
// <= 16384 · channel values <= 8192 Q13 · fixed-point <= 2^32) — the pixel-math
// cast class documented in the crate docs. Truncation/sign paths are unreachable.
use crate::args::{DitherMode, Palette};
use crate::det::ign_threshold;
use crate::ops::palette::LabPalette;
use crate::pix::PixBuf;
use crate::tables;

/// Ordered bias amplitude (±spread/2 per channel) — a pinned house constant.
const SPREAD: i32 = 72;

/// Threshold byte 0..=255 for the ordered modes — the unbiased comparator
/// form ((2m+1)/2n²) folded to a byte.
#[inline]
fn ordered_threshold(mode: DitherMode, x: usize, y: usize) -> u8 {
    match mode {
        DitherMode::Bayer2 => bayer_t(&tables::BAYER2, 2, x, y),
        DitherMode::Bayer4 => bayer_t(&tables::BAYER4, 4, x, y),
        DitherMode::Bayer8 => bayer_t(&tables::BAYER8, 8, x, y),
        DitherMode::BlueNoise => {
            let r = u32::from(tables::BLUE_NOISE_64[(y % 64) * 64 + (x % 64)]);
            (((2 * r + 1) * 255) / 8192) as u8
        }
        DitherMode::Ign => ign_threshold(x as u32, y as u32),
        _ => 128,
    }
}

#[inline]
fn bayer_t(m: &[u16], n: usize, x: usize, y: usize) -> u8 {
    let v = u32::from(m[(y % n) * n + (x % n)]);
    (((2 * v + 1) * 255) / (2 * (n * n) as u32)) as u8
}

#[must_use]
pub fn dither(img: &PixBuf, mode: DitherMode, palette: &Palette) -> PixBuf {
    let pal = LabPalette::new(palette);
    match mode {
        DitherMode::FloydSteinberg => diffuse(img, &pal, &FS, 16),
        DitherMode::Atkinson => diffuse(img, &pal, &ATKINSON, 8),
        DitherMode::Jjn => diffuse(img, &pal, &JJN, 48),
        _ => ordered(img, mode, &pal),
    }
}

fn ordered(img: &PixBuf, mode: DitherMode, pal: &LabPalette) -> PixBuf {
    let mut out = PixBuf::new(img.w, img.h);
    for y in 0..img.h {
        for x in 0..img.w {
            let t = i32::from(ordered_threshold(mode, x, y));
            let bias = ((t - 127) * SPREAD) / 255;
            let p = img.get(x, y);
            let q = [
                (i32::from(p[0]) + bias).clamp(0, 255) as u8,
                (i32::from(p[1]) + bias).clamp(0, 255) as u8,
                (i32::from(p[2]) + bias).clamp(0, 255) as u8,
            ];
            out.set(x, y, pal.colors[pal.nearest(q)]);
        }
    }
    out
}

/// Kernel entry: (dx-forward, dy-down, weight). dx is mirrored on
/// right-to-left rows (serpentine — part of the byte contract).
type Kernel = [(i32, i32, i32)];

const FS: [(i32, i32, i32); 4] = [(1, 0, 7), (-1, 1, 3), (0, 1, 5), (1, 1, 1)];
const ATKINSON: [(i32, i32, i32); 6] = [
    (1, 0, 1),
    (2, 0, 1),
    (-1, 1, 1),
    (0, 1, 1),
    (1, 1, 1),
    (0, 2, 1),
];
const JJN: [(i32, i32, i32); 12] = [
    (1, 0, 7),
    (2, 0, 5),
    (-2, 1, 3),
    (-1, 1, 5),
    (0, 1, 7),
    (1, 1, 5),
    (2, 1, 3),
    (-2, 2, 1),
    (-1, 2, 3),
    (0, 2, 5),
    (1, 2, 3),
    (2, 2, 1),
];

/// Serpentine error diffusion with 3 rolling i32 error rows (JJN reaches
/// dy=2). Errors accumulate raw (err × weight), divided once at spill —
/// the division rounding (truncation toward zero) is part of the contract.
fn diffuse(img: &PixBuf, pal: &LabPalette, kernel: &Kernel, div: i32) -> PixBuf {
    let (w, h) = (img.w, img.h);
    let mut out = PixBuf::new(w, h);
    let mut rows: [Vec<[i32; 3]>; 3] = [vec![[0; 3]; w], vec![[0; 3]; w], vec![[0; 3]; w]];
    for y in 0..h {
        let ltr = y % 2 == 0;
        for i in 0..w {
            let x = if ltr { i } else { w - 1 - i };
            let p = img.get(x, y);
            let e = rows[0][x];
            let q = [
                (i32::from(p[0]) + e[0]).clamp(0, 255) as u8,
                (i32::from(p[1]) + e[1]).clamp(0, 255) as u8,
                (i32::from(p[2]) + e[2]).clamp(0, 255) as u8,
            ];
            let chosen = pal.colors[pal.nearest(q)];
            out.set(x, y, chosen);
            let err = [
                i32::from(q[0]) - i32::from(chosen[0]),
                i32::from(q[1]) - i32::from(chosen[1]),
                i32::from(q[2]) - i32::from(chosen[2]),
            ];
            for &(dx, dy, wt) in kernel {
                let nx = x as i32 + if ltr { dx } else { -dx };
                if nx < 0 || nx >= w as i32 {
                    continue;
                }
                let row = &mut rows[dy as usize];
                let cell = &mut row[nx as usize];
                for c in 0..3 {
                    cell[c] += err[c] * wt / div;
                }
            }
        }
        rows.rotate_left(1);
        rows[2] = vec![[0; 3]; w];
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap
    )]

    use super::*;
    use crate::args::PresetPalette;

    fn gradient(w: usize, h: usize) -> PixBuf {
        let mut img = PixBuf::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let g = (x * 255 / (w - 1)) as u8;
                img.set(x, y, [g, g, g]);
            }
        }
        img
    }

    #[test]
    fn ordered_bw_gradient_density_tracks_luma() {
        let img = gradient(256, 16);
        let out = dither(
            &img,
            DitherMode::Bayer4,
            &Palette::Preset(PresetPalette::Bw),
        );
        let white_left = count_white(&out, 0, 64);
        let white_right = count_white(&out, 192, 256);
        assert!(white_left < white_right, "{white_left} !< {white_right}");
    }

    fn count_white(img: &PixBuf, x0: usize, x1: usize) -> usize {
        let mut n = 0;
        for y in 0..img.h {
            for x in x0..x1 {
                if img.get(x, y) == [255, 255, 255] {
                    n += 1;
                }
            }
        }
        n
    }

    #[test]
    fn diffusion_mean_preserves_tone() {
        // 50% gray through FS on bw must land near 50% white coverage.
        let mut img = PixBuf::new(64, 64);
        for p in img.pixels_mut() {
            // ~50% linear gray in sRGB is 188; use gamma-space 128 and accept
            // the gamma-space classic look: coverage should sit near 128/255.
            *p = [128, 128, 128];
        }
        let out = dither(
            &img,
            DitherMode::FloydSteinberg,
            &Palette::Preset(PresetPalette::Bw),
        );
        let white = count_white(&out, 0, 64);
        let ratio = white as f64 / (64.0 * 64.0);
        assert!((0.40..=0.60).contains(&ratio), "coverage {ratio}");
    }

    #[test]
    fn all_modes_deterministic_and_palette_pure() {
        let img = gradient(50, 20);
        let pal = Palette::Preset(PresetPalette::Gameboy);
        for mode in [
            DitherMode::Bayer2,
            DitherMode::Bayer8,
            DitherMode::BlueNoise,
            DitherMode::Ign,
            DitherMode::Atkinson,
            DitherMode::Jjn,
        ] {
            let a = dither(&img, mode, &pal);
            let b = dither(&img, mode, &pal);
            assert_eq!(a.pixels(), b.pixels());
            let allowed = pal.colors();
            for p in a.pixels() {
                assert!(allowed.contains(p));
            }
        }
    }
}
