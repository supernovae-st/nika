// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! OCR backend — implements the L0.5 `OcrEngine` trait via `ocrs`.
//!
//! **B.2 skeleton.** `read` / `read_region` validate the input purely
//! (RGBA8 frame format + sub-region bounds · headless-testable + mutation-
//! killable) then return the `BackendNotWired` placeholder. B.3 wires the
//! real `ocrs` inference (RGBA→`ImageSource` · `prepare_input` ·
//! `detect_words` · `recognize_text` · `TextLine`→`TextRegion`) inside
//! `tokio::task::spawn_blocking` (the sync `ocrs` engine · kernel CANCEL
//! SAFETY contract · same pattern as `nika-screen`/`xcap`). Per
//! `skeleton-option-a-pattern.md` §5 the placeholder is CLOSED at B.3.

use nika_kernel::io::ocr::{OcrEngine, TextRegion};
use nika_kernel::io::screen::{Frame, Rect};

use crate::error::OcrError;

/// Pure-Rust OCR backend (driven by `ocrs` at B.3). The B.2 skeleton holds no
/// state; B.3 adds the loaded `Arc<ocrs::OcrEngine>` + model paths.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct OcrBackend {}

impl OcrBackend {
    /// Construct a new OCR backend (B.2 skeleton · models wired at B.3).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

/// Validate the frame carries a well-formed RGBA8 buffer — **pure** (no OS /
/// model call · headless-testable). The buffer MUST be exactly
/// `width * height * 4` bytes.
fn validate_frame(frame: &Frame) -> Result<(), OcrError> {
    let expected = (frame.width as usize)
        .saturating_mul(frame.height as usize)
        .saturating_mul(4);
    if frame.pixels.len() == expected {
        Ok(())
    } else {
        Err(OcrError::InvalidFrameFormat {
            reason: format!(
                "RGBA8 buffer is {} bytes, expected {expected} ({}x{}x4)",
                frame.pixels.len(),
                frame.width,
                frame.height
            ),
        })
    }
}

/// Validate a sub-region against frame bounds — **pure** (display-local
/// top-left coords). Returns the unsigned `(x, y)` origin, or
/// [`OcrError::RegionOutOfBounds`] for a negative offset · zero extent · or
/// out-of-bounds extent (strictly `> frame`; the exact boundary is IN bounds).
fn validate_region(region: Rect, frame_w: u32, frame_h: u32) -> Result<(u32, u32), OcrError> {
    let oob = || OcrError::RegionOutOfBounds {
        x: region.x,
        y: region.y,
        width: region.width,
        height: region.height,
        frame_w,
        frame_h,
    };
    let (Ok(rx), Ok(ry)) = (u32::try_from(region.x), u32::try_from(region.y)) else {
        return Err(oob());
    };
    if region.width == 0
        || region.height == 0
        || rx.saturating_add(region.width) > frame_w
        || ry.saturating_add(region.height) > frame_h
    {
        return Err(oob());
    }
    Ok((rx, ry))
}

impl OcrEngine for OcrBackend {
    async fn read(&self, frame: &Frame) -> std::io::Result<Vec<TextRegion>> {
        validate_frame(frame)?; // pure · structural NIKA-1105 before any inference
        // B.2 placeholder · B.3 wires ocrs (prepare_input → detect → recognize).
        Err(OcrError::BackendNotWired.into())
    }

