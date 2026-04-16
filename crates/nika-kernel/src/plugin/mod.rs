// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Plugin traits — WASM host, sandboxing, observability.
//!
//! Future sub-crate: `nika-kernel-plugin` (when kernel exceeds 10k LOC or 50 traits).

pub mod sandbox;
mod wasm;

// Re-export wasm items at plugin level for backward compat.
// `nika_kernel::plugin::WasmPluginHost` etc.
pub use wasm::{
    PluginCallContext, PluginEnv, PluginFs, PluginHttp, TrapKind, WasmPluginError, WasmPluginHost,
    WasmPluginLifecycle,
};
