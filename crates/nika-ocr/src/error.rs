// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! OCR backend errors — **re-exported from the kernel** (Pattern A · FCI-023bis).
//!
//! `OcrError` moved to `nika_kernel::io::ocr` 2026-06-10 so the `OcrEngine` trait
//! returns it directly — the typed NIKA taxonomy survives the contract boundary
//! instead of being erased by `std::io::Error`. The old
//! `From<OcrError> for std::io::Error` is gone (no `io::Result` boundary remains).
//! Codes are `Category::Ocr` · NIKA-1101..1109, with the `NikaErrorCode` impl in
//! `nika_kernel::errors`. This re-export keeps the ergonomic
//! `crate::error::OcrError` path within `nika-ocr`.

pub use nika_kernel::io::ocr::OcrError;

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
                frame_h: 1,
            }
            .is_transient()
        );
    }
}
