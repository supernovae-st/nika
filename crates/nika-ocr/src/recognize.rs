// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! OCR backend — implements the L0.5 `OcrEngine` trait via `ocrs`.
//!
//! **B.3 · real inference wired.** [`OcrBackend::with_models`] eagerly loads
//! the two `.rten` weight files (detection + recognition) from explicit local
//! paths and builds the synchronous `ocrs` engine. `read` / `read_region`
//! validate the RGBA8 frame purely (headless-testable · mutation-killable)
//! then run the `ocrs` pipeline (`prepare_input` → `detect_words` →
//! `find_text_lines` → `recognize_text`) inside `tokio::task::spawn_blocking`
//! — the engine is CPU-bound + synchronous so it runs OFF the async runtime.
//!
//! **CANCEL SAFETY** (kernel `OcrEngine` contract) · dropping the future
//! abandons the read with no caller-visible state · the blocking task runs to
//! completion on the pool and its result is simply discarded (a `Frame` read
//! is side-effect-free · partial text never leaks).
//!
//! **Sovereignty (Rule 1)** · models are NEVER bundled in the binary nor
//! auto-downloaded · the caller passes explicit local paths (operator /
//! daemon-provisioned · e.g. `~/.olympus/cache/ocr/*.rten`). See
//! `docs/crate-specs/nika-ocr.md` §4.
//!
//! The `ocrs` inference path (`run_ocr`) needs real `.rten` weights · it is
//! therefore NOT exercisable headless and carries a Rule-2 mutation exemption
//! (spec §6). All surrounding logic — frame/region validation, RGBA→RGB,
//! region crop, bbox union — is pure + fully tested.

use std::path::Path;
use std::sync::Arc;

use nika_kernel::io::ocr::{OcrEngine, TextRegion};
use nika_kernel::io::screen::{Frame, Rect};
use ocrs::{ImageSource, OcrEngine as OcrsEngine, OcrEngineParams, TextItem};
use rten::Model;

use crate::error::OcrError;

/// Pure-Rust OCR backend (driven by `ocrs`). Holds the loaded engine behind an
/// `Arc` so `read`/`read_region` hand a cheap clone to the `spawn_blocking`
/// inference task.
#[derive(Clone)]
#[non_exhaustive]
pub struct OcrBackend {
    engine: Arc<OcrsEngine>,
}

impl std::fmt::Debug for OcrBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OcrBackend").finish_non_exhaustive()
    }
}

impl OcrBackend {
    /// Load the detection + recognition `.rten` models from explicit local
    /// paths and build the `ocrs` engine.
    ///
    /// Per sovereignty Rule 1 the paths are operator/daemon-provisioned · this
    /// function reads local files only and NEVER fetches over the network.
    ///
    /// # Errors
    /// - [`OcrError::ModelNotFound`] — a model path does not exist.
    /// - [`OcrError::ModelLoadFailed`] — a `.rten` file failed to parse.
    /// - [`OcrError::EngineInit`] — `ocrs` rejected the loaded models.
    pub fn with_models(detection: &Path, recognition: &Path) -> Result<Self, OcrError> {
        let detection_model = load_model(detection)?;
        let recognition_model = load_model(recognition)?;
        let engine = OcrsEngine::new(OcrEngineParams {
            detection_model: Some(detection_model),
            recognition_model: Some(recognition_model),
            ..Default::default()
        })
        .map_err(|e| OcrError::EngineInit {
            reason: e.to_string(),
        })?;
        Ok(Self {
            engine: Arc::new(engine),
        })
    }
}

/// Load one `.rten` model from a local path — pure existence guard + parse.
fn load_model(path: &Path) -> Result<Model, OcrError> {
    if !path.exists() {
        return Err(OcrError::ModelNotFound {
            path: path.display().to_string(),
        });
    }
    Model::load_file(path).map_err(|e| OcrError::ModelLoadFailed {
        reason: e.to_string(),
    })
}