    async fn read_region(&self, frame: &Frame, region: Rect) -> std::io::Result<Vec<TextRegion>> {
        validate_frame(frame)?;
        let _origin = validate_region(region, frame.width, frame.height)?; // NIKA-1104
        Err(OcrError::BackendNotWired.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use nika_kernel::io::screen::DisplayId;
    use proptest::prelude::*;

    /// Build a well-formed RGBA8 frame of the given dimensions.
    fn frame(w: u32, h: u32) -> Frame {
        let len = (w as usize) * (h as usize) * 4;
        Frame::new(w, h, 1.0, Bytes::from(vec![0u8; len]), DisplayId::new(0), 1)
    }

    #[tokio::test]
    async fn read_on_valid_frame_hits_placeholder_not_format_error() {
        let backend = OcrBackend::new();
        let io = backend
            .read(&frame(8, 4))
            .await
            .expect_err("B.2 placeholder denies");
        let oe = io
            .into_inner()
            .expect("boxed")
            .downcast::<OcrError>()
            .expect("OcrError");
        assert_eq!(
            oe.code(),
            "NIKA-1100",
            "valid frame reaches the BackendNotWired placeholder"
        );
    }

    #[tokio::test]
    async fn read_rejects_malformed_frame_buffer() {
        let backend = OcrBackend::new();
        // Claim 8x4 (=128 bytes RGBA8) but give 10 bytes.
        let bad = Frame::new(
            8,
            4,
            1.0,
            Bytes::from_static(&[0u8; 10]),
            DisplayId::new(0),
            1,
        );
        let io = backend
            .read(&bad)
            .await
            .expect_err("malformed buffer rejected");
        let oe = io
            .into_inner()
            .expect("boxed")
            .downcast::<OcrError>()
            .expect("OcrError");
        assert_eq!(
            oe.code(),
            "NIKA-1105",
            "wrong RGBA8 length is a frame-format error"
        );
    }

    #[tokio::test]
    async fn read_region_rejects_out_of_bounds() {
        let backend = OcrBackend::new();
        let io = backend
            .read_region(&frame(100, 100), Rect::new(0, 0, 200, 10))
            .await
            .expect_err("oob region rejected");
        let oe = io
            .into_inner()
            .expect("boxed")
            .downcast::<OcrError>()
            .expect("OcrError");
        assert_eq!(
            oe.code(),
            "NIKA-1104",
            "region wider than frame is out of bounds"
        );
    }

    #[tokio::test]
    async fn read_region_in_bounds_hits_placeholder() {
        let backend = OcrBackend::new();
        let io = backend
            .read_region(&frame(100, 100), Rect::new(10, 10, 50, 50))
            .await
            .expect_err("B.2 placeholder denies");
        let oe = io
            .into_inner()
            .expect("boxed")
            .downcast::<OcrError>()
            .expect("OcrError");
        assert_eq!(
            oe.code(),
            "NIKA-1100",
            "valid region passes validation to the placeholder"
        );
    }

    // --- validate_frame (pure · mutation-killing) ---

    #[test]
    fn validate_frame_accepts_exact_rgba8_length() {
        assert!(validate_frame(&frame(16, 9)).is_ok());
    }

    #[test]
    fn validate_frame_rejects_short_and_long_buffers() {
        let short = Frame::new(
            2,
            2,
            1.0,
            Bytes::from_static(&[0u8; 15]),
            DisplayId::new(0),
            1,
        ); // need 16
        let long = Frame::new(
            2,
            2,
            1.0,
            Bytes::from_static(&[0u8; 17]),
            DisplayId::new(0),
            1,
        );
        assert!(validate_frame(&short).is_err());
        assert!(validate_frame(&long).is_err());
    }

    // --- validate_region (pure · mutation-killing · mirrors nika-screen) ---

    #[test]
    fn validate_region_accepts_in_bounds_and_exact_boundary() {
        assert_eq!(
            validate_region(Rect::new(10, 20, 40, 30), 1920, 1080).expect("in"),
            (10, 20)
        );
        // rx+width == frame_w (and height) is IN bounds (> not >=).
        assert_eq!(
            validate_region(Rect::new(0, 0, 1920, 1080), 1920, 1080).expect("edge"),
            (0, 0)
        );
    }

    #[test]
    fn validate_region_rejects_overflow_zero_and_negative() {
        assert!(validate_region(Rect::new(1, 0, 1920, 10), 1920, 1080).is_err()); // width +1 over
        assert!(validate_region(Rect::new(0, 1, 10, 1080), 1920, 1080).is_err()); // height +1 over
        assert!(validate_region(Rect::new(0, 0, 0, 50), 1920, 1080).is_err()); // zero width alone
        assert!(validate_region(Rect::new(0, 0, 50, 0), 1920, 1080).is_err()); // zero height alone
        assert!(validate_region(Rect::new(-1, 0, 10, 10), 1920, 1080).is_err()); // negative offset
    }

    proptest! {
        /// Gate 6 · any region fully inside the frame round-trips its origin;
        /// pins the bounds comparators against headless mutation.
        #[test]
        fn validate_region_inside_is_ok(
            fw in 1u32..4096, fh in 1u32..4096,
            x in 0i32..4096, y in 0i32..4096, w in 1u32..4096, h in 1u32..4096,
        ) {
            let rx = u32::try_from(x).unwrap_or(0);
            let ry = u32::try_from(y).unwrap_or(0);
            let inside = rx.saturating_add(w) <= fw && ry.saturating_add(h) <= fh;
            let got = validate_region(Rect::new(x, y, w, h), fw, fh);
            prop_assert_eq!(got.is_ok(), inside);
            if inside {
                prop_assert_eq!(got.expect("inside"), (rx, ry));
            }
        }
    }
}
