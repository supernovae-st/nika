// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! WASM plugin host traits — reserved for v0.100.
//!
//! These stubs define the trait shape for the future WASM sandbox.
//! No implementations exist until `nika-wasm-host` ships.
//! See `docs/architecture/forward-compat-invariants.md` Pattern 1.

use std::sync::Arc;

use crate::cancel::CancelCtx;
use crate::sandbox::DenialKind;
use nika_error::trust::TrustLevel;

/// Host-side WASM plugin execution.
///
/// Reserved for v0.100. Implementations will live in `nika-wasm-host`.
#[trait_variant::make(WasmPluginHostDyn: Send)]
pub trait WasmPluginHost: Send + Sync {
    /// Execute a WASM plugin by name with the given input.
    ///
    /// Returns serialized output. Shape TBD when `nika-wasm-host` ships.
    ///
    /// CANCEL SAFETY: cancel-safe via fuel metering. The v0.100 host MUST
    /// terminate the guest Store when the future is dropped (wasmtime's
    /// Store drop tears down all WASM memory + instance state). Partial
    /// host-side effects (file writes via capabilities) follow the
    /// per-capability rules documented by `PluginEnv`/`PluginFs`.
    async fn call_plugin(
        &self,
        plugin_name: &str,
        input: &[u8],
    ) -> Result<Vec<u8>, WasmPluginError>;
}

/// Plugin lifecycle management.
///
/// Reserved for v0.100. Real implementations will manage WASM modules
/// loaded into the host. Current stubs signal "unsupported" by default.
#[trait_variant::make(WasmPluginLifecycleDyn: Send)]
pub trait WasmPluginLifecycle: Send + Sync {
    /// Load a plugin from bytes. Returns an error until v0.100 ships.
    ///
    /// CANCEL SAFETY: cancel-safe. Drop during compilation discards the
    /// half-built Module; name registration happens atomically at the
    /// end of compilation, never mid-way.
    async fn load_plugin(&self, name: &str, bytes: &[u8]) -> Result<(), WasmPluginError>;

    /// Unload a previously loaded plugin. Returns an error until v0.100 ships.
    ///
    /// CANCEL SAFETY: cancel-safe — idempotent name-keyed removal.
    async fn unload_plugin(&self, name: &str) -> Result<(), WasmPluginError>;

    /// List loaded plugins. Returns empty until v0.100 ships.
    ///
    /// CANCEL SAFETY: cancel-safe — read-only registry enumeration.
    async fn list_plugins(&self) -> Result<Vec<String>, WasmPluginError>;
}

/// Capability-scoped environment access for WASM guests.
///
/// Reserved for v0.100. Implementations will scope env access per plugin
/// capabilities (similar to [`crate::sandbox::Capability`]).
#[trait_variant::make(PluginEnvDyn: Send)]
pub trait PluginEnv: Send + Sync {
    /// Read an environment variable scoped to sandbox capabilities.
    ///
    /// CANCEL SAFETY: cancel-safe (read-only).
    async fn env_get(&self, key: &str) -> Result<Option<String>, WasmPluginError>;
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

    /// Plugin exhausted its fuel budget before returning.
    ///
    /// Reserved at v0.81 for v0.100 wasmtime fuel metering. Fuel is a
    /// host-controlled compute budget that prevents infinite loops and
    /// cpu-DoS: the guest consumes fuel units per wasm op, the host
    /// traps when `consumed >= budget`. See ADR-032 seed.
    #[error("wasm plugin out of fuel: consumed {consumed} of {budget}")]
    OutOfFuel {
        /// Fuel units actually consumed by the guest.
        consumed: u64,
        /// Fuel budget the guest was granted.
        budget: u64,
    },

    /// Plugin hit a WASM-level trap.
    ///
    /// Reserved at v0.81 for v0.100 wasmtime integration. A trap is a
    /// guest-level abort (unreachable, integer-divide-zero, stack
    /// overflow, etc.). `kind` captures the structured trap class so
    /// hosts can distinguish guest bugs from host errors.
    #[error("wasm plugin trap: {kind}")]
    Trap {
        /// Structured trap classification.
        kind: TrapKind,
    },
}

/// Classification of a WASM-level trap.
///
/// Reserved at v0.81 for v0.100 wasmtime integration. Mirrors wasmtime's
/// `TrapCode` taxonomy at the nika layer so we can evolve the shape
/// without a direct wasmtime dep in L0.5.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TrapKind {
    /// Guest hit an `unreachable` instruction.
    Unreachable,
    /// Guest attempted integer divide-by-zero.
    IntegerDivisionByZero,
    /// Guest attempted an invalid integer-to-float conversion.
    InvalidConversionToInteger,
    /// Guest attempted out-of-bounds memory access.
    MemoryOutOfBounds,
    /// Guest attempted out-of-bounds table access (indirect call).
    TableOutOfBounds,
    /// Guest exceeded the configured stack depth.
    StackOverflow,
    /// Guest called an indirect function with mismatched signature.
    IndirectCallTypeMismatch,
    /// Host-side interruption (not guest-originated).
    Interrupted,
}

