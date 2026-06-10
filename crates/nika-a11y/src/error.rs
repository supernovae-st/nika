// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Accessibility errors — **re-exported from the kernel** (Pattern A · FCI-023bis).
//!
//! `A11yError` moved to `nika_kernel::io::a11y` 2026-06-10 so the
//! `AccessibilityTree` trait returns it directly — the typed NIKA taxonomy
//! survives the contract boundary instead of being erased by `std::io::Error`.
//! The old `From<A11yError> for std::io::Error` is gone (no `io::Result` boundary
//! remains). Codes are `Category::A11y` · NIKA-1201..1206, with the
//! `NikaErrorCode` impl in `nika_kernel::errors`. This re-export keeps the
//! ergonomic `crate::error::A11yError` path within `nika-a11y`.

pub use nika_kernel::io::a11y::A11yError;

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
    fn transient_classification() {
        assert!(A11yError::AttributeError { reason: "x".into() }.is_transient());
        assert!(A11yError::TreeWalkFailed { reason: "x".into() }.is_transient());
        assert!(A11yError::TaskJoinFailed { reason: "x".into() }.is_transient());
        assert!(!A11yError::PermissionDenied.is_transient());
        assert!(!A11yError::NoFocusedApplication.is_transient());
        assert!(!A11yError::BackendUnavailable.is_transient());
    }
}