/// Drop the alpha channel · RGBA8 → RGB8 (the layout `ocrs::ImageSource`
/// expects). **Pure** · headless-testable · the input length is a multiple of
/// 4 (guaranteed by `validate_frame` upstream).
fn rgba_to_rgb(rgba: &[u8]) -> Vec<u8> {
    let mut rgb = Vec::with_capacity(rgba.len() / 4 * 3);
    for px in rgba.chunks_exact(4) {
        rgb.extend_from_slice(&px[0..3]);
    }
    rgb
}

/// Crop a row-major RGBA8 buffer to an in-bounds sub-region · returns the
/// cropped RGBA bytes (`rw * rh * 4`). **Pure** · the caller guarantees bounds
/// via [`validate_region`].
fn crop_rgba(rgba: &[u8], frame_w: u32, rx: u32, ry: u32, rw: u32, rh: u32) -> Vec<u8> {
    let fw = frame_w as usize;
    let (rx, ry, rw, rh) = (rx as usize, ry as usize, rw as usize, rh as usize);
    let mut out = Vec::with_capacity(rw * rh * 4);
    for row in 0..rh {
        let start = ((ry + row) * fw + rx) * 4;
        out.extend_from_slice(&rgba[start..start + rw * 4]);
    }
    out
}

/// Union of word bounding boxes → one line bbox `(x, y, width, height)`. Each
/// input is `(left, top, width, height)` in `i32` (the shape `ocrs` reports).
/// Returns `None` for an empty iterator. **Pure** · headless-testable.
fn words_bbox_union(
    rects: impl IntoIterator<Item = (i32, i32, i32, i32)>,
) -> Option<(i32, i32, u32, u32)> {
    let mut it = rects.into_iter();
    let (l0, t0, w0, h0) = it.next()?;
    let mut left = l0;
    let mut top = t0;
    let mut right = l0.saturating_add(w0);
    let mut bottom = t0.saturating_add(h0);
    for (l, t, w, h) in it {
        left = left.min(l);
        top = top.min(t);
        right = right.max(l.saturating_add(w));
        bottom = bottom.max(t.saturating_add(h));
    }
    // Saturating: `right` is built with `saturating_add` (can reach i32::MAX) while
    // `left` is a raw `min` (can be i32::MIN), so a plain `right - left` could
    // overflow i32 and panic in debug on adversarial bbox geometry from the model.
    // saturating_sub keeps the whole union non-panicking; try_from clamps the
    // (always-non-negative here) result into u32.
    let width = u32::try_from(right.saturating_sub(left)).unwrap_or(0);
    let height = u32::try_from(bottom.saturating_sub(top)).unwrap_or(0);
    Some((left, top, width, height))
}

