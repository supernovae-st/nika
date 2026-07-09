//! ASCII render — the terminal-native artifact. Ramp per cell luma
//! (linear-light average), three emits: plain text, ANSI truecolor,
//! or a PNG raster through the embedded 8×8 mini-font.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]
// Cast law: every `as` in this file is bounded by construction (pixel coords
// <= 16384 · channel values <= 8192 Q13 · fixed-point <= 2^32) — the pixel-math
// cast class documented in the crate docs. Truncation/sign paths are unreachable.
use crate::args::AsciiEmit;
use crate::det::{linear_luma_q13, linear_to_srgb, srgb_to_linear};
use crate::pix::PixBuf;

/// The default luminance ramp, dark → light (Bourke-lineage short ramp).
pub const RAMP: &[u8; 10] = b" .:-=+*#%@";

/// 8×8 glyphs for RAMP (hand-designed in-house · row-major · bit 7 = left).
const FONT: [[u8; 8]; 10] = [
    [0, 0, 0, 0, 0, 0, 0, 0],                      // ' '
    [0, 0, 0, 0, 0, 0x18, 0x18, 0],                // '.'
    [0, 0x18, 0x18, 0, 0, 0x18, 0x18, 0],          // ':'
    [0, 0, 0, 0x7E, 0x7E, 0, 0, 0],                // '-'
    [0, 0, 0x7E, 0, 0, 0x7E, 0, 0],                // '='
    [0, 0x18, 0x18, 0x7E, 0x7E, 0x18, 0x18, 0],    // '+'
    [0, 0x66, 0x3C, 0xFF, 0x3C, 0x66, 0, 0],       // '*'
    [0x6C, 0x6C, 0xFE, 0x6C, 0xFE, 0x6C, 0x6C, 0], // '#'
    [0x62, 0x66, 0x0C, 0x18, 0x30, 0x66, 0x46, 0], // '%'
    [0x3C, 0x66, 0x6E, 0x6A, 0x6E, 0x60, 0x3C, 0], // '@'
];

struct Cell {
    ch_idx: usize,
    color: [u8; 3],
}

struct Grid {
    cols: usize,
    rows: usize,
    cells: Vec<Cell>,
}

fn grid(img: &PixBuf, cols: usize) -> Grid {
    let cols = cols.min(img.w).max(1);
    let cw = img.w.div_ceil(cols);
    let ch = (2 * cw).max(1); // terminal cells are ~2:1
    let rows = img.h.div_ceil(ch);
    let mut cells = Vec::with_capacity(cols * rows);
    for r in 0..rows {
        for c in 0..cols {
            let x0 = c * cw;
            let y0 = r * ch;
            let x1 = (x0 + cw).min(img.w);
            let y1 = (y0 + ch).min(img.h);
            let mut luma_acc = 0u64;
            let mut lin = [0u64; 3];
            let mut n = 0u64;
            for y in y0..y1 {
                for x in x0..x1 {
                    let p = img.get(x, y);
                    luma_acc += u64::from(linear_luma_q13(p));
                    for k in 0..3 {
                        lin[k] += u64::from(srgb_to_linear(p[k]));
                    }
                    n += 1;
                }
            }
            let n = n.max(1);
            let luma8 = ((luma_acc / n) * 255 / 8192).min(255) as usize;
            let ch_idx = (luma8 * (RAMP.len() - 1) + 127) / 255;
            let color = [
                linear_to_srgb((lin[0] / n) as u16),
                linear_to_srgb((lin[1] / n) as u16),
                linear_to_srgb((lin[2] / n) as u16),
            ];
            cells.push(Cell { ch_idx, color });
        }
    }
    Grid { cols, rows, cells }
}

/// The PNG raster the `emit: png` path WOULD allocate — mirrors `grid()`'s
/// cell math so the caller can budget-gate BEFORE the (potentially huge)
/// allocation on a tall/skinny input. Returns `(width_px, height_px)`.
#[must_use]
pub fn png_raster_dims(img: &PixBuf, cols: u32) -> (u64, u64) {
    let cols = (cols as usize).min(img.w).max(1);
    let cw = img.w.div_ceil(cols);
    let ch = (2 * cw).max(1);
    let rows = img.h.div_ceil(ch);
    (cols as u64 * 8, rows as u64 * 8)
}

pub enum AsciiArtifact {
    Text { body: String, ext: &'static str },
    Raster(PixBuf),
}

#[must_use]
pub fn render(img: &PixBuf, cols: u32, emit: AsciiEmit) -> AsciiArtifact {
    let g = grid(img, cols as usize);
    match emit {
        AsciiEmit::Text => AsciiArtifact::Text {
            body: text(&g),
            ext: "txt",
        },
        AsciiEmit::Ansi => AsciiArtifact::Text {
            body: ansi(&g),
            ext: "ans",
        },
        AsciiEmit::Png => AsciiArtifact::Raster(raster(&g)),
    }
}

fn text(g: &Grid) -> String {
    let mut s = String::with_capacity(g.rows * (g.cols + 1));
    for r in 0..g.rows {
        for c in 0..g.cols {
            s.push(RAMP[g.cells[r * g.cols + c].ch_idx] as char);
        }
        s.push('\n');
    }
    s
}

fn ansi(g: &Grid) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(g.rows * g.cols * 20);
    for r in 0..g.rows {
        for c in 0..g.cols {
            let cell = &g.cells[r * g.cols + c];
            let [cr, cg, cb] = cell.color;
            let _ = write!(s, "\x1b[38;2;{cr};{cg};{cb}m{}", RAMP[cell.ch_idx] as char);
        }
        s.push_str("\x1b[0m\n");
    }
    s
}

fn raster(g: &Grid) -> PixBuf {
    let mut out = PixBuf::new(g.cols * 8, g.rows * 8);
    for r in 0..g.rows {
        for c in 0..g.cols {
            let cell = &g.cells[r * g.cols + c];
            let glyph = &FONT[cell.ch_idx];
            for (gy, bits) in glyph.iter().enumerate() {
                for gx in 0..8 {
                    if bits & (0x80 >> gx) != 0 {
                        out.set(c * 8 + gx, r * 8 + gy, cell.color);
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn text_shape_and_ramp_direction() {
        let img = gradient(160, 40);
        let AsciiArtifact::Text { body, .. } = render(&img, 40, AsciiEmit::Text) else {
            panic!("expected text");
        };
        let lines: Vec<&str> = body.lines().collect();
        assert!(!lines.is_empty());
        assert!(lines[0].len() == 40);
        // Left (dark) must be lighter glyph-mass than right.
        let first = lines[0].as_bytes();
        assert_eq!(first[0], b' ');
        assert_eq!(*first.last().unwrap(), b'@');
    }

    #[test]
    fn ansi_carries_truecolor_and_resets() {
        let img = gradient(80, 20);
        let AsciiArtifact::Text { body, ext } = render(&img, 20, AsciiEmit::Ansi) else {
            panic!("expected ansi");
        };
        assert_eq!(ext, "ans");
        assert!(body.contains("\x1b[38;2;"));
        assert!(body.contains("\x1b[0m"));
    }

    #[test]
    fn raster_dims_are_cols_by_8() {
        let img = gradient(64, 32);
        let AsciiArtifact::Raster(px) = render(&img, 16, AsciiEmit::Png) else {
            panic!("expected raster");
        };
        assert_eq!(px.w, 16 * 8);
        assert_eq!(px.w % 8, 0);
        assert!(px.h % 8 == 0 && px.h > 0);
    }
}
