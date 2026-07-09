//! Vision-loop + bench driver: renders demo pipelines to files and prints
//! per-op timings. `cargo run -p nika-fx --example render_cards --release -- <dir>`

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cast_possible_truncation
)]
#![allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)] // demo dims ≤ 1024
#![allow(clippy::print_stdout, clippy::panic, clippy::too_many_lines)]

use nika_fx::{
    Artifact, AsciiEmit, DitherMode, FxArgs, Op, Palette, PixBuf, PresetPalette, ResizeFilter,
    ScreenAngle, codec, transform,
};
use std::time::Instant;

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
    // color bands footer
    let bands: [[u8; 3]; 8] = [
        [0, 0, 0],
        [255, 255, 255],
        [255, 0, 0],
        [0, 255, 0],
        [0, 0, 255],
        [255, 255, 0],
        [0, 255, 255],
        [255, 0, 255],
    ];
    for y in (h - h / 5)..h {
        for x in 0..w {
            img.set(x, y, bands[(x * bands.len()) / w]);
        }
    }
    codec::png::encode(&img, None)
}

fn run(dir: &str, name: &str, input: &[u8], args: &FxArgs) {
    let t = Instant::now();
    let out = transform(input, args, "nika-fx example").expect(name);
    let dt = t.elapsed();
    match &out.artifact {
        Artifact::Png(bytes) => {
            std::fs::write(format!("{dir}/{name}.png"), bytes).unwrap();
            println!("{name}: {} bytes · {dt:?}", bytes.len());
        }
        Artifact::Text { body, ext } => {
            std::fs::write(format!("{dir}/{name}.{ext}"), body).unwrap();
            println!("{name}.{ext}: {} chars · {dt:?}", body.len());
        }
        _ => println!("{name}: unhandled artifact kind"),
    }
}

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    let input = test_card(640, 400);
    std::fs::write(format!("{dir}/00-input.png"), &input).unwrap();

    let gb = Palette::Preset(PresetPalette::Gameboy);
    let demos: Vec<(&str, FxArgs)> = vec![
        (
            "01-gameboy-fs-crt",
            FxArgs {
                ops: vec![
                    Op::Resize {
                        width: Some(320),
                        height: None,
                        filter: ResizeFilter::Auto,
                    },
                    Op::Dither {
                        mode: DitherMode::FloydSteinberg,
                        palette: gb.clone(),
                    },
                    Op::Grain { intensity: 32 },
                    Op::Scanlines {
                        strength: 110,
                        period: 4,
                    },
                    Op::Vignette { strength: 140 },
                ],
                seed: 42,
            },
        ),
        (
            "02-bluenoise-bw",
            FxArgs {
                ops: vec![Op::Dither {
                    mode: DitherMode::BlueNoise,
                    palette: Palette::Preset(PresetPalette::Bw),
                }],
                seed: 0,
            },
        ),
        (
            "03-cga-bayer8",
            FxArgs {
                ops: vec![Op::Dither {
                    mode: DitherMode::Bayer8,
                    palette: Palette::Preset(PresetPalette::Cga),
                }],
                seed: 0,
            },
        ),
        (
            "04-duotone-glitch-ca",
            FxArgs {
                ops: vec![
                    Op::Duotone {
                        dark: [16, 10, 48],
                        light: [240, 235, 210],
                    },
                    Op::Glitch {
                        line_shift: 24,
                        channel_shift: 6,
                        blocks: 6,
                    },
                    Op::ChromaticAberration { shift: 5 },
                ],
                seed: 7,
            },
        ),
        (
            "05-halftone45",
            FxArgs {
                ops: vec![Op::Halftone {
                    cell: 7,
                    angle: ScreenAngle::A45,
                }],
                seed: 0,
            },
        ),
        (
            "06-pixelate-okabe",
            FxArgs {
                ops: vec![
                    Op::Pixelate { block: 10 },
                    Op::PaletteMap {
                        palette: Palette::Preset(PresetPalette::OkabeIto),
                    },
                ],
                seed: 0,
            },
        ),
        (
            "07-ascii",
            FxArgs {
                ops: vec![Op::Ascii {
                    cols: 96,
                    emit: AsciiEmit::Png,
                }],
                seed: 0,
            },
        ),
        (
            "08-ascii-ansi",
            FxArgs {
                ops: vec![Op::Ascii {
                    cols: 80,
                    emit: AsciiEmit::Ansi,
                }],
                seed: 0,
            },
        ),
    ];
    for (name, args) in &demos {
        run(&dir, name, &input, args);
    }

    // 1MP bench (the §10bis speed row)
    let big = test_card(1024, 1024);
    for (label, args) in [
        (
            "bayer8",
            FxArgs {
                ops: vec![Op::Dither {
                    mode: DitherMode::Bayer8,
                    palette: gb.clone(),
                }],
                seed: 0,
            },
        ),
        (
            "fs",
            FxArgs {
                ops: vec![Op::Dither {
                    mode: DitherMode::FloydSteinberg,
                    palette: gb.clone(),
                }],
                seed: 0,
            },
        ),
        (
            "full-chain",
            FxArgs {
                ops: vec![
                    Op::Dither {
                        mode: DitherMode::FloydSteinberg,
                        palette: gb.clone(),
                    },
                    Op::Grain { intensity: 32 },
                    Op::Scanlines {
                        strength: 110,
                        period: 4,
                    },
                    Op::Vignette { strength: 140 },
                ],
                seed: 42,
            },
        ),
    ] {
        let t = Instant::now();
        let out = transform(&big, &args, "bench").expect(label);
        let Artifact::Png(bytes) = &out.artifact else {
            panic!()
        };
        println!(
            "BENCH {label} 1MP: {:?} ({} bytes out)",
            t.elapsed(),
            bytes.len()
        );
    }
}
