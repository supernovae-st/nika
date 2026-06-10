// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `NikaErrorCode` implementations for all kernel error types.
//!
//! Error code ranges:
//! - 050–099: Shell/exec (`ShellError`)
//! - 100–139: File/IO (`BlobError`)
//! - 140–189: Http/network (`HttpError`)
//! - 230–279: MCP/tools (`ToolExecError`)
//! - 330–379: Provider (`ProviderError`) (moved from 380-429 2026-05-11 to
//!   free the Shield-reserved 380-389 slot per `dx/.claude/rules/security.md`
//!   §"Nika Shield" canonical mapping. Historical `NIKA_380..389` names dropped
//!   per `no-legacy-no-back-compat.md` · zero alias · git is archive.)
//! - 380–429: Shield (`ShieldError`) — RESERVED, per `nika/SECURITY.md`
//!   6-layer defense stack. `NIKA-380` `CapabilityDenied` · `NIKA-381`
//!   `TrustViolation` · `NIKA-382` `CanaryLeaked` · `NIKA-383`
//!   `InjectionDetected` · `NIKA-384` `SpotlightRequired` · `NIKA-385`
//!   `MlModelMissing` · `NIKA-386` `RunDepthExceeded` · `NIKA-387`
//!   `RunCycleDetected` · `NIKA-388` `CanaryInThinking` · `NIKA-389`
//!   `UntrustedVisionBlocked`. Crate not yet admitted — slot LOCKED.
//! - 600–649: Memory (`MemoryError`)
//! - 700–749: WASM plugin (`WasmPluginError`)
//! - 750–799: Sandbox (`SandboxError`)
//! - 800–819: Observability range (reserved for v0.100 `OTel` adapter; no
//!   kernel type maps to it today — `ObservabilitySink` was dropped per Q12)

// Core-owned constants + impls moved to `nika-kernel-core` with their
// types (orphan rule) · re-exported so `nika_kernel::errors::NIKA_050`
// stays a valid path.
pub use nika_kernel_core::errors::*;

use nika_error::prelude::*;

#[cfg(test)]
use crate::memory::MemoryError;
use crate::plugin::WasmPluginError;
use crate::provider::ProviderError;
use crate::sandbox::SandboxError;
use crate::tool_executor::ToolExecError;

// ─── Error code constants ────────────────────────────────────────────

// MCP/tools: 230–279
/// Tool not found.
pub const NIKA_230: NikaCode = NikaCode {
    num: 230,
    category: Category::Mcp,
    severity: Severity::Error,
    slug: "tool-not-found",
};
/// Tool timeout.
pub const NIKA_231: NikaCode = NikaCode {
    num: 231,
    category: Category::Mcp,
    severity: Severity::Error,
    slug: "tool-timeout",
};
/// Tool execution failed.
pub const NIKA_232: NikaCode = NikaCode {
    num: 232,
    category: Category::Mcp,
    severity: Severity::Error,
    slug: "tool-exec-failed",
};
/// Tool not available.
pub const NIKA_233: NikaCode = NikaCode {
    num: 233,
    category: Category::Mcp,
    severity: Severity::Error,
    slug: "tool-not-available",
};

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

// Memory: 600–649 codes live in `nika_error::codes` (NIKA_601..604 wired
// Diamond W2.1). The kernel-local NIKA_600..603 placeholders were DRIFT
// (off-by-one + duplicate slug surface) and removed per Diamond W2.2 /
// no-legacy-no-back-compat. Use `nika_error::codes::NIKA_601..604`
// directly · MemoryError impl is in ai/memory.rs.

// ─── NikaErrorCode implementations ──────────────────────────────────

impl NikaErrorCode for ToolExecError {
    fn nika_code(&self) -> NikaCode {
        match self {
            Self::NotFound { .. } => NIKA_230,
            Self::Timeout { .. } => NIKA_231,
            Self::ExecutionFailed { .. } => NIKA_232,
            Self::NotAvailable { .. } => NIKA_233,
        }
    }
}

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

// MemoryError NikaErrorCode impl lives in ai/memory.rs co-located with the
// enum definition · maps to NIKA_601..604 per Diamond W2.2 (plan §2 v1.1
// amendment). Previous mapping 600..603 in this file was DRIFT vs the
// canonical sub-allocation table · removed per no-legacy-no-back-compat.

impl NikaErrorCode for WasmPluginError {
    fn nika_code(&self) -> NikaCode {
        codes::NIKA_700
    }
}

impl NikaErrorCode for SandboxError {
    fn nika_code(&self) -> NikaCode {
        codes::NIKA_750
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_exec_error_codes_in_range() {
        let err = ToolExecError::NotFound { name: "x".into() };
        let code = err.nika_code();
        assert!(code.num >= 230 && code.num <= 279, "tool code {}", code.num);
        assert_eq!(code.category, Category::Mcp);
    }

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

    #[test]
    fn wasm_plugin_error_code_in_range() {
        let err = WasmPluginError::NotFound { name: "x".into() };
        let code = err.nika_code();
        assert!(code.num >= 700 && code.num <= 749, "wasm code {}", code.num);
        assert_eq!(code.category, Category::WasmPlugin);
    }

    #[test]
    fn sandbox_error_code_in_range() {
        let err = SandboxError::Unavailable { reason: "x".into() };
        let code = err.nika_code();
        assert!(
            code.num >= 750 && code.num <= 799,
            "sandbox code {}",
            code.num
        );
        assert_eq!(code.category, Category::Sandbox);
    }
}
