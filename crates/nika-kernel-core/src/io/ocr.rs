// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! OCR engine trait — async text extraction from captured frames.
//!
//! Phase 2 M1 PR 2 (sprint plan `2026-05-14-nika-phase-2-m1-kernel-
//! modules-sprint-plan.md` §4). Reserved error codes NIKA-1020..1039
//! per `docs/architecture/forward-compat-invariants.md` Gate 12.
//!
//! ADR-006 monolithic-kernel-spirit (single trait + DTO · no proc macro
//! · no async runtime dep). ADR-016 ISP discipline (1 trait = 1
//! capability). ADR-037 bottom-up layer (L0.5 traits-only · zero tokio
//! · zero tesseract / paddleocr / vision-LLM crates · those land in L1
//! at Phase 2 M2-M3 · `nika-ocr-tesseract` · `nika-ocr-paddle` · vision
//! fallback via `nika-verb-infer`).
//!
//! Cross-PR dep · `TextRegion::bbox: Rect` reuses the screen-capture
//! `Rect` DTO shipped at PR 1 (M1.1 · canonical pixel-coordinate
//! rectangle). Keeping a single `Rect` across capture + OCR + a11y
//! prevents downstream consumers from coercing between parallel
//! geometry types (single canonical taxonomy · cohérent
//! `no-legacy-no-back-compat.md` Class 1).

use crate::io::screen::{Frame, Rect};
use serde::{Deserialize, Serialize};

/// Text region extracted from a frame.
///
/// `confidence` is a probability score in `[0.0, 1.0]` reported by the
/// underlying OCR engine · L1 impls SHOULD clamp out-of-range upstream
/// values before constructing this DTO. `language` follows BCP-47 (e.g.
/// "en" · "fr-FR" · "zh-Hans") when the engine can detect script ·
/// `None` when the engine is single-language or detection failed.
///
/// `PartialEq` only (no `Eq`) because `confidence: f32` lacks total
/// equality. Structural `==` is sufficient for the projection-layer
/// diff semantics + integration test assertions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TextRegion {
    /// Recognized text content (UTF-8 · engine-decoded).
    pub text: String,
    /// Bounding box in physical-pixel coordinates relative to the
    /// source frame (top-left origin · cohérent `io::screen::Rect`).
    pub bbox: Rect,
    /// Engine confidence score in `[0.0, 1.0]`.
    pub confidence: f32,
    /// BCP-47 language tag · `None` when undetectable.
    pub language: Option<String>,
}

impl TextRegion {
    /// Construct a new text-region record.
    ///
    /// Per Invariant #19 · every `#[non_exhaustive]` struct ships a
    /// `new()` constructor so downstream code never field-literals.
    #[must_use]
    pub fn new(text: String, bbox: Rect, confidence: f32, language: Option<String>) -> Self {
        Self {
            text,
            bbox,
            confidence,
            language,
        }
    }
}

/// OCR backend errors — the typed boundary of the `OcrEngine` trait (Pattern A ·
/// FCI-023bis). `#[non_exhaustive]`; `NikaErrorCode` impl + reserved range
/// (`Category::Ocr` · NIKA-1101..1109) live in `crate::errors`. Moved here from
/// `nika-ocr` 2026-06-10 so the typed NIKA taxonomy survives the trait boundary.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum OcrError {
    /// A `.rten` model file was not found at the configured path.
    #[error("NIKA-1101 · OCR model file not found: {path}")]
    ModelNotFound {
        /// Missing model path.
        path: String,
    },
    /// A `.rten` model failed to load / parse.
    #[error("NIKA-1102 · OCR model load failed: {reason}")]
    ModelLoadFailed {
        /// Backend-reported reason.
        reason: String,
    },
    /// The `ocrs` engine failed to initialize from the loaded models.
    #[error("NIKA-1103 · OCR engine init failed: {reason}")]
    EngineInit {
        /// Backend-reported reason.
        reason: String,
    },
    /// A requested sub-region lies outside the source frame bounds.
    #[error("NIKA-1104 · region {width}x{height}+{x}+{y} out of frame bounds {frame_w}x{frame_h}")]
    RegionOutOfBounds {
        /// Region x offset (physical px · top-left origin).
        x: i32,
        /// Region y offset.
        y: i32,
        /// Region width.
        width: u32,
        /// Region height.
        height: u32,
        /// Frame width.
        frame_w: u32,
        /// Frame height.
        frame_h: u32,
    },
    /// The frame's RGBA8 buffer length does not match `width * height * 4`.
    #[error("NIKA-1105 · invalid frame format: {reason}")]
    InvalidFrameFormat {
        /// What was malformed.
        reason: String,
    },
    /// `ocrs::OcrEngine::prepare_input` failed (image decode / shape).
    #[error("NIKA-1106 · OCR input preparation failed: {reason}")]
    PrepareInputFailed {
        /// Backend-reported reason.
        reason: String,
    },
    /// Text detection (`detect_words`) failed.
    #[error("NIKA-1107 · OCR detection failed: {reason}")]
    DetectionFailed {
        /// Backend-reported reason.
        reason: String,
    },
    /// Text recognition (`recognize_text`) failed.
    #[error("NIKA-1108 · OCR recognition failed: {reason}")]
    RecognitionFailed {
        /// Backend-reported reason.
        reason: String,
    },
    /// A `spawn_blocking` inference task panicked or was cancelled.
    #[error("NIKA-1109 · OCR task join failed: {reason}")]
    TaskJoinFailed {
        /// Join failure detail.
        reason: String,
    },
}

