//! Integer rasterizer · RGB8 framebuffer · zero float in the pixel path
//! (Bresenham lines · half-open rects · bitmap font) — nondeterminism has no
//! entry point (R3 Q7). Out-of-bounds is CLIPPED, never a panic or a wrap.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)] // integer raster: every cast is a deliberate pixel quantization
use crate::font8;
use crate::png;

/// RGB color.
pub type Rgb = (u8, u8, u8);

/// Row-major RGB8 canvas.
pub struct Raster {
    w: i32,
    h: i32,
    px: Vec<u8>,
}

impl Raster {
    /// Defense-in-depth pixel budget (`spec::MAX_DIM²` · ~50MB RGB) — the
    /// raster is pub and reachable without `spec.validate()`.
    const MAX_PIXELS: u64 = 4096 * 4096;

    #[must_use]
    pub fn new(w: u32, h: u32, bg: Rgb) -> Self {
        let (w, h) = if u64::from(w) * u64::from(h) > Self::MAX_PIXELS {
            (1, 1) // clamp: a degenerate 1×1, never a 30GB allocation
        } else {
            (w, h)
        };
        let mut px = Vec::with_capacity((w * h * 3) as usize);
        for _ in 0..(w * h) {
            px.push(bg.0);
            px.push(bg.1);
            px.push(bg.2);
        }
        Self {
            w: w as i32,
            h: h as i32,
            px,
        }
    }

    fn put(&mut self, x: i32, y: i32, c: Rgb) {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            return;
        }
        let off = ((y * self.w + x) * 3) as usize;
        if let Some(slot) = self.px.get_mut(off..off + 3) {
            slot.copy_from_slice(&[c.0, c.1, c.2]);
        }
    }

    /// Half-open rect fill `[x, x+w) × [y, y+h)`.
    #[allow(clippy::many_single_char_names)] // geometry signature (x y w h)
    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: Rgb) {
        for yy in y.max(0)..(y + h).min(self.h) {
            for xx in x.max(0)..(x + w).min(self.w) {
                self.put(xx, yy, c);
            }
        }
    }

    pub fn hline(&mut self, x0: i32, x1: i32, y: i32, c: Rgb) {
        self.fill_rect(x0.min(x1), y, (x1 - x0).abs() + 1, 1, c);
    }

    pub fn vline(&mut self, x: i32, y0: i32, y1: i32, c: Rgb) {
        self.fill_rect(x, y0.min(y1), 1, (y1 - y0).abs() + 1, c);
    }

    /// Bresenham line (integer-only · deterministic by construction).
    #[allow(clippy::many_single_char_names)] // Bresenham/glyph notation
    pub fn line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: Rgb) {
        let (mut x, mut y) = (x0, y0);
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            self.put(x, y, c);
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    /// Filled disc (integer radius test · r² compare · no sqrt).
    pub fn disc(&mut self, cx: i32, cy: i32, r: i32, c: Rgb) {
        let r2 = r * r + r; // +r softens the edge one notch
        for dy in -r..=r {
            for dx in -r..=r {
                if dx * dx + dy * dy <= r2 {
                    self.put(cx + dx, cy + dy, c);
                }
            }
        }
    }

    /// 8×8 bitmap text at integer scale (1 = 8px · 2 = 16px).
    #[allow(clippy::many_single_char_names)] // Bresenham/glyph notation
    pub fn text(&mut self, x: i32, y: i32, s: &str, scale: i32, c: Rgb) {
        let mut cx = x;
        for ch in s.chars() {
            let g = font8::glyph(ch);
            for (row, bits) in g.iter().enumerate() {
                for col in 0..8i32 {
                    if bits >> col & 1 == 1 {
                        self.fill_rect(cx + col * scale, y + (row as i32) * scale, scale, scale, c);
                    }
                }
            }
            cx += 8 * scale;
        }
    }

    /// Text pixel width at scale.
    #[must_use]
    pub fn text_width(s: &str, scale: i32) -> i32 {
        (s.chars().count() as i32) * 8 * scale
    }

    /// Encode to PNG bytes (deterministic).
    #[must_use]
    pub fn to_png(&self) -> Vec<u8> {
        png::encode_rgb(self.w as u32, self.h as u32, &self.px)
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::unreadable_literal
)]
mod tests {
    use super::*;

    #[test]
    fn clipping_never_panics() {
        let mut r = Raster::new(10, 10, (255, 255, 255));
        r.fill_rect(-5, -5, 100, 100, (0, 0, 0));
        r.line(-10, -10, 50, 50, (1, 2, 3));
        r.text(-3, -3, "clip", 2, (9, 9, 9));
        assert_eq!(r.px.len(), 300);
    }

    #[test]
    fn deterministic_png() {
        let mut r = Raster::new(20, 20, (250, 250, 250));
        r.disc(10, 10, 5, (0, 114, 178));
        r.text(1, 1, "ok", 1, (26, 26, 26));
        let a = r.to_png();
        let b = r.to_png();
        assert_eq!(a, b);
        assert_eq!(&a[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }
}
