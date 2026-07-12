// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-models` — the local-models unit (issue #146 · ADR-091/093).
//!
//! One crate, one law: the ONE canonical models dir
//! (`~/.nika/models/<owner>/<repo>/`). The DOWNLOADER ([`pull`]) and the
//! RESOLVER ([`store`] — what `nika model serve --model <id>` reads)
//! share the root by construction, so the brouillon-era pull/load
//! two-dir mismatch cannot re-happen. [`serve`] is the candle sidecar
//! launch glue the CLI adapts (feature-gated `local-infer` — the
//! default build's body teaches the recipe).
//!
//! Split from `nika-cli`'s `verbs/model` 2026-07-12 per D-2026-07-09-N1
//! (the 15k prod-LOC crate cap — the `nika-onboard`/`nika-display`
//! precedents): the composition root keeps thin `VerbOutput` adapters;
//! this member owns the logic and speaks plain `Result<String, String>`
//! (`Ok` = receipt · `Err` = an environment-class refusal that teaches
//! its fix).

// Test fixtures expect/unwrap freely — the nika-http/nika-cli idiom.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod gguf;
pub mod pull;
pub mod serve;
pub mod store;

/// The one GGUF family the v1 serve loader reads (mirrors the loader's
/// own `SUPPORTED_ARCH` validation in `nika-infer-local`).
pub const SERVE_FAMILY: &str = "qwen3";

/// The canonical models dir's presence facts — what `nika doctor`'s
/// models row reads (observed once, so the diagnosis stays pure).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModelsProbe {
    /// The dir (`None` when HOME/USERPROFILE cannot resolve).
    pub root: Option<String>,
    /// GGUFs on disk under it.
    pub count: usize,
    /// Their cumulative bytes.
    pub bytes: u64,
}

/// Observe the ONE models dir (count + bytes · `~/.nika/models`).
#[must_use]
pub fn models_probe() -> ModelsProbe {
    match store::models_root() {
        Ok(root) => {
            let items = store::installed(&root);
            ModelsProbe {
                root: Some(root.display().to_string()),
                count: items.len(),
                bytes: items.iter().map(|m| m.size).sum(),
            }
        }
        Err(_) => ModelsProbe::default(),
    }
}
