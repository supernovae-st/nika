// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `NikaErrorCode` implementations for the AI-owned kernel error types
//! (orphan-rule home · the impls live with their types).
//!
//! AI-owned ranges (full cross-domain registry in the hub
//! `nika-kernel/src/errors.rs` module doc):
//! - 330–379: Provider (`ProviderError`)
//! - 600–649: Memory (`MemoryError` · impl co-located in `memory.rs` ·
//!   constants in `nika_error::codes::NIKA_601..604`)
//! - 1500–1599: Vision (`VisionError` · `vision.rs` · constants in
//!   `nika_error::codes::NIKA_1501..1505` · ADR-081 M2.6)
//! - 1600–1699: Audio (`AudioError` · `audio.rs` · constants in
//!   `nika_error::codes::NIKA_1601..1605` · FCI-005 audio block)

use nika_error::prelude::*;

use crate::audio::AudioError;
use crate::provider::ProviderError;
use crate::vision::VisionError;

// Provider: 330–379 (moved from 380-429 2026-05-11 · Shield slot free)
/// Provider API error.
pub const NIKA_330: NikaCode = NikaCode {
    num: 330,
    category: Category::Provider,
    severity: Severity::Error,
    slug: "provider-api",
};
/// Model not found.
pub const NIKA_331: NikaCode = NikaCode {
    num: 331,
    category: Category::Provider,
    severity: Severity::Error,
    slug: "provider-model-not-found",
};
/// Rate limited.
pub const NIKA_332: NikaCode = NikaCode {
    num: 332,
    category: Category::Provider,
    severity: Severity::Error,
    slug: "provider-rate-limited",
};
/// Authentication failed.
pub const NIKA_333: NikaCode = NikaCode {
    num: 333,
    category: Category::Provider,
    severity: Severity::Error,
    slug: "provider-auth-failed",
};
/// Provider other.
pub const NIKA_339: NikaCode = NikaCode {
    num: 339,
    category: Category::Provider,
    severity: Severity::Error,
    slug: "provider-other",
};

// ─── NikaErrorCode implementations ──────────────────────────────────

impl NikaErrorCode for ProviderError {
    fn nika_code(&self) -> NikaCode {
        match self {
            Self::Api { .. } => NIKA_330,
            Self::ModelNotFound { .. } => NIKA_331,
            Self::RateLimited { .. } => NIKA_332,
            Self::AuthFailed { .. } => NIKA_333,
            Self::Other { .. } => NIKA_339,
        }
    }

    fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::RateLimited { .. }
                | Self::Api {
                    status: 500..=599,
                    ..
                }
        )
    }
}

impl NikaErrorCode for VisionError {
    fn nika_code(&self) -> NikaCode {
        use nika_error::codes;
        match self {
            Self::ModelUnavailable { .. } => codes::NIKA_1501,
            Self::InvalidInput { .. } => codes::NIKA_1502,
            Self::InferenceFailed { .. } => codes::NIKA_1503,
            Self::BackendUnavailable => codes::NIKA_1504,
            Self::TaskJoinFailed { .. } => codes::NIKA_1505,
        }
    }

    /// Inference + join failures may be transient (backend contention ·
    /// worker churn); model absence, invalid input, and a missing backend
    /// are structural.
    fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::InferenceFailed { .. } | Self::TaskJoinFailed { .. }
        )
    }
}

impl NikaErrorCode for AudioError {
    fn nika_code(&self) -> NikaCode {
        use nika_error::codes;
        match self {
            Self::ModelUnavailable { .. } => codes::NIKA_1601,
            Self::InvalidInput { .. } => codes::NIKA_1602,
            Self::InferenceFailed { .. } => codes::NIKA_1603,
            Self::BackendUnavailable => codes::NIKA_1604,
            Self::TaskJoinFailed { .. } => codes::NIKA_1605,
        }
    }

    /// Same transience split as `VisionError` (one capability-family canon).
    fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::InferenceFailed { .. } | Self::TaskJoinFailed { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::MemoryError;

    #[test]
    fn provider_error_codes_in_range() {
        let err = ProviderError::ModelNotFound { model: "x".into() };
        let code = err.nika_code();
        assert!(
            code.num >= 330 && code.num <= 379,
            "provider code {}",
            code.num
        );
        assert_eq!(code.category, Category::Provider);
    }

    #[test]
    fn provider_rate_limited_is_transient() {
        let err = ProviderError::RateLimited {
            retry_after_ms: None,
        };
        assert!(NikaErrorCode::is_transient(&err));
    }

    #[test]
    fn provider_auth_not_transient() {
        let err = ProviderError::AuthFailed {
            reason: "bad key".into(),
        };
        assert!(!NikaErrorCode::is_transient(&err));
    }

    #[test]
    fn all_provider_variants_have_codes() {
        let _ = ProviderError::Api {
            status: 500,
            message: "".into(),
        }
        .nika_code();
        let _ = ProviderError::ModelNotFound { model: "".into() }.nika_code();
        let _ = ProviderError::RateLimited {
            retry_after_ms: None,
        }
        .nika_code();
        let _ = ProviderError::AuthFailed { reason: "".into() }.nika_code();
        let _ = ProviderError::Other { reason: "".into() }.nika_code();
    }

    #[test]
    fn memory_error_codes_in_range() {
        let err = MemoryError::Unavailable { reason: "x".into() };
        let code = err.nika_code();
        assert!(
            code.num >= 600 && code.num <= 649,
            "memory code {}",
            code.num
        );
        assert_eq!(code.category, Category::Memory);
    }

    #[test]
    fn all_memory_variants_have_codes() {
        use crate::memory::MemoryId;
        let _ = MemoryError::Unavailable { reason: "".into() }.nika_code();
        let _ = MemoryError::NotFound {
            id: MemoryId::nil(),
        }
        .nika_code();
        let _ = MemoryError::EmbeddingFailed { reason: "".into() }.nika_code();
        let _ = MemoryError::Storage { reason: "".into() }.nika_code();
    }
}
