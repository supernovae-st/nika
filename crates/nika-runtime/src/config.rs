// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Composer-owned execution knobs (spec §2) — split out of `lib.rs` under
//! the ADR-023 1,500-LOC ceiling (the sandbox-root field pushed the root
//! module over).

use std::num::NonZeroUsize;

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RuntimeConfig {
    /// Per-wave in-flight cap (`for_each` has its own `max_parallel`).
    /// `None` = wave-width (every wave member in flight at once).
    pub wave_parallelism: Option<NonZeroUsize>,
    /// Seed for the retry full-jitter PRNG — pure splitmix64 over
    /// `(seed, task, attempt)` · replay-stable by construction.
    pub jitter_seed: u64,
    /// Operator run budget (`--max-cost-usd`) over METERED spend. Once
    /// crossed, the run stops admitting new work: in-flight tasks
    /// complete and count, unstarted ones cancel, the run fails with
    /// NIKA-1704. `None` = no budget (the default). Unmetered work
    /// (local · mock · unpriced) can never trip it — the budget bounds
    /// what the ledger can SEE, said loudly at the preflight.
    pub max_cost_usd: Option<f64>,
    /// The root relative `permits:` globs anchor at for the exec sandbox's
    /// absolute grants (ADR-095 Layer 6 — the SAME root the builtin
    /// `FsBoundary` canonicalizes against, so check≡run≡jail cannot drift).
    /// `None` = the process cwd at dispatch time.
    pub sandbox_root: Option<std::path::PathBuf>,
}

impl RuntimeConfig {
    /// Construct (INV-019 · `new()` on every `#[non_exhaustive]` struct).
    #[must_use]
    pub fn new(wave_parallelism: Option<NonZeroUsize>, jitter_seed: u64) -> Self {
        Self {
            wave_parallelism,
            jitter_seed,
            max_cost_usd: None,
            sandbox_root: None,
        }
    }

    /// Attach an operator run budget (builder — `new()` stays stable).
    #[must_use]
    pub fn with_max_cost_usd(mut self, budget: Option<f64>) -> Self {
        self.max_cost_usd = budget;
        self
    }

    /// Pin the sandbox grants root (builder — the composer sets the launch
    /// cwd so a `cd` mid-process never re-anchors a boundary).
    #[must_use]
    pub fn with_sandbox_root(mut self, root: std::path::PathBuf) -> Self {
        self.sandbox_root = Some(root);
        self
    }
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self::new(None, 0)
    }
}
