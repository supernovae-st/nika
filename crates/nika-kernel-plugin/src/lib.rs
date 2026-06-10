// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-kernel-plugin` — the plugin sibling of the split kernel (L0.5).
//!
//! WASM host + sandbox contracts — 6 traits · all OPEN (unsealed) per
//! ADR-020 community-backend posture
//! (`docs/architecture/kernel-split-census-2026-06-10.md`).
//!
//! **Zero implementations live here.** Contracts only.
//!
//! Downstream crates import through the `nika-kernel` facade
//! (`nika_kernel::plugin::…` · `nika_kernel::Sandbox` · …) — depending
//! on this crate directly is reserved for kernel siblings.

#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::used_underscore_items,
        clippy::float_cmp,
        clippy::manual_string_new,
        clippy::unnecessary_literal_bound,
    )
)]

pub mod errors;
pub mod sandbox;
mod wasm;

// Re-export wasm items at crate level for backward compat —
// `nika_kernel::plugin::WasmPluginHost` etc. resolve through the hub
// facade onto these.
pub use wasm::{
    PluginCallContext, PluginEnv, PluginFs, PluginHttp, TrapKind, WasmPluginError, WasmPluginHost,
    WasmPluginLifecycle,
};
