// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Screen-capture errors — **re-exported from the kernel** (Pattern A · FCI-023bis).
//!
//! `ScreenError` moved to `nika_kernel::io::screen` 2026-06-10 so the
//! `ScreenCapture` trait returns it directly — the typed NIKA taxonomy survives
//! the contract boundary instead of being erased by `std::io::Error`. The old
//! `From<ScreenError> for std::io::Error` is gone (no `io::Result` boundary
//! remains). Codes are `Category::Screen` · NIKA-1000..1009, with the
//! `NikaErrorCode` impl in `nika_kernel::errors`. This re-export keeps the
//! ergonomic `crate::error::ScreenError` path within `nika-screen`.

pub use nika_kernel::io::screen::ScreenError;

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
}
