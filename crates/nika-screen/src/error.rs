// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Screen-capture errors — NIKA-1000..1099 reserved range (computer-use L1).
//!
//! Codes are exposed via the workspace-canonical [`NikaErrorCode`] trait
//! (`nika_code()` → `nika_error::codes::NIKA_1000..1009` · `Category::Screen`
//! · B5 error one-voice 2026-06-10 — the crate-local string `code()`
//! accessor was the F8 drift · removed per `no-legacy-no-back-compat.md`).
//! The NIKA-1000..1099 range is reserved per ADR-081 +
//! `forward-compat-invariants.md`.
//!
//! [`NikaErrorCode`]: nika_kernel::prelude::NikaErrorCode
//!
//! At the `ScreenCapture` trait boundary the kernel returns `std::io::Result`,
//! so [`ScreenError`] converts into [`std::io::Error`] via [`From`] — the rich
//! crate-local error (with its NIKA code in `Display`) rides inside the boxed
//! `io::Error` source for callers that downcast.

/// Errors from the `nika-screen` L1 effect crate.
///
/// `#[non_exhaustive]` per FCI-3 + Invariant #19 — downstream matches MUST use
/// a wildcard arm; new variants land additively on MINOR.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum ScreenError {
    /// Capture backend not yet wired — B.2 skeleton placeholder.
    ///
    /// Closed at B.3 when the `xcap` single-shot impl ships (skeleton-option-a
    /// closure ceremony · 1-cascade window).
    #[error("screen-capture backend not wired (skeleton · pending xcap impl)")]
    BackendNotWired,

    /// Requested display id was not found among the connected displays.
    #[error("display not found: id {id}")]
    DisplayNotFound {
        /// The display identifier that did not resolve.
        id: u32,
    },

    /// No displays are connected / enumerable.
    #[error("no displays found")]
    NoDisplaysFound,

    /// The OS capture call failed.
    #[error("capture failed: {reason}")]
    CaptureFailed {
        /// Human-readable cause from the OS backend.
        reason: String,
    },

    /// The requested sub-region falls outside the display bounds.
    #[error(
        "region out of bounds: {width}x{height} at ({x},{y}) exceeds display {display_w}x{display_h}"
    )]
    RegionOutOfBounds {
        /// Region x offset (physical px · top-left origin).
        x: i32,
        /// Region y offset.
        y: i32,
        /// Region width.
        width: u32,
        /// Region height.
        height: u32,
        /// Display width.
        display_w: u32,
        /// Display height.
        display_h: u32,
    },

    /// The captured frame had an unexpected pixel format / size.
    #[error("invalid frame format: {reason}")]
    InvalidFrameFormat {
        /// Why the frame was rejected.
        reason: String,
    },

    /// Capture was attempted without (or after losing) user consent.
    #[error("screen-capture consent denied")]
    ConsentDenied,

    /// Consent was revoked mid-capture — the active stream is torn down.
    #[error("screen-capture consent revoked mid-capture")]
    ConsentRevoked,

    /// The capture-LED indicator could not be engaged (guard 6).
    #[error("capture indicator unavailable: {reason}")]
    IndicatorUnavailable {
        /// Why the indicator could not be engaged.
        reason: String,
    },

    /// The capture backend failed to initialize.
    #[error("capture backend init failed: {reason}")]
    BackendInit {
        /// Why initialization failed.
        reason: String,
    },
}

impl nika_kernel::prelude::NikaErrorCode for ScreenError {
    /// Stable NIKA code (registry-owned · `Category::Screen` 1000-1099).
    fn nika_code(&self) -> nika_kernel::prelude::NikaCode {
        use nika_kernel::prelude::codes;
        match self {
            Self::BackendNotWired => codes::NIKA_1000,
            Self::DisplayNotFound { .. } => codes::NIKA_1001,
            Self::NoDisplaysFound => codes::NIKA_1002,
            Self::CaptureFailed { .. } => codes::NIKA_1003,
            Self::RegionOutOfBounds { .. } => codes::NIKA_1004,
            Self::InvalidFrameFormat { .. } => codes::NIKA_1005,
            Self::ConsentDenied => codes::NIKA_1006,
            Self::ConsentRevoked => codes::NIKA_1007,
            Self::IndicatorUnavailable { .. } => codes::NIKA_1008,
            Self::BackendInit { .. } => codes::NIKA_1009,
        }
    }

