// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Accessibility error taxonomy — NIKA-1201..1206.
//!
//! Reserved sub-range **NIKA-1200..1299** per ADR-081 `nika_codes` matrix
//! (supersedes the stale `io/a11y.rs` doc-comment "NIKA-1060..1079" which
//! predates ADR-081 · same reconciliation as nika-ocr NIKA-1100..1199).
//! `code()` is the grep-anchor for logs + journal; `is_transient()` lets a
//! caller decide retry vs structural failure.
//!
//! NIKA-1200 was the B.2 `BackendNotWired` skeleton placeholder · CLOSED at
//! B.3 when the macOS `AXUIElement` walk was wired (per
//! `skeleton-option-a-pattern.md` §5) · the slot stays reserved.

use thiserror::Error;

/// Accessibility backend errors · NIKA-1201..1206 · `code()` grep-anchor.
#[derive(Debug, Error)]
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

impl A11yError {
    /// Stable NIKA code for this error (grep-anchor for logs + journal).
    ///
    /// NIKA-1201..1206 currently used · NIKA-1200..1299 reserved for
    /// nika-a11y (ADR-081 · NIKA-1200 = retired B.2 placeholder slot).
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::PermissionDenied => "NIKA-1201",
            Self::NoFocusedApplication => "NIKA-1202",
            Self::AttributeError { .. } => "NIKA-1203",
            Self::TreeWalkFailed { .. } => "NIKA-1204",
            Self::BackendUnavailable => "NIKA-1205",
            Self::TaskJoinFailed { .. } => "NIKA-1206",
        }
    }

    /// True for retryable failures (transient attribute / walk / task) · false
    /// for structural ones (not wired · permission · no app · no backend).
    #[must_use]
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::AttributeError { .. } | Self::TreeWalkFailed { .. } | Self::TaskJoinFailed { .. }
        )
    }
}

/// Bridge to `std::io::Error` at the `AccessibilityTree` trait boundary (the
/// trait returns `std::io::Result`). The rich `A11yError` rides as the boxed
/// source · `code()` reaches logs via `Display`.
impl From<A11yError> for std::io::Error {
    fn from(err: A11yError) -> Self {
        std::io::Error::other(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let mut codes: Vec<&str> = variants.iter().map(A11yError::code).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), n, "all NIKA codes are unique");
        for e in &variants {
            let num: u32 = e
                .code()
                .strip_prefix("NIKA-")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            assert!(
                (1200..=1299).contains(&num),
                "{} in nika-a11y range",
                e.code()
            );
        }
    }

    #[test]
    fn from_a11y_error_to_io_preserves_source() {
        let io: std::io::Error = A11yError::PermissionDenied.into();
        let src = io.into_inner().expect("boxed source");
        let ae = src.downcast::<A11yError>().expect("A11yError source");
        assert_eq!(ae.code(), "NIKA-1201");
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