/// Run the full synchronous `ocrs` pipeline on a 3-channel RGB buffer.
///
/// CPU-bound + blocking — call ONLY inside `spawn_blocking`. `x_off`/`y_off`
/// shift reported bboxes back into frame coordinates (`0,0` for a full read ·
/// the region origin for `read_region`).
///
/// **Rule-2 mutation exemption** (spec §6) · every line below either calls into
/// `ocrs`/`rten` or maps their output · it cannot run headless without real
/// `.rten` weights, so cargo-mutants cannot exercise it. The pure helpers it
/// composes (`words_bbox_union`) are tested + mutation-killed directly.
fn run_ocr(
    engine: &OcrsEngine,
    rgb: &[u8],
    w: u32,
    h: u32,
    x_off: i32,
    y_off: i32,
) -> Result<Vec<TextRegion>, OcrError> {
    let source =
        ImageSource::from_bytes(rgb, (w, h)).map_err(|e| OcrError::PrepareInputFailed {
            reason: e.to_string(),
        })?;
    let input = engine
        .prepare_input(source)
        .map_err(|e| OcrError::PrepareInputFailed {
            reason: e.to_string(),
        })?;
    let words = engine
        .detect_words(&input)
        .map_err(|e| OcrError::DetectionFailed {
            reason: e.to_string(),
        })?;
    let lines = engine.find_text_lines(&input, &words);
    let recognized =
        engine
            .recognize_text(&input, &lines)
            .map_err(|e| OcrError::RecognitionFailed {
                reason: e.to_string(),
            })?;

    let mut regions = Vec::new();
    for line in recognized.into_iter().flatten() {
        let text = line.to_string();
        if text.is_empty() {
            continue;
        }
        let word_rects = line.words().map(|word| {
            let br = word.bounding_rect();
            (br.left(), br.top(), br.width(), br.height())
        });
        if let Some((x, y, bw, bh)) = words_bbox_union(word_rects) {
            // ocrs exposes no public per-line scalar confidence · presence of a
            // recognized line ⇒ 1.0 (refine if a future ocrs surfaces it).
            let bbox = Rect::new(x.saturating_add(x_off), y.saturating_add(y_off), bw, bh);
            regions.push(TextRegion::new(text, bbox, 1.0, None));
        }
    }
    Ok(regions)
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
    async fn read(&self, frame: &Frame) -> Result<Vec<TextRegion>, OcrError> {
        validate_frame(frame)?; // pure · NIKA-1105 before any inference
        let rgb = rgba_to_rgb(frame.pixels.as_ref());
        let (w, h) = (frame.width, frame.height);
        let engine = Arc::clone(&self.engine);
        let regions = tokio::task::spawn_blocking(move || run_ocr(&engine, &rgb, w, h, 0, 0))
            .await
            .map_err(|e| OcrError::TaskJoinFailed {
                reason: e.to_string(),
            })??;
        Ok(regions)
    }

    async fn read_region(&self, frame: &Frame, region: Rect) -> Result<Vec<TextRegion>, OcrError> {
        validate_frame(frame)?;
        let (rx, ry) = validate_region(region, frame.width, frame.height)?; // NIKA-1104
        let cropped = crop_rgba(
            frame.pixels.as_ref(),
            frame.width,
            rx,
            ry,
            region.width,
            region.height,
        );
        let rgb = rgba_to_rgb(&cropped);
        let (w, h) = (region.width, region.height);
        let (x_off, y_off) = (region.x, region.y);
        let engine = Arc::clone(&self.engine);
        let regions =
            tokio::task::spawn_blocking(move || run_ocr(&engine, &rgb, w, h, x_off, y_off))
                .await
                .map_err(|e| OcrError::TaskJoinFailed {
                    reason: e.to_string(),
                })??;
        Ok(regions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use nika_kernel::io::screen::DisplayId;
    use nika_kernel::prelude::{NikaErrorCode, codes};
    use proptest::prelude::*;

    /// Build a well-formed RGBA8 frame of the given dimensions.
    fn frame(w: u32, h: u32) -> Frame {
        let len = (w as usize) * (h as usize) * 4;
        Frame::new(w, h, 1.0, Bytes::from(vec![0u8; len]), DisplayId::new(0), 1)
    }

    // --- with_models (real ocrs constructor · headless error paths) ---

    #[test]
    fn with_models_missing_detection_is_model_not_found() {
        let err = OcrBackend::with_models(
            Path::new("/no/such/detection.rten"),
            Path::new("/no/such/recognition.rten"),
        )
        .expect_err("missing model rejected");
        assert_eq!(
            err.nika_code(),
            codes::NIKA_1101,
            "absent path is ModelNotFound"
        );
    }

    #[test]
    fn with_models_garbage_file_is_model_load_failed() {
        // Detection path EXISTS but holds garbage → load_model parses it and
        // fails with ModelLoadFailed *before* the (missing) recognition path.
        let tmp = tempfile::NamedTempFile::new().expect("temp file");
        std::fs::write(tmp.path(), b"this is not a real .rten model").expect("write garbage");
        let err = OcrBackend::with_models(tmp.path(), Path::new("/no/such/recognition.rten"))
            .expect_err("garbage model rejected");
        assert_eq!(
            err.nika_code(),
            codes::NIKA_1102,
            "unparseable .rten is ModelLoadFailed"
        );
    }

    // --- rgba_to_rgb (pure · mutation-killing) ---

    #[test]
    fn rgba_to_rgb_drops_alpha_and_keeps_rgb_order() {
        // Two pixels: (1,2,3,255) and (4,5,6,128) → 6 RGB bytes.
        let rgba = [1, 2, 3, 255, 4, 5, 6, 128];
        assert_eq!(rgba_to_rgb(&rgba), vec![1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn rgba_to_rgb_empty_is_empty() {
        assert!(rgba_to_rgb(&[]).is_empty());
    }

    // --- crop_rgba (pure · mutation-killing) ---

    #[test]
    fn crop_rgba_extracts_the_requested_window() {
        // 2x2 RGBA frame · each pixel's R = its linear index (0,1,2,3).
        // Pixel layout (R only shown): [0][1]
        //                              [2][3]
        let mut rgba = Vec::new();
        for r in 0u8..4 {
            rgba.extend_from_slice(&[r, 0, 0, 255]);
        }
        // Crop the bottom-right 1x1 (rx=1, ry=1) → pixel index 3.
        let got = crop_rgba(&rgba, 2, 1, 1, 1, 1);
        assert_eq!(got, vec![3, 0, 0, 255]);
        // Crop the top row (rx=0, ry=0, 2x1) → pixels 0,1.
        let row = crop_rgba(&rgba, 2, 0, 0, 2, 1);
        assert_eq!(row, vec![0, 0, 0, 255, 1, 0, 0, 255]);
    }

    // --- words_bbox_union (pure · mutation-killing) ---

    #[test]
    fn words_bbox_union_empty_is_none() {
        let empty: Vec<(i32, i32, i32, i32)> = vec![];
        assert_eq!(words_bbox_union(empty), None);
    }

    #[test]
    fn words_bbox_union_single_word_is_that_rect() {
        assert_eq!(words_bbox_union([(10, 20, 30, 40)]), Some((10, 20, 30, 40)));
    }

    #[test]
    fn words_bbox_union_spans_min_left_top_to_max_right_bottom() {
        // word A: x10..40 y20..50 · word B: x5..25 y30..90
        // union: left=5 top=20 right=40 bottom=90 → (5,20,35,70)
        let got = words_bbox_union([(10, 20, 30, 30), (5, 30, 20, 60)]);
        assert_eq!(got, Some((5, 20, 35, 70)));
    }

    #[test]
    fn words_bbox_union_extreme_geometry_does_not_panic() {
        // Adversarial model output: one word pinned at i32::MIN, another whose
        // left+width saturates to i32::MAX. A plain `right - left` would overflow
        // i32 and panic in debug; saturating_sub keeps it bounded (no panic).
        let got = words_bbox_union([(i32::MIN, 0, i32::MAX, 10), (i32::MAX, 0, 10, 10)]);
        // left=i32::MIN, right saturates to i32::MAX → width = saturating span.
        assert!(got.is_some());
        let (left, _top, width, _height) = got.expect("non-empty union");
        assert_eq!(left, i32::MIN);
        assert_eq!(
            width,
            u32::try_from(i32::MAX.saturating_sub(i32::MIN)).unwrap_or(0)
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

        /// Crop window has the right byte length for any in-bounds region.
        /// Region coords are derived from the frame dims (modulo) so every
        /// case is in-bounds by construction — no rejects.
        #[test]
        fn crop_rgba_output_len_matches_region(
            fw in 1u32..64, fh in 1u32..64,
            rx_r in 0u32..4096, ry_r in 0u32..4096, rw_r in 0u32..4096, rh_r in 0u32..4096,
        ) {
            let rx = rx_r % fw;                  // 0 ..= fw-1
            let ry = ry_r % fh;
            let rw = (rw_r % (fw - rx)) + 1;     // 1 ..= fw-rx
            let rh = (rh_r % (fh - ry)) + 1;     // 1 ..= fh-ry
            let rgba = vec![0u8; (fw as usize) * (fh as usize) * 4];
            let out = crop_rgba(&rgba, fw, rx, ry, rw, rh);
            prop_assert_eq!(out.len(), (rw as usize) * (rh as usize) * 4);
        }
    }
}