impl core::fmt::Display for TrapKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            Self::Unreachable => "unreachable instruction",
            Self::IntegerDivisionByZero => "integer divide by zero",
            Self::InvalidConversionToInteger => "invalid conversion to integer",
            Self::MemoryOutOfBounds => "memory access out of bounds",
            Self::TableOutOfBounds => "table access out of bounds",
            Self::StackOverflow => "stack overflow",
            Self::IndirectCallTypeMismatch => "indirect call type mismatch",
            Self::Interrupted => "host-side interruption",
        };
        f.write_str(s)
    }
}

/// Per-call context passed into `WasmPluginHost::call_plugin` at v0.100.
///
/// Reserved at v0.81 as an additive forward-compat seam: the call signature
/// stays stable (plugin name + input bytes) until v0.100, then this context
/// is threaded through the dyn trait via the `trait_variant` companion. ADR-032
/// captures the full lifecycle (fuel decrement, trust demotion, cancel token
/// wiring, timeout enforcement).
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct PluginCallContext {
    /// Trust level granted to the plugin's OUTPUT. Lower than
    /// `input_trust` by host policy — plugin output is always
    /// "untrusted data" regardless of what it was given.
    pub trust: TrustLevel,
    /// Trust level of the INPUT bytes (propagated from the verb that
    /// constructed the call).
    pub input_trust: TrustLevel,
    /// Cancellation token — hosts MUST check before each wasm-side
    /// resource allocation and on fuel-budget exhaustion.
    pub cancel: Arc<CancelCtx>,
    /// Fuel budget in wasmtime fuel units. None = no metering.
    pub fuel_budget: Option<u64>,
    /// Wall-clock timeout in milliseconds. None = no wall timeout.
    pub wall_timeout_ms: Option<u64>,
}

impl PluginCallContext {
    /// New context with the untrusted-output / untrusted-input default.
    ///
    /// Hosts that have more information should call
    /// [`Self::with_trust`] or mutate the fields directly.
    #[must_use]
    pub fn new(cancel: Arc<CancelCtx>) -> Self {
        Self {
            trust: TrustLevel::UNTRUSTED,
            input_trust: TrustLevel::UNTRUSTED,
            cancel,
            fuel_budget: None,
            wall_timeout_ms: None,
        }
    }

    /// New context with explicit trust taint on both input and output.
    #[must_use]
    pub fn with_trust(
        cancel: Arc<CancelCtx>,
        input_trust: TrustLevel,
        output_trust: TrustLevel,
    ) -> Self {
        Self {
            trust: output_trust,
            input_trust,
            cancel,
            fuel_budget: None,
            wall_timeout_ms: None,
        }
    }
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

    #[test]
    fn wasm_plugin_error_out_of_fuel_display() {
        let err = WasmPluginError::OutOfFuel {
            consumed: 1_000_000,
            budget: 500_000,
        };
        let s = err.to_string();
        assert!(s.contains("out of fuel"), "got: {s}");
        assert!(s.contains("1000000") && s.contains("500000"), "got: {s}");
    }

    #[test]
    fn wasm_plugin_error_trap_display_names_kind() {
        for (kind, needle) in [
            (TrapKind::Unreachable, "unreachable"),
            (TrapKind::IntegerDivisionByZero, "divide by zero"),
            (TrapKind::MemoryOutOfBounds, "memory access"),
            (TrapKind::TableOutOfBounds, "table access"),
            (TrapKind::StackOverflow, "stack overflow"),
            (TrapKind::IndirectCallTypeMismatch, "indirect call"),
            (TrapKind::Interrupted, "host-side"),
            (
                TrapKind::InvalidConversionToInteger,
                "conversion to integer",
            ),
        ] {
            let s = WasmPluginError::Trap { kind }.to_string();
            assert!(s.contains(needle), "{kind:?} missing `{needle}` in {s}");
        }
    }

    #[test]
    fn plugin_call_context_new_defaults_untrusted() {
        let cancel = Arc::new(CancelCtx::new());
        let ctx = PluginCallContext::new(cancel);
        assert_eq!(ctx.trust, TrustLevel::UNTRUSTED);
        assert_eq!(ctx.input_trust, TrustLevel::UNTRUSTED);
        assert!(ctx.fuel_budget.is_none());
        assert!(ctx.wall_timeout_ms.is_none());
    }

    #[test]
    fn plugin_call_context_with_trust_splits_axes() {
        let cancel = Arc::new(CancelCtx::new());
        let ctx = PluginCallContext::with_trust(cancel, TrustLevel::TRUSTED, TrustLevel::UNTRUSTED);
        assert_eq!(ctx.input_trust, TrustLevel::TRUSTED);
        assert_eq!(ctx.trust, TrustLevel::UNTRUSTED);
    }

    // ── Seam 4-5 trait shape tests ──────────────────────────────────
    // Verify trait shapes compile + are object-safe.

    fn _assert_lifecycle_trait<T: WasmPluginLifecycle>() {}
    fn _assert_plugin_env_trait<T: PluginEnv>() {}

    #[test]
    fn lifecycle_and_env_traits_are_send_sync() {
        // Compile-time check: the trait bounds include Send + Sync.
        fn _check<T: WasmPluginLifecycle + Send + Sync>() {}
        fn _check_env<T: PluginEnv + Send + Sync>() {}
    }
}
