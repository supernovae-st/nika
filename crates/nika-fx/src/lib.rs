//! # nika-fx — deterministic artistic image effects
//!
//! The pure substrate behind the `nika:image_fx` builtin: PNG in → styled
//! artifact out, **byte-identical forever** for identical `(input, args)`.
//! Zero dependencies — codec (full RFC 1951 inflate + PNG both directions),
//! sha256, PCG32 and all color math are hand-rolled from primary specs
//! (see `docs/crate-specs/nika-fx.md` + the FX master plan).
//!
//! The determinism contract is the product: every artifact's sha256 joins
//! the run's hash-chained trace, re-render IS the tamper check (`verify`),
//! and the recipe rides the artifact itself (`nika` tEXt chunk · no
//! timestamp). Receipts, not decoration.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod args;
pub mod codec;
pub mod det;
pub mod error;
pub mod ops;
pub mod pix;
pub mod tables;

pub use args::{
    AsciiEmit, DitherMode, FxArgs, Op, Palette, PresetPalette, ResizeFilter, ScreenAngle,
};
pub use error::FxError;

/// The ALGORITHM contract tag embedded in artifact recipes. Stable across
/// engine releases — re-render from a recipe stays byte-exact until the fx
/// contract itself majors (then this becomes `image_fx/v2`). WHICH binary
/// produced an artifact lives in the trace/manifest planes, never here.
pub const CONTRACT_TAG: &str = "image_fx/v1";
pub use pix::PixBuf;

use ops::ascii::AsciiArtifact;