    /// Capture/init failures may be transient (device contention · GPU
    /// busy); consent + region + format errors are structural.
    fn is_transient(&self) -> bool {
        matches!(self, Self::CaptureFailed { .. } | Self::BackendInit { .. })
    }
}

impl From<ScreenError> for std::io::Error {
    /// Box the rich crate-local error into `io::Error` for the `ScreenCapture`
    /// trait boundary (the kernel returns `std::io::Result`). The NIKA code
    /// rides in `Display`; callers can downcast the source to `ScreenError`.
    fn from(err: ScreenError) -> Self {
        std::io::Error::other(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_kernel::prelude::{NikaErrorCode, codes};
    use std::collections::BTreeSet;

    fn all_variants() -> [ScreenError; 10] {
        [
            ScreenError::BackendNotWired,
            ScreenError::DisplayNotFound { id: 0 },
            ScreenError::NoDisplaysFound,
            ScreenError::CaptureFailed { reason: "x".into() },
            ScreenError::RegionOutOfBounds {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
                display_w: 1,
                display_h: 1,
            },
            ScreenError::InvalidFrameFormat { reason: "x".into() },
            ScreenError::ConsentDenied,
            ScreenError::ConsentRevoked,
            ScreenError::IndicatorUnavailable { reason: "x".into() },
            ScreenError::BackendInit { reason: "x".into() },
        ]
    }

    /// Every variant maps to a distinct REGISTRY code in the reserved
    /// range · `Category::Screen` (one-voice · B5).
    #[test]
    fn nika_codes_unique_in_range_and_registered() {
        let errs = all_variants();
        let nums: BTreeSet<u16> = errs.iter().map(|e| e.nika_code().num).collect();
        assert_eq!(nums.len(), errs.len(), "all codes distinct");
        for e in &errs {
            let c = e.nika_code();
            assert!((1000..=1099).contains(&c.num), "{c} in reserved range");
            assert_eq!(c.category, nika_kernel::prelude::Category::Screen);
            assert!(
                codes::lookup(&c.to_string()).is_some(),
                "{c} resolvable via the registry"
            );
        }
    }

    /// Pinned wire codes — the grep-anchor contract survives the trait
    /// migration byte-for-byte (`Display` of `NikaCode` == old string codes).
    #[test]
    fn wire_codes_pinned() {
        assert_eq!(ScreenError::ConsentDenied.nika_code(), codes::NIKA_1006);
        assert_eq!(ScreenError::ConsentRevoked.nika_code(), codes::NIKA_1007);
        assert_eq!(
            ScreenError::ConsentDenied.nika_code().to_string(),
            "NIKA-1006"
        );
        assert_eq!(
            ScreenError::BackendNotWired.nika_code().to_string(),
            "NIKA-1000"
        );
    }

    /// Transient classification — capture/init retryable, the rest structural.
    #[test]
    fn transient_classification() {
        assert!(
            ScreenError::CaptureFailed {
                reason: "busy".into()
            }
            .is_transient()
        );
        assert!(ScreenError::BackendInit { reason: "x".into() }.is_transient());
        assert!(!ScreenError::ConsentDenied.is_transient());
        assert!(!ScreenError::BackendNotWired.is_transient());
        assert!(!ScreenError::DisplayNotFound { id: 3 }.is_transient());
    }

    /// Distinct codes hash to distinct fingerprints (R4 · default impl).
    #[test]
    fn fingerprints_distinct_per_code() {
        let errs = all_variants();
        let fps: BTreeSet<u64> = errs.iter().map(NikaErrorCode::fingerprint).collect();
        assert_eq!(fps.len(), errs.len());
    }

    /// `Display` carries the human message; conversion to `io::Error` preserves
    /// the source so callers can downcast back to `ScreenError`.
    #[test]
    fn converts_to_io_error_preserving_source() {
        let err = ScreenError::DisplayNotFound { id: 7 };
        let msg = err.to_string();
        assert!(msg.contains("display not found"), "display message");
        let io: std::io::Error = err.into();
        let src = io.into_inner().expect("boxed source present");
        let back = src
            .downcast::<ScreenError>()
            .expect("downcast to ScreenError");
        assert_eq!(back.nika_code(), codes::NIKA_1001);
    }
}