/// OCR engine trait · async text extraction from RGBA8 frames.
///
/// CANCEL SAFETY: every method is cancel-safe · OCR ops are read-only
/// inference over a captured `Frame` and produce no side effects. L1
/// impls SHOULD wrap the inference call in a `spawn_blocking` + cancel
/// token shim when the underlying engine is synchronous (tesseract-rs
/// is sync · paddleocr-rust is sync · vision-LLM via `nika-verb-infer`
/// is async-native). Dropping the future abandons the extraction
/// without partial state · partial text MUST NOT leak to the caller.
///
/// `#[trait_variant::make(OcrEngineDyn: Send)]` generates a `Send`
/// companion trait `OcrEngineDyn` for generic constraints (cohérent
/// `io::screen::ScreenCaptureDyn` pattern · ADR-006 + `io::fs::FsRead`
/// canonical shape). Downstream code uses `T: OcrEngineDyn` for
/// monomorphized hot paths. Per `trait_variant` 0.1 semantics · the
/// companion uses `impl Future + Send` returns which are NOT dyn-
/// compatible · this is intentional (kernel traits stay zero-cost ·
/// L1 impls wrap via `Arc<T>` not `Arc<dyn _>`).
#[trait_variant::make(OcrEngineDyn: Send)]
pub trait OcrEngine: Send + Sync {
    /// Extract all text regions from a frame.
    ///
    /// CANCEL SAFETY: cancel-safe (read-only inference · no side
    /// effects · L1 impls wrap sync engines in `spawn_blocking`).
    async fn read(&self, frame: &Frame) -> Result<Vec<TextRegion>, OcrError>;

    /// Extract text regions from a specific sub-region of a frame.
    ///
    /// CANCEL SAFETY: same contract as `read` · partial regional reads
    /// MUST NOT surface partial regions. `region` coordinates are
    /// physical-pixel relative to the frame origin (top-left · cohérent
    /// `io::screen::Rect`).
    async fn read_region(&self, frame: &Frame, region: Rect) -> Result<Vec<TextRegion>, OcrError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::screen::{DisplayId, Rect};
    use bytes::Bytes;

    /// `TextRegion` DTO round-trips through serde JSON · proves the
    /// shared `Rect` reuse (M1.1 cross-PR dep) is wired correctly.
    #[test]
    fn text_region_serde_roundtrip() {
        let region = TextRegion::new(
            "hello".to_string(),
            Rect::new(10, 20, 100, 30),
            0.95,
            Some("en".to_string()),
        );
        let json = serde_json::to_string(&region).expect("serialize text region");
        let back: TextRegion = serde_json::from_str(&json).expect("deserialize text region");
        assert_eq!(back, region);
    }

    /// `TextRegion::language = None` path round-trips · proves the
    /// `Option<String>` branch is wired (engines without language
    /// detection produce `None`).
    #[test]
    fn text_region_confidence_default_optional_language() {
        let region = TextRegion::new("world".to_string(), Rect::new(0, 0, 1, 1), 0.0, None);
        assert_eq!(region.language, None);
        assert!((region.confidence - 0.0).abs() < f32::EPSILON);
        let json = serde_json::to_string(&region).expect("serialize none-language");
        let back: TextRegion = serde_json::from_str(&json).expect("deserialize none-language");
        assert_eq!(back, region);
    }

    /// `OcrEngine` + `OcrEngineDyn` are well-formed as generic bounds.
    ///
    /// Per DEV-2 M1.1 lesson · `trait_variant` 0.1 returns
    /// `impl Future + Send` which is NOT dyn-compatible · use a
    /// generic-bound test (`T: OcrEngineDyn`) NOT a trait-object test.
    /// Pure compile-time check · no runtime path. If this compiles ·
    /// the trait bounds are well-formed.
    #[test]
    fn ocr_engine_generic_bound_compile_check() {
        fn _accepts_ocr_engine<T: OcrEngine>() {}
        fn _accepts_ocr_engine_dyn<T: OcrEngineDyn>() {}
    }

    /// `TextRegion` is Send + Sync · cohérent CANCEL SAFETY (DTO
    /// crosses task boundaries in async L1 impls).
    #[test]
    fn text_region_send_sync() {
        fn _assert<T: Send + Sync>() {}
        _assert::<TextRegion>();
    }

    /// `TextRegion::bbox` reuses canonical `io::screen::Rect` ·
    /// structural compatibility check (single canonical geometry type
    /// across capture + OCR + a11y · cohérent NUKE-LEGACY Class 1).
    #[test]
    fn text_region_bbox_shares_screen_rect() {
        let rect = Rect::new(5, 10, 200, 50);
        let region = TextRegion::new("shared".to_string(), rect, 0.8, None);
        assert_eq!(region.bbox.x, 5);
        assert_eq!(region.bbox.y, 10);
        assert_eq!(region.bbox.width, 200);
        assert_eq!(region.bbox.height, 50);
    }

    /// Construct a `Frame` from M1.1 + verify the OCR trait signatures
    /// accept it · cross-PR integration sanity (no runtime call · pure
    /// type-check).
    #[test]
    fn ocr_trait_accepts_frame_dto() {
        // Compile-time check that the function signatures are valid
        // generic bounds taking &Frame + Rect parameters. Inner fns
        // declared first to avoid `clippy::items-after-statements`.
        fn _read_signature<T: OcrEngine>(_engine: &T, _frame: &Frame) {}
        fn _read_region_signature<T: OcrEngine>(_engine: &T, _frame: &Frame, _region: Rect) {}

        let _frame = Frame::new(
            64,
            64,
            1.0,
            Bytes::from_static(&[0u8; 16384]),
            DisplayId(0),
            1000,
        );
    }
}
