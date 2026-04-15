// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Sealed trait pattern to prevent external implementations.
//!
//! Traits bounded by `Sealed` can only be implemented within this crate
//! or crates that depend on `nika-kernel` and explicitly impl `Sealed`
//! for their types. This protects forward-compatibility for traits where
//! we may add required methods in future versions.
//!
//! ## Sealed traits (v0.81)
//!
//! - `Provider` (via the blanket super-trait, not the atomic sub-traits)
//! - `EventSink`
//! - `BillingSink`
//!
//! ## NOT sealed (intentionally)
//!
//! - `ProviderInfer`, `ProviderStream`, `ProviderMeta` — external crates
//!   can implement individual ISP traits, just not the combined `Provider`.
//! - `IdGenerator`, `SecretResolver`, `MetricsExporter`, `TracerProvider` —
//!   designed for external implementations (env-var resolver, `OTel` exporter,
//!   etc.).
//! - `WasmPluginHost`, `Sandbox` — open per ADR-020 (community WASM/sandbox
//!   backends).

/// Sealed supertrait. Only implementable within crates that can name this
/// type and explicitly impl it for their types.
///
/// This trait has no methods. It exists only as a permission gate.
pub trait Sealed {}

#[cfg(test)]
mod tests {
    use super::*;

    struct InternalType;
    impl Sealed for InternalType {}

    #[test]
    fn internal_types_can_impl_sealed() {
        fn _accepts(_: &dyn Sealed) {}
        _accepts(&InternalType);
    }
}
