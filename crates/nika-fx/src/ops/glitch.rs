//! Seeded databending — row shifts · channel offset · block swaps.
//! One PCG32 stream, draws in documented order: rows top→bottom, then the
//! channel offset, then `blocks` rect swaps. The seed IS the composition.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]
// Cast law: every `as` in this file is bounded by construction (pixel coords
// <= 16384 · channel values <= 8192 Q13 · fixed-point <= 2^32) — the pixel-math
// cast class documented in the crate docs. Truncation/sign paths are unreachable.
use crate::det::Pcg32;
use crate::pix::PixBuf;

pub fn glitch(
    img: &PixBuf,
    line_shift: u32,
    channel_shift: u32,
    blocks: u32,
    rng: &mut Pcg32,
) -> PixBuf {
    let mut out = img.clone();
    if line_shift > 0 {
        shift_rows(&mut out, line_shift, rng);
    }
    if channel_shift > 0 {
        offset_channel(&mut out, channel_shift, rng);
    }
    if blocks > 0 {
        swap_blocks(&mut out, blocks, rng);
    }
    out
}

fn shift_rows(img: &mut PixBuf, amount: u32, rng: &mut Pcg32) {
    let w = img.w;
    let mut row = vec![[0u8; 3]; w];
    for y in 0..img.h {
        // ~1 row in 4 glitches — the classic broken-scan cadence.
        if rng.below(4) != 0 {
            continue;
        }
        let off = (i64::from(rng.below(2 * amount + 1)) - i64::from(amount)).rem_euclid(w as i64)
            as usize;
        if off == 0 {
            continue;
        }
        for (x, slot) in row.iter_mut().enumerate() {
            *slot = img.get(x, y);
        }
        for x in 0..w {
            img.set(x, y, row[(x + off) % w]);
        }
    }
}

fn offset_channel(img: &mut PixBuf, amount: u32, rng: &mut Pcg32) {
    let dx = 1 + rng.below(amount) as usize;
    let snapshot = img.clone();
    for y in 0..img.h {
        for x in 0..img.w {
            let mut p = img.get(x, y);
            p[0] = snapshot.get((x + dx) % img.w, y)[0]; // R pulled left
            p[2] = snapshot.get((x + img.w - dx % img.w) % img.w, y)[2]; // B right
            img.set(x, y, p);
        }
    }
}

fn swap_blocks(img: &mut PixBuf, blocks: u32, rng: &mut Pcg32) {
    let snapshot = img.clone();
    for _ in 0..blocks {
        let bw = (img.w / 8)
            .max(1)
            .min(1 + rng.below((img.w as u32 / 4).max(1)) as usize);
        let bh = (img.h / 16)
            .max(1)
            .min(1 + rng.below((img.h as u32 / 8).max(1)) as usize);
        let sx = rng.below((img.w - bw + 1) as u32) as usize;
        let sy = rng.below((img.h - bh + 1) as u32) as usize;
        let dx = rng.below((img.w - bw + 1) as u32) as usize;
        let dy = rng.below((img.h - bh + 1) as u32) as usize;
        for y in 0..bh {
            for x in 0..bw {
                img.set(dx + x, dy + y, snapshot.get(sx + x, sy + y));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(w: usize, h: usize) -> PixBuf {
        let mut img = PixBuf::new(w, h);
        for y in 0..h {
            for x in 0..w {
                img.set(x, y, [(x * 7 % 256) as u8, (y * 11 % 256) as u8, 77]);
            }
        }
        img
    }

    #[test]
    fn glitch_same_seed_same_bytes() {
        let img = card(40, 30);
        let mut r1 = Pcg32::new(9);
        let mut r2 = Pcg32::new(9);
        let a = glitch(&img, 8, 4, 3, &mut r1);
        let b = glitch(&img, 8, 4, 3, &mut r2);
        assert_eq!(a.pixels(), b.pixels());
        let mut r3 = Pcg32::new(10);
        let c = glitch(&img, 8, 4, 3, &mut r3);
        assert_ne!(a.pixels(), c.pixels());
    }

    #[test]
    fn glitch_actually_mutates() {
        let img = card(40, 30);
        let mut rng = Pcg32::new(1);
        let g = glitch(&img, 8, 4, 3, &mut rng);
        assert_ne!(g.pixels(), img.pixels());
    }
}
