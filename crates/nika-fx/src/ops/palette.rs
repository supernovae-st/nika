//! Palette machinery — Oklab-distance nearest mapping (Euclid per §12-B).

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]
// Cast law: every `as` in this file is bounded by construction (pixel coords
// <= 16384 · channel values <= 8192 Q13 · fixed-point <= 2^32) — the pixel-math
// cast class documented in the crate docs. Truncation/sign paths are unreachable.
use crate::args::Palette;
use crate::det::{oklab_dist_sq, srgb_to_oklab_q12};
use crate::pix::PixBuf;

/// Palette with precomputed Oklab coordinates (one conversion per entry).
pub struct LabPalette {
    pub colors: Vec<[u8; 3]>,
    labs: Vec<[i32; 3]>,
}

impl LabPalette {
    #[must_use]
    pub fn new(palette: &Palette) -> LabPalette {
        let colors = palette.colors();
        let labs = colors.iter().map(|c| srgb_to_oklab_q12(*c)).collect();
        LabPalette { colors, labs }
    }

    /// Nearest entry by squared Oklab distance — stable first-wins ties.
    #[inline]
    #[must_use]
    pub fn nearest(&self, p: [u8; 3]) -> usize {
        let lab = srgb_to_oklab_q12(p);
        let mut best = 0usize;
        let mut bd = i64::MAX;
        for (i, pl) in self.labs.iter().enumerate() {
            let d = oklab_dist_sq(lab, *pl);
            if d < bd {
                bd = d;
                best = i;
            }
        }
        best
    }
}

#[must_use]
pub fn palette_map(img: &PixBuf, palette: &Palette) -> PixBuf {
    let pal = LabPalette::new(palette);
    let mut out = PixBuf::new(img.w, img.h);
    for y in 0..img.h {
        for x in 0..img.w {
            out.set(x, y, pal.colors[pal.nearest(img.get(x, y))]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::PresetPalette;

    #[test]
    fn nearest_picks_perceptual_neighbor() {
        let pal = LabPalette::new(&Palette::Preset(PresetPalette::Cga));
        // Dark navy → black, not magenta (the Oklab win over naive RGB).
        assert_eq!(pal.colors[pal.nearest([10, 10, 60])], [0, 0, 0]);
        // Near-white stays white.
        assert_eq!(pal.colors[pal.nearest([250, 250, 245])], [255, 255, 255]);
    }

    #[test]
    fn palette_map_only_emits_palette_colors() {
        let mut img = PixBuf::new(8, 8);
        for y in 0..8 {
            for x in 0..8 {
                img.set(x, y, [(x * 30) as u8, (y * 30) as u8, 120]);
            }
        }
        let pal = Palette::Preset(PresetPalette::Gameboy);
        let mapped = palette_map(&img, &pal);
        let allowed = pal.colors();
        for p in mapped.pixels() {
            assert!(allowed.contains(p));
        }
    }
}
