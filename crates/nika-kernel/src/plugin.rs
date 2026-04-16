// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! WASM plugin host traits — reserved for v0.100.
//!
//! These stubs define the trait shape for the future WASM sandbox.
//! No implementations exist until `nika-wasm-host` ships.
//! See `docs/architecture/forward-compat-invariants.md` Pattern 1.

use crate::sandbox::DenialKind;

/// Host-side WASM plugin execution.
///
/// Reserved for v0.100. Implementations will live in `nika-wasm-host`.
#[trait_variant::make(WasmPluginHostDyn: Send)]
pub trait WasmPluginHost: Send + Sync {
    /// Execute a WASM plugin by name with the given input.
    ///
    /// Returns serialized output. Shape TBD when `nika-wasm-host` ships.
    async fn call_plugin(
        &self,
        plugin_name: &str,
        input: &[u8],
    ) -> Result<Vec<u8>, WasmPluginError>;
}

/// Filesystem access for WASM plugins (sandboxed).
///
/// Reserved for v0.100. Grants limited fs to WASM guests.
pub trait PluginFs: Send + Sync {
    /// Read a file within the plugin sandbox.
    ///
    /// # Errors
    ///
    /// Returns [`WasmPluginError::SandboxViolation`] if the path escapes
    /// the granted capability set.
    fn read_sandboxed(&self, path: &str) -> Result<Vec<u8>, WasmPluginError>;
}

/// HTTP access for WASM plugins (sandboxed).
///
/// Reserved for v0.100. Grants limited HTTP to WASM guests.
pub trait PluginHttp: Send + Sync {
    /// Fetch a URL within the plugin sandbox (allowlist enforced).
    ///
    /// # Errors
    ///
    /// Returns [`WasmPluginError::SandboxViolation`] if the URL is not on
    /// the allowlist.
    fn fetch_sandboxed(&self, url: &str) -> Result<Vec<u8>, WasmPluginError>;
}

/// WASM plugin errors.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum WasmPluginError {
    /// Plugin not found.
    #[error("wasm plugin not found: {name}")]
    NotFound {
        /// Plugin name.
        name: String,
    },

    /// Plugin execution failed.
    ///
    /// **SECURITY**: `reason` is a host-internal diagnostic. Hosts MUST
    /// NOT propagate this string to the WASM guest (trap message, return
    /// value, or any guest-visible surface) — it can carry allowlist
    /// paths, backend names, or other host internals. Guest-visible
    /// failure signalling should be structured (see `DenialKind` for the
    /// sandbox-side pattern). Future work: split into
    /// `Trap`/`OutOfMemory`/`HostError` variants per ADR-020 addendum.
    #[error("wasm plugin execution failed: {reason}")]
    ExecutionFailed {
        /// Host-internal failure description. Never forward to guest.
        reason: String,
    },

    /// Sandbox violation (capability denied).
    #[error("wasm sandbox violation: {kind}")]
    SandboxViolation {
        /// Structured denial class (no free-form text; see `DenialKind`).
        kind: DenialKind,
    },

    /// Plugin timed out.
    #[error("wasm plugin timed out after {timeout_ms}ms")]
    Timeout {
        /// Timeout in milliseconds.
        timeout_ms: u64,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn wasm_plugin_error_is_send_sync() {
        _assert_send_sync::<WasmPluginError>();
    }

    #[test]
    fn wasm_plugin_error_display() {
        let err = WasmPluginError::NotFound {
            name: "my-plugin".into(),
        };
        assert_eq!(err.to_string(), "wasm plugin not found: my-plugin");

        let err = WasmPluginError::Timeout { timeout_ms: 5000 };
        assert!(err.to_string().contains("5000"));
    }

    #[test]
    fn wasm_plugin_error_sandbox_violation_display() {
        let err = WasmPluginError::SandboxViolation {
            kind: DenialKind::FsReadNotGranted,
        };
        let s = err.to_string();
        assert!(s.contains("sandbox violation"));
        assert!(s.contains("filesystem read not granted"));
    }

    #[test]
    fn wasm_plugin_error_execution_failed_display() {
        let err = WasmPluginError::ExecutionFailed {
            reason: "oom".into(),
        };
        assert!(err.to_string().contains("oom"));
    }
}
