// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Screen-capture errors — NIKA-1000..1099 reserved range (computer-use L1).
//!
//! Codes are exposed via the crate-local [`ScreenError::code`] accessor.
//! Central `nika-error` `Category` integration (the `nika-schema` pattern)
//! is a deferred follow-up — it would add a `Category` variant to the L0
//! `nika-error` crate, outside the M2.1 B.2 scope-lock. The NIKA-1000..1099
//! range is reserved per ADR-081 + `forward-compat-invariants.md`.
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

impl ScreenError {
    /// Stable NIKA code for this error (grep-anchor for logs + journal).
    ///
    /// NIKA-1000..1009 · the reserved nika-screen sub-range (ADR-081).
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::BackendNotWired => "NIKA-1000",
            Self::DisplayNotFound { .. } => "NIKA-1001",
            Self::NoDisplaysFound => "NIKA-1002",
            Self::CaptureFailed { .. } => "NIKA-1003",
            Self::RegionOutOfBounds { .. } => "NIKA-1004",
            Self::InvalidFrameFormat { .. } => "NIKA-1005",
            Self::ConsentDenied => "NIKA-1006",
            Self::ConsentRevoked => "NIKA-1007",
            Self::IndicatorUnavailable { .. } => "NIKA-1008",
            Self::BackendInit { .. } => "NIKA-1009",
        }
    }

    /// Whether the error is transient — safe to retry with backoff.
    ///
    /// Capture/init failures may be transient (device contention · GPU busy);
    /// consent + region + format errors are structural (retry won't help).
    #[must_use]
    pub fn is_transient(&self) -> bool {
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
    use std::collections::BTreeSet;

    /// Every variant has a distinct NIKA code in the 1000..1009 range.
    #[test]
    fn codes_are_unique_and_in_range() {
        let errs = [
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
        ];
        let codes: BTreeSet<&str> = errs.iter().map(ScreenError::code).collect();
        assert_eq!(codes.len(), errs.len(), "all codes distinct");
        for c in &codes {
            let n: u32 = c
                .strip_prefix("NIKA-")
                .expect("NIKA- prefix")
                .parse()
                .expect("number");
            assert!((1000..=1099).contains(&n), "{c} in reserved range");
        }
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
        assert_eq!(back.code(), "NIKA-1001");
    }
}
