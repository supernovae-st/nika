// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `NikaErrorCode` implementations for all kernel error types.
//!
//! Error code ranges:
//! - 050–099: Shell/exec (`ShellError`)
//! - 100–139: File/IO (`BlobError`)
//! - 140–189: Http/network (`HttpError`)
//! - 230–279: MCP/tools (`ToolExecError`)
//! - 380–429: Provider (`ProviderError`)
//! - 600–649: Memory (`MemoryError`)
//! - 700–749: WASM plugin (`WasmPluginError`)
//! - 750–799: Sandbox (`SandboxError`)
//! - 800–819: Observability (`ObservabilityError`)

use nika_error::prelude::*;

use crate::blob::BlobError;
use crate::http::HttpError;
use crate::memory::MemoryError;
use crate::observability::ObservabilityError;
use crate::plugin::WasmPluginError;
use crate::process::ShellError;
use crate::provider::ProviderError;
use crate::sandbox::SandboxError;
use crate::tool_executor::ToolExecError;

// ─── Error code constants ────────────────────────────────────────────

// Shell/exec: 050–099
/// Program not found.
pub const NIKA_050: NikaCode = NikaCode {
    num: 50,
    category: Category::Shell,
    severity: Severity::Error,
    slug: "shell-not-found",
};
/// Shell execution timed out.
pub const NIKA_051: NikaCode = NikaCode {
    num: 51,
    category: Category::Shell,
    severity: Severity::Error,
    slug: "shell-timeout",
};
/// Shell execution cancelled.
pub const NIKA_052: NikaCode = NikaCode {
    num: 52,
    category: Category::Shell,
    severity: Severity::Error,
    slug: "shell-cancelled",
};
/// Command blocked by security.
pub const NIKA_053: NikaCode = NikaCode {
    num: 53,
    category: Category::Shell,
    severity: Severity::Error,
    slug: "shell-blocked",
};
/// Shell other error.
pub const NIKA_059: NikaCode = NikaCode {
    num: 59,
    category: Category::Shell,
    severity: Severity::Error,
    slug: "shell-other",
};

// File/IO (blob): 100–139
/// Blob not found.
pub const NIKA_100: NikaCode = NikaCode {
    num: 100,
    category: Category::FileIo,
    severity: Severity::Error,
    slug: "blob-not-found",
};
/// Blob I/O error.
pub const NIKA_101: NikaCode = NikaCode {
    num: 101,
    category: Category::FileIo,
    severity: Severity::Error,
    slug: "blob-io",
};
/// Blob too large.
pub const NIKA_102: NikaCode = NikaCode {
    num: 102,
    category: Category::FileIo,
    severity: Severity::Error,
    slug: "blob-too-large",
};

// Http/network: 140–189
/// HTTP timeout.
pub const NIKA_140: NikaCode = NikaCode {
    num: 140,
    category: Category::Http,
    severity: Severity::Error,
    slug: "http-timeout",
};
/// HTTP connection failed.
pub const NIKA_141: NikaCode = NikaCode {
    num: 141,
    category: Category::Http,
    severity: Severity::Error,
    slug: "http-connection",
};
/// SSRF blocked.
pub const NIKA_142: NikaCode = NikaCode {
    num: 142,
    category: Category::Http,
    severity: Severity::Error,
    slug: "http-ssrf-blocked",
};
/// HTTP response too large.
pub const NIKA_143: NikaCode = NikaCode {
    num: 143,
    category: Category::Http,
    severity: Severity::Error,
    slug: "http-too-large",
};
/// HTTP unsupported.
pub const NIKA_144: NikaCode = NikaCode {
    num: 144,
    category: Category::Http,
    severity: Severity::Error,
    slug: "http-unsupported",
};
/// HTTP other.
pub const NIKA_149: NikaCode = NikaCode {
    num: 149,
    category: Category::Http,
    severity: Severity::Error,
    slug: "http-other",
};

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

// Provider: 380–429
/// Provider API error.
pub const NIKA_380: NikaCode = NikaCode {
    num: 380,
    category: Category::Provider,
    severity: Severity::Error,
    slug: "provider-api",
};
/// Model not found.
pub const NIKA_381: NikaCode = NikaCode {
    num: 381,
    category: Category::Provider,
    severity: Severity::Error,
    slug: "provider-model-not-found",
};
/// Rate limited.
pub const NIKA_382: NikaCode = NikaCode {
    num: 382,
    category: Category::Provider,
    severity: Severity::Error,
    slug: "provider-rate-limited",
};
/// Authentication failed.
pub const NIKA_383: NikaCode = NikaCode {
    num: 383,
    category: Category::Provider,
    severity: Severity::Error,
    slug: "provider-auth-failed",
};
/// Provider other.
pub const NIKA_389: NikaCode = NikaCode {
    num: 389,
    category: Category::Provider,
    severity: Severity::Error,
    slug: "provider-other",
};

