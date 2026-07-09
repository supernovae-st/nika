//! Typed op pipeline — the closed vocabulary of `nika:image_fx` v1.
//! The nika-builtin wiring maps the workflow's JSON args onto these types;
//! this crate stays serde-free (zero-dep law). `recipe_json` re-serializes
//! the pipeline deterministically (fixed key order) for the tEXt chunk.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]
// Cast law: every `as` in this file is bounded by construction (pixel coords
// <= 16384 · channel values <= 8192 Q13 · fixed-point <= 2^32) — the pixel-math
// cast class documented in the crate docs. Truncation/sign paths are unreachable.
use crate::error::FxError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FxArgs {
    pub ops: Vec<Op>,
    pub seed: u64,
}

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    /// Linear-light resample. One dim may be None (aspect-preserved).
    Resize {
        width: Option<u32>,
        height: Option<u32>,
        filter: ResizeFilter,
    },
    Crop {
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    },
    /// brightness −255..=255 · contrast −128..=128 (gamma lands in a minor —
    /// deterministic pow needs its own seam).
    Levels {
        brightness: i32,
        contrast: i32,
    },
    Grayscale,
    /// Map every pixel to the nearest palette color (Oklab distance).
    PaletteMap {
        palette: Palette,
    },
    /// Quantize to a palette through a dither (the core op).
    Dither {
        mode: DitherMode,
        palette: Palette,
    },
    /// Two-stop gradient map through Oklab (shadows → dark · lights → light).
    Duotone {
        dark: [u8; 3],
        light: [u8; 3],
    },
    Pixelate {
        block: u32,
    },
    /// Clustered-dot rotated screen (black ink on white) — print aesthetic.
    Halftone {
        cell: u32,
        angle: ScreenAngle,
    },
    /// Luminance-shaped film grain (AV1-shaped · seeded · midtone-humped).
    Grain {
        intensity: u32,
    },
    /// cos⁴ natural falloff (pure rational — zero trig) · strength 0..=255.
    Vignette {
        strength: u32,
    },
    /// Per-channel radial scale (R out · B in) · shift = px at the edge.
    ChromaticAberration {
        shift: u32,
    },
    /// Darken every Nth row band with a smoothstep profile.
    Scanlines {
        strength: u32,
        period: u32,
    },
    /// Seeded databending: row shifts · channel offset · block swaps.
    Glitch {
        line_shift: u32,
        channel_shift: u32,
        blocks: u32,
    },
    /// Character-grid render. MUST be the last op. Emit png|text|ansi.
    Ascii {
        cols: u32,
        emit: AsciiEmit,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeFilter {
    /// Auto = iterated 2× box halving in linear light, then bilinear.
    Auto,
    Nearest,
    Bilinear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DitherMode {
    Bayer2,
    Bayer4,
    Bayer8,
    BlueNoise,
    Ign,
    FloydSteinberg,
    Atkinson,
    Jjn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenAngle {
    A0,
    A15,
    A45,
    A75,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsciiEmit {
    Png,
    Text,
    Ansi,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Palette {
    Preset(PresetPalette),
    Custom(Vec<[u8; 3]>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetPalette {
    Bw,
    Gray4,
    Gameboy,
    Cga,
    OkabeIto,
}

impl PresetPalette {
    #[must_use]
    pub fn colors(self) -> &'static [[u8; 3]] {
        match self {
            PresetPalette::Bw => &[[0, 0, 0], [255, 255, 255]],
            PresetPalette::Gray4 => &[[0, 0, 0], [85, 85, 85], [170, 170, 170], [255, 255, 255]],
            PresetPalette::Gameboy => &[[15, 56, 15], [48, 98, 48], [139, 172, 15], [155, 188, 15]],
            PresetPalette::Cga => &[[0, 0, 0], [85, 255, 255], [255, 85, 255], [255, 255, 255]],
            // Okabe-Ito 8 (the colorblind-safe categorical canon).
            PresetPalette::OkabeIto => &[
                [0, 0, 0],
                [230, 159, 0],
                [86, 180, 233],
                [0, 158, 115],
                [240, 228, 66],
                [0, 114, 178],
                [213, 94, 0],
                [204, 121, 167],
            ],
        }
    }

    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            PresetPalette::Bw => "bw",
            PresetPalette::Gray4 => "gray4",
            PresetPalette::Gameboy => "gameboy",
            PresetPalette::Cga => "cga",
            PresetPalette::OkabeIto => "okabe_ito",
        }
    }
}

impl Palette {
    #[must_use]
    pub fn colors(&self) -> Vec<[u8; 3]> {
        match self {
            Palette::Preset(p) => p.colors().to_vec(),
            Palette::Custom(c) => c.clone(),
        }
    }
}

pub const MAX_OPS: usize = 32;
pub const MAX_PALETTE: usize = 256;

/// Pre-pixel validation — every rejection is `FxError::Args` (maps to -001).
///
/// # Errors
/// `FxError::Args` with the offending op index and reason.
pub fn validate(args: &FxArgs) -> Result<(), FxError> {
    if args.ops.is_empty() {
        return Err(FxError::Args("ops must contain at least one op".into()));
    }
    if args.ops.len() > MAX_OPS {
        return Err(FxError::Args(format!("ops exceeds the {MAX_OPS}-op cap")));
    }
    for (i, op) in args.ops.iter().enumerate() {
        let last = i == args.ops.len() - 1;
        validate_op(op, last).map_err(|m| FxError::Args(format!("ops[{i}]: {m}")))?;
    }
    Ok(())
}

fn validate_op(op: &Op, last: bool) -> Result<(), String> {
    match op {
        Op::Resize { width, height, .. } => {
            if width.is_none() && height.is_none() {
                return Err("resize needs width and/or height".into());
            }
            for d in [width, height].into_iter().flatten() {
                if *d == 0 || *d > crate::codec::png::MAX_DIMENSION {
                    return Err(format!("resize dimension {d} out of 1..=16384"));
                }
            }
        }
        Op::Crop { width, height, .. } => {
            if *width == 0 || *height == 0 {
                return Err("crop dimensions must be ≥ 1".into());
            }
        }
        Op::Levels {
            brightness,
            contrast,
        } => {
            if !(-255..=255).contains(brightness) {
                return Err("brightness out of -255..=255".into());
            }
            if !(-128..=128).contains(contrast) {
                return Err("contrast out of -128..=128".into());
            }
        }
        Op::PaletteMap { palette } | Op::Dither { palette, .. } => {
            let n = palette.colors().len();
            if !(2..=MAX_PALETTE).contains(&n) {
                return Err(format!("palette needs 2..={MAX_PALETTE} colors (got {n})"));
            }
        }
        Op::Pixelate { block } => {
            if !(2..=256).contains(block) {
                return Err("pixelate block out of 2..=256".into());
            }
        }
        Op::Halftone { cell, .. } => {
            if !(3..=64).contains(cell) {
                return Err("halftone cell out of 3..=64".into());
            }
        }
        Op::Grain { intensity } => {
            if *intensity > 128 {
                return Err("grain intensity out of 0..=128".into());
            }
        }
        Op::Vignette { strength } => {
            if *strength > 255 {
                return Err("vignette strength out of 0..=255".into());
            }
        }
        Op::ChromaticAberration { shift } => {
            if !(1..=16).contains(shift) {
                return Err("chromatic_aberration shift out of 1..=16".into());
            }
        }
        Op::Scanlines { strength, period } => {
            if *strength > 255 {
                return Err("scanlines strength out of 0..=255".into());
            }
            if !(2..=64).contains(period) {
                return Err("scanlines period out of 2..=64".into());
            }
        }
        Op::Glitch {
            line_shift,
            channel_shift,
            blocks,
        } => {
            if *line_shift > 64 || *channel_shift > 16 || *blocks > 64 {
                return Err(
                    "glitch amounts out of range (line ≤64 · channel ≤16 · blocks ≤64)".into(),
                );
            }
            if *line_shift == 0 && *channel_shift == 0 && *blocks == 0 {
                return Err("glitch needs at least one non-zero amount".into());
            }
        }
        Op::Ascii { cols, .. } => {
            if !(2..=1024).contains(cols) {
                return Err("ascii cols out of 2..=1024".into());
            }
            if !last {
                return Err("ascii must be the LAST op (it changes the artifact type)".into());
            }
        }
        Op::Grayscale | Op::Duotone { .. } => {}
    }
    Ok(())
}

// ── Deterministic recipe JSON (fixed key order · ASCII) ───────────────────

fn push_rgb(s: &mut String, c: [u8; 3]) {
    use std::fmt::Write;
    let _ = write!(s, "[{},{},{}]", c[0], c[1], c[2]);
}

fn palette_json(p: &Palette) -> String {
    match p {
        Palette::Preset(pr) => format!("\"{}\"", pr.name()),
        Palette::Custom(colors) => {
            let mut s = String::from("[");
            for (i, c) in colors.iter().enumerate() {
                if i > 0 {
                    s.push(',');
                }
                push_rgb(&mut s, *c);
            }
            s.push(']');
            s
        }
    }
}

fn op_json(op: &Op) -> String {
    match op {
        Op::Resize {
            width,
            height,
            filter,
        } => {
            let f = match filter {
                ResizeFilter::Auto => "auto",
                ResizeFilter::Nearest => "nearest",
                ResizeFilter::Bilinear => "bilinear",
            };
            let w = width.map_or("null".into(), |v| v.to_string());
            let h = height.map_or("null".into(), |v| v.to_string());
            format!("{{\"resize\":{{\"width\":{w},\"height\":{h},\"filter\":\"{f}\"}}}}")
        }
        Op::Crop {
            x,
            y,
            width,
            height,
        } => format!("{{\"crop\":{{\"x\":{x},\"y\":{y},\"width\":{width},\"height\":{height}}}}}"),
        Op::Levels {
            brightness,
            contrast,
        } => format!("{{\"levels\":{{\"brightness\":{brightness},\"contrast\":{contrast}}}}}"),
        Op::Grayscale => "{\"grayscale\":{}}".into(),
        Op::PaletteMap { palette } => {
            format!(
                "{{\"palette_map\":{{\"palette\":{}}}}}",
                palette_json(palette)
            )
        }
        Op::Dither { mode, palette } => {
            let m = match mode {
                DitherMode::Bayer2 => "bayer2",
                DitherMode::Bayer4 => "bayer4",
                DitherMode::Bayer8 => "bayer8",
                DitherMode::BlueNoise => "blue_noise",
                DitherMode::Ign => "ign",
                DitherMode::FloydSteinberg => "floyd_steinberg",
                DitherMode::Atkinson => "atkinson",
                DitherMode::Jjn => "jjn",
            };
            format!(
                "{{\"dither\":{{\"mode\":\"{m}\",\"palette\":{}}}}}",
                palette_json(palette)
            )
        }
        Op::Duotone { dark, light } => {
            let mut s = String::from("{\"duotone\":{\"dark\":");
            push_rgb(&mut s, *dark);
            s.push_str(",\"light\":");
            push_rgb(&mut s, *light);
            s.push_str("}}");
            s
        }
        Op::Pixelate { block } => format!("{{\"pixelate\":{{\"block\":{block}}}}}"),
        Op::Halftone { cell, angle } => {
            let a = match angle {
                ScreenAngle::A0 => 0,
                ScreenAngle::A15 => 15,
                ScreenAngle::A45 => 45,
                ScreenAngle::A75 => 75,
            };
            format!("{{\"halftone\":{{\"cell\":{cell},\"angle\":{a}}}}}")
        }
        Op::Grain { intensity } => format!("{{\"grain\":{{\"intensity\":{intensity}}}}}"),
        Op::Vignette { strength } => {
            format!("{{\"vignette\":{{\"strength\":{strength}}}}}")
        }
        Op::ChromaticAberration { shift } => {
            format!("{{\"chromatic_aberration\":{{\"shift\":{shift}}}}}")
        }
        Op::Scanlines { strength, period } => {
            format!("{{\"scanlines\":{{\"strength\":{strength},\"period\":{period}}}}}")
        }
        Op::Glitch {
            line_shift,
            channel_shift,
            blocks,
        } => format!(
            "{{\"glitch\":{{\"line_shift\":{line_shift},\"channel_shift\":{channel_shift},\"blocks\":{blocks}}}}}"
        ),
        Op::Ascii { cols, emit } => {
            let e = match emit {
                AsciiEmit::Png => "png",
                AsciiEmit::Text => "text",
                AsciiEmit::Ansi => "ansi",
            };
            format!("{{\"ascii\":{{\"cols\":{cols},\"emit\":\"{e}\"}}}}")
        }
    }
}

/// The self-describing recipe embedded in the artifact (tEXt `nika`) —
/// fixed key order · no timestamp · byte-stable forever.
#[must_use]
pub fn recipe_json(args: &FxArgs, input_sha256: &str, engine: &str) -> String {
    let mut s = String::from("{\"kind\":\"image_fx\",\"v\":1,\"engine\":\"");
    s.push_str(engine);
    s.push_str("\",\"input_sha256\":\"");
    s.push_str(input_sha256);
    s.push_str("\",\"seed\":");
    s.push_str(&args.seed.to_string());
    s.push_str(",\"ops\":[");
    for (i, op) in args.ops.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&op_json(op));
    }
    s.push_str("]}");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_rejects_the_documented_traps() {
        let empty = FxArgs {
            ops: vec![],
            seed: 0,
        };
        assert!(matches!(validate(&empty), Err(FxError::Args(_))));

        let ascii_not_last = FxArgs {
            ops: vec![
                Op::Ascii {
                    cols: 80,
                    emit: AsciiEmit::Text,
                },
                Op::Grayscale,
            ],
            seed: 0,
        };
        assert!(matches!(validate(&ascii_not_last), Err(FxError::Args(_))));

        let bad_palette = FxArgs {
            ops: vec![Op::PaletteMap {
                palette: Palette::Custom(vec![[0, 0, 0]]),
            }],
            seed: 0,
        };
        assert!(matches!(validate(&bad_palette), Err(FxError::Args(_))));
    }

    #[test]
    fn recipe_json_is_deterministic_and_ascii() {
        let args = FxArgs {
            ops: vec![
                Op::Resize {
                    width: Some(320),
                    height: None,
                    filter: ResizeFilter::Auto,
                },
                Op::Dither {
                    mode: DitherMode::FloydSteinberg,
                    palette: Palette::Preset(PresetPalette::Gameboy),
                },
            ],
            seed: 42,
        };
        let a = recipe_json(&args, "ff00", "nika 0.98.0");
        let b = recipe_json(&args, "ff00", "nika 0.98.0");
        assert_eq!(a, b);
        assert!(a.is_ascii());
        assert!(a.contains("\"seed\":42"));
        assert!(a.contains("\"gameboy\""));
    }
}
