// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! OCR error taxonomy — NIKA-1101..1109.
//!
//! Reserved sub-range **NIKA-1100..1199** per ADR-081 `nika_codes` matrix
//! (supersedes the stale `io/ocr.rs` doc-comment "NIKA-1020..1039" which
//! predates ADR-081). Codes speak the workspace-canonical
//! [`NikaErrorCode`] trait (`nika_code()` → registry consts
//! `NIKA_1101..1109` · `Category::Ocr` · B5 error one-voice 2026-06-10
//! — the crate-local string `code()` accessor was the F8 drift ·
//! removed per `no-legacy-no-back-compat.md`); `is_transient()` lets a
//! caller decide retry vs structural failure.
//!
//! [`NikaErrorCode`]: nika_kernel::prelude::NikaErrorCode
//!
//! NIKA-1100 was the B.2 `BackendNotWired` skeleton placeholder · CLOSED at
//! B.3 when the `ocrs` backend was wired (per `skeleton-option-a-pattern.md`
//! §5) · the slot stays reserved.

use thiserror::Error;

/// OCR backend errors · NIKA-1101..1109 (registry-owned · `Category::Ocr`).
#[derive(Debug, Error, miette::Diagnostic)]
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

impl nika_kernel::prelude::NikaErrorCode for OcrError {
    /// Stable NIKA code (registry-owned · `Category::Ocr` 1100-1199 ·
    /// NIKA-1100 = retired B.2 placeholder slot).
    fn nika_code(&self) -> nika_kernel::prelude::NikaCode {
        use nika_kernel::prelude::codes;
        match self {
            Self::ModelNotFound { .. } => codes::NIKA_1101,
            Self::ModelLoadFailed { .. } => codes::NIKA_1102,
            Self::EngineInit { .. } => codes::NIKA_1103,
            Self::RegionOutOfBounds { .. } => codes::NIKA_1104,
            Self::InvalidFrameFormat { .. } => codes::NIKA_1105,
            Self::PrepareInputFailed { .. } => codes::NIKA_1106,
            Self::DetectionFailed { .. } => codes::NIKA_1107,
            Self::RecognitionFailed { .. } => codes::NIKA_1108,
            Self::TaskJoinFailed { .. } => codes::NIKA_1109,
        }
    }

    /// True for retryable failures (transient inference / task) · false for
    /// structural ones (not wired · model missing · bad frame · bad region).
    fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::DetectionFailed { .. }
                | Self::RecognitionFailed { .. }
                | Self::TaskJoinFailed { .. }
        )
    }
}

/// Bridge to `std::io::Error` at the `OcrEngine` trait boundary (the trait
/// returns `std::io::Result`). The rich `OcrError` rides as the boxed source ·
/// the NIKA code reaches logs via `Display`.
impl From<OcrError> for std::io::Error {
    fn from(err: OcrError) -> Self {
        std::io::Error::other(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_kernel::prelude::{Category, NikaErrorCode, codes};

    fn all_variants() -> Vec<OcrError> {
        vec![
            OcrError::ModelNotFound {
                path: "m.rten".into(),
            },
            OcrError::ModelLoadFailed { reason: "x".into() },
            OcrError::EngineInit { reason: "x".into() },
            OcrError::RegionOutOfBounds {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
                frame_w: 1,
                frame_h: 1,
            },
            OcrError::InvalidFrameFormat { reason: "x".into() },
            OcrError::PrepareInputFailed { reason: "x".into() },
            OcrError::DetectionFailed { reason: "x".into() },
            OcrError::RecognitionFailed { reason: "x".into() },
            OcrError::TaskJoinFailed { reason: "x".into() },
        ]
    }

    #[test]
    fn codes_are_unique_and_in_range() {
        let variants = all_variants();
        let n = variants.len();
        let mut nums: Vec<u16> = variants.iter().map(|e| e.nika_code().num).collect();
        nums.sort_unstable();
        nums.dedup();
        assert_eq!(nums.len(), n, "all NIKA codes are unique");
        for e in &variants {
            let c = e.nika_code();
            assert!((1100..=1199).contains(&c.num), "{c} in nika-ocr range");
            assert_eq!(c.category, Category::Ocr);
            assert!(
                codes::lookup(&c.to_string()).is_some(),
                "{c} resolvable via the registry"
            );
        }
    }

    #[test]
    fn from_ocr_error_to_io_preserves_source() {
        let io: std::io::Error = OcrError::ModelNotFound {
            path: "m.rten".into(),
        }
        .into();
        let src = io.into_inner().expect("boxed source");
        let oe = src.downcast::<OcrError>().expect("OcrError source");
        assert_eq!(oe.nika_code(), codes::NIKA_1101);
    }

    #[test]
    fn transient_classification() {
        assert!(OcrError::RecognitionFailed { reason: "x".into() }.is_transient());
        assert!(OcrError::DetectionFailed { reason: "x".into() }.is_transient());
        assert!(OcrError::TaskJoinFailed { reason: "x".into() }.is_transient());
        assert!(!OcrError::ModelLoadFailed { reason: "x".into() }.is_transient());
        assert!(!OcrError::ModelNotFound { path: "m".into() }.is_transient());
        assert!(
            !OcrError::RegionOutOfBounds {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
                frame_w: 1,
                frame_h: 1
            }
            .is_transient()
        );
    }
}