// Memory: 600–649
/// Memory unavailable.
pub const NIKA_600: NikaCode = NikaCode {
    num: 600,
    category: Category::Memory,
    severity: Severity::Error,
    slug: "memory-unavailable",
};
/// Memory not found.
pub const NIKA_601: NikaCode = NikaCode {
    num: 601,
    category: Category::Memory,
    severity: Severity::Error,
    slug: "memory-not-found",
};
/// Embedding failed.
pub const NIKA_602: NikaCode = NikaCode {
    num: 602,
    category: Category::Memory,
    severity: Severity::Error,
    slug: "memory-embedding-failed",
};
/// Memory storage error.
pub const NIKA_603: NikaCode = NikaCode {
    num: 603,
    category: Category::Memory,
    severity: Severity::Error,
    slug: "memory-storage",
};

// ─── NikaErrorCode implementations ──────────────────────────────────

impl NikaErrorCode for ShellError {
    fn nika_code(&self) -> NikaCode {
        match self {
            Self::NotFound { .. } => NIKA_050,
            Self::Timeout { .. } => NIKA_051,
            Self::Cancelled { .. } => NIKA_052,
            Self::Blocked { .. } => NIKA_053,
            Self::Other { .. } => NIKA_059,
        }
    }
}

impl NikaErrorCode for BlobError {
    fn nika_code(&self) -> NikaCode {
        match self {
            Self::NotFound { .. } => NIKA_100,
            Self::Io { .. } => NIKA_101,
            Self::TooLarge { .. } => NIKA_102,
        }
    }
}

impl NikaErrorCode for HttpError {
    fn nika_code(&self) -> NikaCode {
        match self {
            Self::Timeout { .. } => NIKA_140,
            Self::Connection { .. } => NIKA_141,
            Self::SsrfBlocked { .. } => NIKA_142,
            Self::TooLarge { .. } => NIKA_143,
            Self::Unsupported { .. } => NIKA_144,
            Self::Other { .. } => NIKA_149,
        }
    }
}

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
            Self::Api { .. } => NIKA_380,
            Self::ModelNotFound { .. } => NIKA_381,
            Self::RateLimited { .. } => NIKA_382,
            Self::AuthFailed { .. } => NIKA_383,
            Self::Other { .. } => NIKA_389,
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

impl NikaErrorCode for MemoryError {
    fn nika_code(&self) -> NikaCode {
        match self {
            Self::Unavailable { .. } => NIKA_600,
            Self::NotFound { .. } => NIKA_601,
            Self::EmbeddingFailed { .. } => NIKA_602,
            Self::Storage { .. } => NIKA_603,
        }
    }
}

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

impl NikaErrorCode for ObservabilityError {
    fn nika_code(&self) -> NikaCode {
        codes::NIKA_800
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_error_codes_in_range() {
        let err = ShellError::NotFound {
            program: "x".into(),
        };
        let code = err.nika_code();
        assert!(code.num >= 50 && code.num <= 99, "shell code {}", code.num);
        assert_eq!(code.category, Category::Shell);
    }

    #[test]
    fn blob_error_codes_in_range() {
        let err = BlobError::NotFound { hash: "x".into() };
        let code = err.nika_code();
        assert!(code.num >= 100 && code.num <= 139, "blob code {}", code.num);
        assert_eq!(code.category, Category::FileIo);
    }

    #[test]
    fn http_error_codes_in_range() {
        let err = HttpError::Timeout { duration_ms: 1000 };
        let code = err.nika_code();
        assert!(code.num >= 140 && code.num <= 189, "http code {}", code.num);
        assert_eq!(code.category, Category::Http);
    }

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
            code.num >= 380 && code.num <= 429,
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
    fn all_shell_variants_have_codes() {
        let _ = ShellError::NotFound { program: "".into() }.nika_code();
        let _ = ShellError::Timeout { duration_ms: 0 }.nika_code();
        let _ = ShellError::Cancelled { id: "".into() }.nika_code();
        let _ = ShellError::Blocked { reason: "".into() }.nika_code();
        let _ = ShellError::Other { reason: "".into() }.nika_code();
    }

    #[test]
    fn all_http_variants_have_codes() {
        let _ = HttpError::Timeout { duration_ms: 0 }.nika_code();
        let _ = HttpError::Connection { reason: "".into() }.nika_code();
        let _ = HttpError::SsrfBlocked { url: "".into() }.nika_code();
        let _ = HttpError::TooLarge { size: 0, max: 0 }.nika_code();
        let _ = HttpError::Unsupported { reason: "".into() }.nika_code();
        let _ = HttpError::Other { reason: "".into() }.nika_code();
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

    #[test]
    fn observability_error_code_in_range() {
        let err = ObservabilityError::NotConfigured { reason: "x".into() };
        let code = err.nika_code();
        assert!(
            code.num >= 800 && code.num <= 819,
            "observability code {}",
            code.num
        );
        assert_eq!(code.category, Category::Observability);
    }
}
