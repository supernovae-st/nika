//! RGB8 pixel buffer — the one representation every op consumes and produces.
//! Alpha policy v1: decoded alpha is discarded (documented; the transform
//! output is a new derived artifact — compositing options land in a minor).

#[derive(Clone, PartialEq, Eq)]
pub struct PixBuf {
    pub w: usize,
    pub h: usize,
    px: Vec<[u8; 3]>,
}

impl std::fmt::Debug for PixBuf {
    /// Compact — never dump megapixels into a panic message.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PixBuf({}x{})", self.w, self.h)
    }
}

impl PixBuf {
    /// Allocate a black buffer. Caller has already passed the pixel budget
    /// gate — this only guards the trivial zero case.
    #[must_use]
    pub fn new(w: usize, h: usize) -> PixBuf {
        PixBuf {
            w,
            h,
            px: vec![[0, 0, 0]; w * h],
        }
    }

    #[must_use]
    pub fn from_pixels(w: usize, h: usize, px: Vec<[u8; 3]>) -> Option<PixBuf> {
        if px.len() == w * h {
            Some(PixBuf { w, h, px })
        } else {
            None
        }
    }

    #[inline]
    #[must_use]
    pub fn get(&self, x: usize, y: usize) -> [u8; 3] {
        self.px[y * self.w + x]
    }

    #[inline]
    pub fn set(&mut self, x: usize, y: usize, p: [u8; 3]) {
        self.px[y * self.w + x] = p;
    }

    #[inline]
    #[must_use]
    pub fn pixels(&self) -> &[[u8; 3]] {
        &self.px
    }

    #[inline]
    pub fn pixels_mut(&mut self) -> &mut [[u8; 3]] {
        &mut self.px
    }

    /// Clamped read — the sampling helper for radial ops (CA, barrel).
    #[inline]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // clamped to 0..dim
    #[allow(clippy::cast_possible_wrap)] // dims ≤ 16384
    #[must_use]
    pub fn get_clamped(&self, x: i64, y: i64) -> [u8; 3] {
        let cx = x.clamp(0, self.w as i64 - 1) as usize;
        let cy = y.clamp(0, self.h as i64 - 1) as usize;
        self.get(cx, cy)
    }
}
