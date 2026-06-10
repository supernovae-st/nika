// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `NikaErrorCode` implementations for the PLUGIN-owned kernel error
//! types (orphan-rule home · the impls live with their types).
//!
//! Plugin-owned ranges (full cross-domain registry in the hub
//! `nika-kernel/src/errors.rs` module doc · constants live in
//! `nika_error::codes`):
//! - 700–749: WASM plugin (`WasmPluginError`)
//! - 750–799: Sandbox (`SandboxError`)

use nika_error::prelude::*;

use crate::sandbox::SandboxError;
use crate::wasm::WasmPluginError;

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
