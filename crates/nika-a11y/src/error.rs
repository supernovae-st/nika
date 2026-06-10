// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Accessibility error taxonomy — NIKA-1201..1206.
//!
//! Reserved sub-range **NIKA-1200..1299** per ADR-081 `nika_codes` matrix
//! (supersedes the stale `io/a11y.rs` doc-comment "NIKA-1060..1079" which
//! predates ADR-081 · same reconciliation as nika-ocr NIKA-1100..1199).
//! Codes speak the workspace-canonical [`NikaErrorCode`] trait
//! (`nika_code()` → registry consts `NIKA_1201..1206` · `Category::A11y`
//! · B5 error one-voice 2026-06-10 — the crate-local string `code()`
//! accessor was the F8 drift · removed per
//! `no-legacy-no-back-compat.md`); `is_transient()` lets a caller decide
//! retry vs structural failure.
//!
//! [`NikaErrorCode`]: nika_kernel::prelude::NikaErrorCode
//!
//! NIKA-1200 was the B.2 `BackendNotWired` skeleton placeholder · CLOSED at
//! B.3 when the macOS `AXUIElement` walk was wired (per
//! `skeleton-option-a-pattern.md` §5) · the slot stays reserved.

use thiserror::Error;

/// Accessibility backend errors · NIKA-1201..1206 (registry-owned ·
/// `Category::A11y`).
#[derive(Debug, Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum A11yError {
    /// The process lacks the OS accessibility grant (macOS Accessibility
    /// trust · `AXIsProcessTrusted` false) — the operator must grant it.
    #[error("NIKA-1201 · accessibility permission denied (grant Accessibility access)")]
    PermissionDenied,
    /// No focused/active application to snapshot.
    #[error("NIKA-1202 · no focused application")]
    NoFocusedApplication,
    /// Reading an `AXUIElement` attribute failed.
    #[error("NIKA-1203 · accessibility attribute error: {reason}")]
    AttributeError {
        /// Backend-reported reason.
        reason: String,
    },
    /// Walking the accessibility tree failed mid-traversal.
    #[error("NIKA-1204 · accessibility tree walk failed: {reason}")]
    TreeWalkFailed {
        /// Backend-reported reason.
        reason: String,
    },
    /// No accessibility backend is compiled for this platform (non-macOS
    /// until Linux AT-SPI / Windows UIA backends land · §4 macOS-first).
    #[error("NIKA-1205 · no accessibility backend on this platform")]
    BackendUnavailable,
    /// A `spawn_blocking` walk task panicked or was cancelled.
    #[error("NIKA-1206 · accessibility task join failed: {reason}")]
    TaskJoinFailed {
        /// Join failure detail.
        reason: String,
    },
}

impl nika_kernel::prelude::NikaErrorCode for A11yError {
    /// Stable NIKA code (registry-owned · `Category::A11y` 1200-1299 ·
    /// NIKA-1200 = retired B.2 placeholder slot).
    fn nika_code(&self) -> nika_kernel::prelude::NikaCode {
        use nika_kernel::prelude::codes;
        match self {
            Self::PermissionDenied => codes::NIKA_1201,
            Self::NoFocusedApplication => codes::NIKA_1202,
            Self::AttributeError { .. } => codes::NIKA_1203,
            Self::TreeWalkFailed { .. } => codes::NIKA_1204,
            Self::BackendUnavailable => codes::NIKA_1205,
            Self::TaskJoinFailed { .. } => codes::NIKA_1206,
        }
    }

    /// Attribute/walk/join failures may be transient (focus churn · app
    /// teardown mid-walk) · permission + backend absence are structural.
    fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::AttributeError { .. } | Self::TreeWalkFailed { .. } | Self::TaskJoinFailed { .. }
        )
    }
}

/// Bridge to `std::io::Error` at the `AccessibilityTree` trait boundary (the
/// trait returns `std::io::Result`). The rich `A11yError` rides as the boxed
/// source · the NIKA code reaches logs via `Display`.
impl From<A11yError> for std::io::Error {
    fn from(err: A11yError) -> Self {
        std::io::Error::other(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_kernel::prelude::{Category, NikaErrorCode, codes};

    fn all_variants() -> Vec<A11yError> {
        vec![
            A11yError::PermissionDenied,
            A11yError::NoFocusedApplication,
            A11yError::AttributeError { reason: "x".into() },
            A11yError::TreeWalkFailed { reason: "x".into() },
            A11yError::BackendUnavailable,
            A11yError::TaskJoinFailed { reason: "x".into() },
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
            assert!((1200..=1299).contains(&c.num), "{c} in nika-a11y range");
            assert_eq!(c.category, Category::A11y);
            assert!(
                codes::lookup(&c.to_string()).is_some(),
                "{c} resolvable via the registry"
            );
        }
    }

    #[test]
    fn from_a11y_error_to_io_preserves_source() {
        let io: std::io::Error = A11yError::PermissionDenied.into();
        let src = io.into_inner().expect("boxed source");
        let ae = src.downcast::<A11yError>().expect("A11yError source");
        assert_eq!(ae.nika_code(), codes::NIKA_1201);
    }

    #[test]
    fn transient_classification() {
        assert!(A11yError::AttributeError { reason: "x".into() }.is_transient());
        assert!(A11yError::TreeWalkFailed { reason: "x".into() }.is_transient());
        assert!(A11yError::TaskJoinFailed { reason: "x".into() }.is_transient());
        assert!(!A11yError::PermissionDenied.is_transient());
        assert!(!A11yError::NoFocusedApplication.is_transient());
        assert!(!A11yError::BackendUnavailable.is_transient());
    }
}
