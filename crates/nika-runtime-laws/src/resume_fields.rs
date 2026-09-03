// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The resume projection's payload field names (ADR-127): the secret
//! custody reads them, so they live with the laws; the runtime re-exports
//! them at `nika_runtime::resume::fields`.

/// The additive `task_completed` / `task_cache_hit` trace field names
/// (ADR-099 · the compatibility surface: these evolve additively).
pub mod fields {
    /// The task-definition hash (blake3 hex over the JCS definition payload).
    pub const DEF_HASH: &str = "def_hash";
    /// The resolved-input hash (blake3 hex over the JCS input payload).
    pub const INPUT_HASH: &str = "input_hash";
    /// The task's output as ONE compact JSON text (rehydration source).
    pub const OUTPUT: &str = "output";
}
