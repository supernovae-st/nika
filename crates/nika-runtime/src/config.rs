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
    /// The OS command-sandbox backend the composer selected for this run
    /// (`seatbelt` · `landlock` · `noop` — the `CommandSandbox::backend`
    /// name). Journaled on `workflow_started` so the evidence pack reads
    /// the run's confinement mode from the journal itself; `None` = the
    /// composer did not say (older composers — the pack reports it as
    /// unrecorded, never guessed).
    pub sandbox_backend: Option<String>,
    /// The sandbox policy the composer judged under (`auto` · `require` ·
    /// `off` — #889 · the ONE knob). Journaled on `workflow_started` so
    /// the confinement mode reads WITH its posture; `None` = unrecorded
    /// (older composers), never guessed.
    pub sandbox_policy: Option<String>,
    /// True when this run proceeds UNCONFINED with `permits:` declared
    /// under `NIKA_SANDBOX=off` (#889) — the witnessed waiver: journaled
    /// on `workflow_started` so a sealed trace SHOWS the operator chose
    /// it. `false` in every other posture (nothing was waived).
    pub sandbox_waived: bool,
    /// The fingerprint of the project root this run belongs to (blake3 of
    /// the canonical root path · [`project_root_fingerprint`]): stamped on
    /// the opening frame so a trace can only be resumed from the project
    /// that wrote it (#1367).
    pub project_root_fingerprint: Option<String>,
}

/// The ONE fingerprint of a project root — blake3 of its canonical path —
/// computed by the composition root when it stamps a run and by the resume
/// door when it judges a trace. A root that cannot be canonicalized has no
/// fingerprint (an older trace's case: no claim, no refusal).
#[must_use]
pub fn project_root_fingerprint(root: &std::path::Path) -> Option<String> {
    let canonical = std::fs::canonicalize(root).ok()?;
    Some(
        blake3::hash(canonical.to_string_lossy().as_bytes())
            .to_hex()
            .to_string(),
    )
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
            sandbox_backend: None,
            sandbox_policy: None,
            sandbox_waived: false,
            project_root_fingerprint: None,
        }
    }

    /// Bind the run to `root` (the project the sandbox admits).
    #[must_use]
    pub fn with_project_root(mut self, root: &std::path::Path) -> Self {
        self.project_root_fingerprint = project_root_fingerprint(root);
        self
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

    /// Record the OS sandbox backend name for the journal (builder — the
    /// composer names the `CommandSandbox::backend` it selected so the
    /// evidence pack reads the run's confinement mode from the journal).
    #[must_use]
    pub fn with_sandbox_backend(mut self, backend: &str) -> Self {
        self.sandbox_backend = Some(backend.to_owned());
        self
    }

    /// Record the sandbox policy the composer judged under (builder —
    /// #889 · the journal attests the posture beside the backend).
    #[must_use]
    pub fn with_sandbox_policy(mut self, policy: &str) -> Self {
        self.sandbox_policy = Some(policy.to_owned());
        self
    }

    /// Mark the witnessed waiver (builder — #889: true ONLY when the run
    /// proceeds unconfined with `permits:` declared under
    /// `NIKA_SANDBOX=off`; the sealed trace shows the choice).
    #[must_use]
    pub fn with_sandbox_waived(mut self, waived: bool) -> Self {
        self.sandbox_waived = waived;
        self
    }
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self::new(None, 0)
    }
}
