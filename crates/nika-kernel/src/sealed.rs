// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Sealed trait pattern — opt-in participation gate.
//!
//! `Sealed` is re-exported as `pub`, so any crate that depends on
//! `nika-kernel` *can* still impl it for its own types (including
//! workspace-internal mocks such as `nika-kernel-mock`). This is a
//! **soft seal**: it prevents accidental impls (a downstream crate
//! cannot provide `Provider` / `EventSink` / `BillingSink` /
//! `SecretResolver` by mistake) and documents which traits are part
//! of the stable contract vs. ISP sub-traits open to third parties.
//!
//! It is **NOT** a hard barrier against adversarial in-tree crates
//! — any code in the workspace can write `impl Sealed for MyType {}`.
//! Hard sealing (private-module trait, crate-private subtrait
//! pattern) is tracked for v0.85+ in ADR-014's follow-up addendum.
//!
//! ## Sealed traits (v0.81)
//!
//! - `Provider` (via the blanket super-trait, not the atomic sub-traits)
//! - `EventSink`
//! - `BillingSink`
//! - `SecretResolver` (added Wave 3, P1-1 fix)
//!
//! ## NOT sealed (intentionally)
//!
//! - `ProviderInfer`, `ProviderStream`, `ProviderMeta` — external crates
//!   can implement individual ISP traits, just not the combined `Provider`.
//! - `IdGenerator`, `MetricsExporter`, `TracerProvider` —
//!   designed for external implementations (env-var resolver, `OTel` exporter,
//!   etc.).
//! - `WasmPluginHost`, `Sandbox` — open per ADR-020 (community WASM/sandbox
//!   backends).

/// Sealed supertrait — opt-in participation gate (soft seal).
///
/// Any crate that can name `nika_kernel::sealed::Sealed` may impl it
/// for its own types. This is enough to keep accidental third-party
/// impls out while letting workspace-internal mocks participate. Hard
/// sealing is deferred to v0.85+ (ADR-014 addendum).
///
/// This trait has no methods.
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