/// What a pipeline produces — a PNG (with the recipe chunk) or a text
/// artifact (ascii `text`/`ansi` emits). Bytes never carry timestamps.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Artifact {
    Png(Vec<u8>),
    Text { body: String, ext: &'static str },
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct FxOutput {
    pub artifact: Artifact,
    pub width: u32,
    pub height: u32,
    pub ops_applied: u32,
    /// sha256 of the INPUT bytes — the lineage link the trace records.
    pub input_sha256: String,
    /// The deterministic recipe JSON embedded in PNG artifacts.
    pub recipe: String,
}

/// Run the pipeline. Pure: no I/O, no clock, no global state.
///
/// # Errors
/// `FxError::Args` (invalid pipeline) · `FxError::UnsupportedFormat` /
/// `FxError::Decode` / `FxError::Budget` (input gate) — every failure is
/// typed, never a panic.
#[allow(clippy::cast_possible_truncation)] // dims ≤ 16384 by the decode gate
pub fn transform(input_png: &[u8], args: &FxArgs, engine_tag: &str) -> Result<FxOutput, FxError> {
    args::validate(args)?;
    let input_sha256 = det::sha256_hex(input_png);
    let recipe = args::recipe_json(args, &input_sha256, engine_tag);
    let mut img = codec::png::decode(input_png)?;
    let mut rng = det::Pcg32::new(args.seed);
    let mut applied = 0u32;
    for op in &args.ops {
        if let Op::Ascii { cols, emit } = op {
            applied += 1;
            // The ascii `png` emit derives a raster from the CELL GRID, not
            // the input dims — a tall/skinny input can blow past the decode
            // budget on the OUTPUT side. Gate it before render allocates.
            if *emit == AsciiEmit::Png {
                let (rw, rh) = ops::ascii::png_raster_dims(&img, *cols);
                let max_dim = u64::from(codec::png::MAX_DIMENSION);
                if rw > max_dim || rh > max_dim {
                    return Err(FxError::Args(format!(
                        "ascii png raster {rw}x{rh} exceeds the {max_dim} per-side cap \
                         (reduce cols, or crop/resize the input first)"
                    )));
                }
                if rw * rh > codec::png::MAX_PIXELS {
                    return Err(FxError::Budget {
                        pixels: rw * rh,
                        max: codec::png::MAX_PIXELS,
                    });
                }
            }
            return Ok(finish_ascii(
                ops::ascii::render(&img, *cols, *emit),
                &recipe,
                applied,
                input_sha256,
            ));
        }
        img = ops::apply(&img, op, &mut rng, args.seed)?;
        applied += 1;
    }
    let (w, h) = (img.w as u32, img.h as u32);
    let png = codec::png::encode(&img, Some(&recipe));
    Ok(FxOutput {
        artifact: Artifact::Png(png),
        width: w,
        height: h,
        ops_applied: applied,
        input_sha256,
        recipe,
    })
}

#[allow(clippy::cast_possible_truncation)] // raster dims ≤ cols·8 ≤ 8192
fn finish_ascii(art: AsciiArtifact, recipe: &str, applied: u32, input_sha256: String) -> FxOutput {
    match art {
        AsciiArtifact::Text { body, ext } => FxOutput {
            width: 0,
            height: 0,
            artifact: Artifact::Text { body, ext },
            ops_applied: applied,
            input_sha256,
            recipe: recipe.to_string(),
        },
        AsciiArtifact::Raster(px) => {
            let (w, h) = (px.w as u32, px.h as u32);
            let png = codec::png::encode(&px, Some(recipe));
            FxOutput {
                artifact: Artifact::Png(png),
                width: w,
                height: h,
                ops_applied: applied,
                input_sha256,
                recipe: recipe.to_string(),
            }
        }
    }
}

/// Re-render and byte-compare — the tamper check (`re-render IS the
/// verification`). `Ok(true)` = the artifact is exactly what this input
/// and recipe produce.
///
/// # Errors
/// Same surface as [`transform`] — verification re-runs the pipeline.
pub fn verify(
    input_png: &[u8],
    args: &FxArgs,
    engine_tag: &str,
    artifact: &Artifact,
) -> Result<bool, FxError> {
    let fresh = transform(input_png, args, engine_tag)?;
    Ok(&fresh.artifact == artifact)
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss
    )]

    use super::*;

    /// Deterministic synthetic test card (gradient + discs + bands).
    fn test_card(w: usize, h: usize) -> Vec<u8> {
        let mut img = PixBuf::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let g = (x * 255 / (w - 1)) as u8;
                img.set(x, y, [g, g, g]);
            }
        }
        let discs: [(i64, i64, i64, [u8; 3]); 3] = [
            (w as i64 / 5, h as i64 / 3, h as i64 / 5, [200, 60, 50]),
            (w as i64 / 2, h as i64 / 2, h as i64 / 4, [60, 180, 90]),
            (4 * w as i64 / 5, h as i64 / 4, h as i64 / 7, [70, 90, 210]),
        ];
        for (cx, cy, r, col) in discs {
            for y in 0..h as i64 {
                for x in 0..w as i64 {
                    if (x - cx) * (x - cx) + (y - cy) * (y - cy) <= r * r {
                        img.set(x as usize, y as usize, col);
                    }
                }
            }
        }
        codec::png::encode(&img, None)
    }

    fn gameboy_pipeline() -> FxArgs {
        FxArgs {
            ops: vec![
                Op::Resize {
                    width: Some(160),
                    height: None,
                    filter: ResizeFilter::Auto,
                },
                Op::Dither {
                    mode: DitherMode::FloydSteinberg,
                    palette: Palette::Preset(PresetPalette::Gameboy),
                },
                Op::Grain { intensity: 24 },
                Op::Scanlines {
                    strength: 96,
                    period: 4,
                },
            ],
            seed: 42,
        }
    }

    #[test]
    fn full_pipeline_double_transform_byte_identical() {
        let input = test_card(320, 200);
        let args = gameboy_pipeline();
        let a = transform(&input, &args, "test 0.0.0").expect("run a");
        let b = transform(&input, &args, "test 0.0.0").expect("run b");
        assert_eq!(a, b);
        let Artifact::Png(bytes) = &a.artifact else {
            panic!("expected png")
        };
        assert!(bytes.len() > 100);
        assert_eq!(a.ops_applied, 4);
    }

    #[test]
    fn artifact_decodes_and_recipe_chunk_is_embedded() {
        let input = test_card(200, 120);
        let args = gameboy_pipeline();
        let out = transform(&input, &args, "test 0.0.0").expect("run");
        let Artifact::Png(bytes) = &out.artifact else {
            panic!("png")
        };
        let img = codec::png::decode(bytes).expect("self-decode");
        assert_eq!((img.w as u32, img.h as u32), (out.width, out.height));
        // The recipe JSON must ride the bytes (tEXt · keyword `nika`).
        let needle = b"\"kind\":\"image_fx\"";
        assert!(
            bytes.windows(needle.len()).any(|w| w == needle),
            "recipe chunk missing"
        );
        assert!(out.recipe.contains(&out.input_sha256));
    }

    #[test]
    fn verify_accepts_honest_and_rejects_tampered() {
        let input = test_card(160, 100);
        let args = gameboy_pipeline();
        let out = transform(&input, &args, "test 0.0.0").expect("run");
        assert!(verify(&input, &args, "test 0.0.0", &out.artifact).expect("verify"));
        // Tamper one byte inside the artifact → verify must refuse.
        let Artifact::Png(mut bytes) = out.artifact else {
            panic!("png")
        };
        let mid = bytes.len() / 2;
        bytes[mid] ^= 0xFF;
        let tampered = Artifact::Png(bytes);
        assert!(!verify(&input, &args, "test 0.0.0", &tampered).expect("verify"));
        // A different seed is a different artifact (the seed IS the style).
        let mut other = args.clone();
        other.seed = 43;
        let out2 = transform(&input, &other, "test 0.0.0").expect("run2");
        assert_ne!(
            transform(&input, &args, "test 0.0.0").expect("re").artifact,
            out2.artifact
        );
    }

    #[test]
    fn ascii_text_and_ansi_terminate_the_pipeline() {
        let input = test_card(160, 80);
        let args = FxArgs {
            ops: vec![
                Op::Grayscale,
                Op::Ascii {
                    cols: 40,
                    emit: AsciiEmit::Text,
                },
            ],
            seed: 0,
        };
        let out = transform(&input, &args, "test 0.0.0").expect("ascii");
        let Artifact::Text { body, ext } = &out.artifact else {
            panic!("text")
        };
        assert_eq!(*ext, "txt");
        assert!(body.lines().count() >= 4);
        // ANSI emit
        let args2 = FxArgs {
            ops: vec![Op::Ascii {
                cols: 40,
                emit: AsciiEmit::Ansi,
            }],
            seed: 0,
        };
        let out2 = transform(&input, &args2, "test 0.0.0").expect("ansi");
        let Artifact::Text {
            body: ansi,
            ext: e2,
        } = &out2.artifact
        else {
            panic!()
        };
        assert_eq!(*e2, "ans");
        assert!(ansi.contains("\x1b[38;2;"));
    }

    #[test]
    fn every_core_op_runs_end_to_end() {
        let input = test_card(120, 90);
        let single_ops = vec![
            Op::Resize {
                width: Some(64),
                height: None,
                filter: ResizeFilter::Auto,
            },
            Op::Crop {
                x: 10,
                y: 10,
                width: 80,
                height: 60,
            },
            Op::Levels {
                brightness: 20,
                contrast: 30,
            },
            Op::Grayscale,
            Op::PaletteMap {
                palette: Palette::Preset(PresetPalette::OkabeIto),
            },
            Op::Dither {
                mode: DitherMode::BlueNoise,
                palette: Palette::Preset(PresetPalette::Cga),
            },
            Op::Duotone {
                dark: [20, 12, 60],
                light: [245, 240, 220],
            },
            Op::Pixelate { block: 8 },
            Op::Halftone {
                cell: 6,
                angle: ScreenAngle::A45,
            },
            Op::Grain { intensity: 64 },
            Op::Vignette { strength: 200 },
            Op::ChromaticAberration { shift: 6 },
            Op::Scanlines {
                strength: 128,
                period: 3,
            },
            Op::Glitch {
                line_shift: 12,
                channel_shift: 4,
                blocks: 4,
            },
        ];
        for op in single_ops {
            let args = FxArgs {
                ops: vec![op.clone()],
                seed: 7,
            };
            let out = transform(&input, &args, "test 0.0.0")
                .unwrap_or_else(|e| panic!("op {op:?} failed: {e}"));
            let Artifact::Png(bytes) = &out.artifact else {
                panic!("png")
            };
            codec::png::decode(bytes).unwrap_or_else(|e| panic!("op {op:?} self-decode: {e}"));
        }
    }

    #[test]
    fn ascii_png_raster_respects_the_output_budget() {
        // A tall/skinny input with a fine ascii grid derives a huge raster on
        // the OUTPUT side — the decode-side budget does not cover it. Must be
        // a typed Args/Budget rejection, never a multi-GiB allocation.
        let input = test_card(64, 16384);
        let args = FxArgs::new(
            vec![Op::Ascii {
                cols: 64,
                emit: AsciiEmit::Png,
            }],
            0,
        );
        let err = transform(&input, &args, "t").unwrap_err();
        assert!(
            matches!(err, FxError::Args(_) | FxError::Budget { .. }),
            "expected a typed budget rejection, got {err:?}"
        );
        // A sane ascii raster still succeeds.
        let ok = FxArgs::new(
            vec![Op::Ascii {
                cols: 40,
                emit: AsciiEmit::Png,
            }],
            0,
        );
        transform(&test_card(200, 120), &ok, "t").expect("normal ascii png renders");
    }

    #[test]
    fn golden_gameboy_pipeline_sha256_pinned() {
        // THE golden — any byte change anywhere in the pipeline flips this.
        let input = test_card(320, 200);
        let out = transform(&input, &gameboy_pipeline(), "golden 1").expect("run");
        let Artifact::Png(bytes) = &out.artifact else {
            panic!("png")
        };
        let hash = det::sha256_hex(bytes);
        assert_eq!(
            hash, "02a1111f5b8bddd07ca4d0ebc607e41e6d0b5fc28adcf81738ca105c7d04bc05",
            "golden artifact hash changed — deliberate? re-pin + document"
        );
    }
}
