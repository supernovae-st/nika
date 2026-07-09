//! The op implementations. Each op is `PixBuf → PixBuf` (ascii excepted —
//! it terminates the pipeline with a text/raster artifact, handled in lib.rs).

pub mod ascii;
pub mod dither;
pub mod duotone;
pub mod geometry;
pub mod glitch;
pub mod halftone;
pub mod palette;
pub mod retro;
pub mod tone;

use crate::args::Op;
use crate::det::Pcg32;
use crate::error::FxError;
use crate::pix::PixBuf;

/// Apply one non-terminal op. Ascii is rejected here (lib.rs owns it).
///
/// # Errors
/// Propagates the op's typed rejection (`FxError::Args` for bounds ·
/// `FxError::Budget` when a resize would exceed the pixel budget).
pub fn apply(img: &PixBuf, op: &Op, rng: &mut Pcg32, seed: u64) -> Result<PixBuf, FxError> {
    match op {
        Op::Resize {
            width,
            height,
            filter,
        } => geometry::resize(img, *width, *height, *filter),
        Op::Crop {
            x,
            y,
            width,
            height,
        } => geometry::crop(img, *x, *y, *width, *height),
        Op::Levels {
            brightness,
            contrast,
        } => Ok(tone::levels(img, *brightness, *contrast)),
        Op::Grayscale => Ok(tone::grayscale(img)),
        Op::PaletteMap { palette } => Ok(palette::palette_map(img, palette)),
        Op::Dither { mode, palette } => Ok(dither::dither(img, *mode, palette)),
        Op::Duotone { dark, light } => Ok(duotone::duotone(img, *dark, *light)),
        Op::Pixelate { block } => Ok(tone::pixelate(img, *block)),
        Op::Halftone { cell, angle } => Ok(halftone::halftone(img, *cell, *angle)),
        Op::Grain { intensity } => Ok(retro::grain(img, *intensity, seed)),
        Op::Vignette { strength } => Ok(retro::vignette(img, *strength)),
        Op::ChromaticAberration { shift } => Ok(retro::chromatic_aberration(img, *shift)),
        Op::Scanlines { strength, period } => Ok(retro::scanlines(img, *strength, *period)),
        Op::Glitch {
            line_shift,
            channel_shift,
            blocks,
        } => Ok(glitch::glitch(
            img,
            *line_shift,
            *channel_shift,
            *blocks,
            rng,
        )),
        Op::Ascii { .. } => Err(FxError::Args(
            "ascii is terminal — handled by the pipeline driver".into(),
        )),
    }
}
