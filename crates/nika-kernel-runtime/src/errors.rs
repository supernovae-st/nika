// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `NikaErrorCode` implementations for the RUNTIME-owned kernel error
//! types (orphan-rule home · the impls live with their types).
//!
//! Runtime-owned range (full cross-domain registry in the hub
//! `nika-kernel/src/errors.rs` module doc):
//! - 230–279: MCP/tools (`ToolExecError`)

use nika_error::prelude::*;

use crate::tool_executor::ToolExecError;

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
}
